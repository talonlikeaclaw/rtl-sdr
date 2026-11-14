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

## plane-stats future ideas (ranked by value-to-effort; none built yet)

1. **Aircraft profile page** per hex: visit history, all callsigns, records.
2. **Registration/type enrichment**: hex → registration/model/owner via bundled
   CSV lookup (opensky / adsbdb dumps). Offline-friendly, big UX win.
3. **Daily digest**: "today you saw N aircraft, M new, record range X nm" — page
   or Telegram/Discord push.
4. **Emergency squawk log**: persist 7500/7600/7700 + `emergency != none`.
5. **Range-over-time graph** from `poll_log` (already collected) to see
   antenna/autogain changes pay off.
6. **"New to me" alert feed**: first-time hexes with a dashboard toast.
7. **Feeder tie-in**: once ADSB Exchange/ADSB.fi feeding exists, show
   "planes only you saw" (MLAT/latitude badges).
8. **Airport lens**: filter by callsign prefix (e.g. all POE/TSC flights).
