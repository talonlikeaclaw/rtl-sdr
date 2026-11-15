# AGENTS.md

Context an agent needs before touching this repo.

## Stack

- Pi 4 (arm64), one RTL-SDR Blog V4L dongle.
- Docker Compose project `name: sdr`.
- **Only one profile may be `up` at a time** — the dongle is exclusive.

## Driver gotcha

The upstream readsb image ships librtlsdr 2.0.1, which predates the V4L; its RF
input switch is never configured and 1090 MHz looks dead. `readsb-v4l/Dockerfile`
builds the RTL-SDR Blog fork and installs its `librtlsdr.so.0` over the image's,
at `/usr/local/lib/librtlsdr.so.0` (the SONAME readsb links against).
Regression symptom: no aircraft on the map, and no
`RTL-SDR Blog V4 Lite Detected` line in `docker logs readsb`.

## Config

- `.env` holds `TZ`, `READSB_LAT`, `READSB_LON`.
- `READSB_ENABLE_BIASTEE=on` — bias tee powered (antenna LNA). **Do not disable
  without asking the user.**
- `READSB_GAIN=autogain`.

## Verification

```bash
docker compose --profile adsb up -d
curl -s localhost:8080/data/aircraft.json | head -c 400   # live aircraft
docker logs readsb | grep -i "V4 Lite Detected"
```

## Conventions

- One compose profile per future app (ais/sat/acars/pocsag/sondes/feeders per
  the README roadmap); only one profile up at a time.
- Third-party images pinned `:latest` from sdr-enthusiasts.
- Custom code lives in its own directory + service (pattern: `plane-stats/`).

## plane-stats service

Rust crate in `plane-stats/` (axum + tokio + rusqlite bundled + askama templates,
htmx vendored in-binary). Read-only ledger of every aircraft ever seen:
first/last seen, "polls seen" (≈ airtime in 10 s units), max alt/speed/range,
strongest signal. Polls `http://tar1090:80/data/aircraft.json` every 10 s,
upserts into SQLite (WAL) at `DB_PATH` (default `/data/planes.db`, named volume
`plane_stats_db`). Web UI + JSON API on port **8081**; only the poller writes.

Rebuild after code changes:

```bash
docker compose --profile adsb build plane-stats
```

### Gotchas

- **`cargo` works on the host** (Nix-provided, 1.99.x). `cargo check`/`build`
  compile locally; the Nix wrapper injects `rustc`, which is not on `PATH`
  directly. `clippy`/`rustfmt` are not installed — add them from the same
  nixpkgs pin if needed. A local `target/` is ~1.5–2 GB. Docker is still the
  deployment path (`docker compose --profile adsb build plane-stats`, ~4 min).
- **`Cargo.lock`** is present and `plane-stats/Dockerfile` copies it next to
  `Cargo.toml`. Commit it: a binary crate should pin dependency versions.
- **Enrichment reload is guarded.** The OpenSky CSV (~94 MB, ~520k rows) is
  re-inserted on boot only when its size+mtime differs from the stamp in
  `enrichment_meta`. First boot after an image/CSV change takes ~1 min; later
  boots are seconds. The CSV is downloaded in a separate `csv` Docker stage, so
  source rebuilds reuse it and the stamp stays valid. Bump `CSV_VERSION`
  (`docker compose build --build-arg CSV_VERSION=2 plane-stats`) to force a
  fresh download. Force a reload by clearing the stamp:
  `DELETE FROM enrichment_meta;` (no `sqlite3` in the image — use a throwaway
  container).
- **DB volume** is `sdr_plane_stats_db`. Back up before schema steps:
  `docker run --rm -v sdr_plane_stats_db:/data -v "$PWD":/backup alpine tar czf /backup/planes.tgz -C /data .`
  A nightly DB copy also lands in `/mnt/truenas/talon/pi-backups/daily/`
  (03:00 timer, `/usr/local/bin/pi-backup.sh`).
- **Adding columns:** `aircraft_sorted` reads `COLS` positionally via
  `row_to_aircraft`. Append new fields to the end of `COLS`, `AircraftRow`, and
  `row_to_aircraft`; never insert mid-list.
- **tar1090 deep links:** `?icao=<hex>` selects an aircraft, `?showTrace=<hex>`
  shows its track. Base URL is `TAR1090_URL` if set, else derived from the
  request `Host` with the port swapped to 8080. Per-aircraft links render only
  while `AircraftRow::is_live()` is true (`last_seen` within
  `LIVE_WINDOW_SECS`, 60 s); the page header always links to the bare map. Do
  **not** add `hideSidebar`: tar1090 persists it in its own localStorage (a
  permanent layout change for the user), it does not hide the
  selected-infoblock panel, and tar1090 strips consumed params from the URL.
- **tar1090 `dbFlags`:** bit 0 = military, 1 = interesting, 2 = PIA, 3 = LADD.
- `aircraft.json` per-aircraft fields worth knowing: `r_dst` (range nm), `seen`
  (message age s), `seen_pos`, `dbFlags`, `category`. The poller keeps only
  `seen <= 60`.
- Day/hour bucketing uses `chrono::Local` (container `TZ`). `poll_log` is pruned
  at 90 days; `aircraft` and `visits` are never pruned.
- Startup order: enrichment load → HTTP server; the poller is spawned inside the
  runtime, so the UI is up before the first poll.

## plane-stats status

Built: aircraft profile page (visits, callsigns, records), OpenSky
registration/type enrichment, emergency squawk log, tar1090 map/trace deep links.

Detailed step-by-step plan for the remaining work: `plane-stats/PLAN.md`.

Remaining ideas (ranked by value-to-effort):

1. **Daily digest**: "today you saw N aircraft, M new, record range X nm" — page
   or Telegram/Discord push.
2. **Range-over-time graph** from `poll_log` (already collected) to see
   antenna/autogain changes pay off.
3. **"New to me" alert feed**: first-time hexes with a dashboard toast.
4. **Feeder tie-in**: once ADSB Exchange/ADSB.fi feeding exists, show
   "planes only you saw" (MLAT/latitude badges).
5. **Airport lens**: filter by callsign prefix (e.g. all POE/TSC flights).
