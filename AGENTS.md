# AGENTS.md

Context an agent needs before touching this repo.

## Stack

- Pi 4 (arm64), one RTL-SDR Blog V4L dongle.
- Docker Compose project `name: sdr`.
- **Only one profile may be `up` at a time** — the dongle is exclusive.
- tar1090's `/var/globe_history` (heatmap, traces, coverage/radar outline) is
  the named volume `tar1090_globe_history`. Without it the data lives in the
  container layer and a recreate wipes the radar history.

## Driver gotcha

The upstream readsb image ships librtlsdr 2.0.1, which predates the V4L; its RF
input switch is never configured and 1090 MHz looks dead. `readsb-v4l/Dockerfile`
builds the RTL-SDR Blog fork and installs its `librtlsdr.so.0` over the image's,
at `/usr/local/lib/librtlsdr.so.0` (the SONAME readsb links against).
Regression symptom: no aircraft on the map, and no
`RTL-SDR Blog V4 Lite Detected` line in `docker logs readsb`.

Second, distinct symptom: `docker compose up -d` after a config change *recreates*
the container while the old readsb is still streaming, and the new instance can
open the dongle cleanly (log shows `V4 Lite Detected` + gain) yet decode zero
messages for minutes — a wedged USB endpoint, invisible in `dmesg`. Verify with
`curl -s localhost:8080/data/stats.json` (`last1min.messages` stays 0). Recovery:
`docker compose stop readsb`, run `rtl_test -t` once on the free device, start it
again. Plain `docker restart readsb` did not clear it.

## Config

- `.env` holds `TZ`, `READSB_LAT`, `READSB_LON`, `LAN_DNS` (the router,
  `192.168.0.1`).
- Every service pins `dns:` from the `x-dns` anchor (`${LAN_DNS}`, then
  `1.1.1.1`). Docker snapshots the host `/etc/resolv.conf` when a container
  starts, and this box's file is Tailscale-owned: `tailscaled` only writes
  `nameserver 100.100.100.100` after the tailnet is up, which is later than
  dockerd. Containers created right after a boot therefore got
  `NO EXTERNAL NAMESERVERS DEFINED` and every outbound request in
  `plane-stats`/`tar1090` failed with `error sending request for url (...)`
  until it was recreated. Never drop the `dns:` keys.
- `READSB_ENABLE_BIASTEE=on` — bias tee powered (antenna LNA). **Do not disable
  without asking the user.**
- `READSB_GAIN=autogain`.
- `tar1090` sets `READSB_ENABLE_TRACES=true`: readsb writes one gzipped
  per-airframe trace per day under `/var/globe_history/YYYY/MM/DD/traces/`.
  `MAX_GLOBE_HISTORY=14` bounds retention; the nightly tar1090 backup archive
  carries them to the NAS. Trace files are the only durable per-position
  history (plane-stats stores just `aircraft.last_lat`/`last_lon`).

## Verification

```bash
docker compose --profile adsb up -d
curl -s localhost:8080/data/aircraft.json | head -c 400   # live aircraft
docker logs readsb | grep -i "V4 Lite Detected"
```

## adsb.fi feeder

Host-level, not compose: adsb.fi has no Docker image. `feeders/adsbfi/` documents
it; the installer compiles a patched `readsb` (`feed-adsbfi`) and `mlat-client`
into `/usr/local/share/adsbfi`, creates the `adsbfi` system user, and installs
`adsbfi-feed.service` + `adsbfi-mlat.service`. Config is `/etc/default/adsbfi`.

- `feed-adsbfi` connects to `127.0.0.1:30005` (compose-published Beast output)
  and pushes reduced Beast to `feed.adsb.fi:30004`.
- `mlat-client` connects to the same `30005`, talks to `feed.adsb.fi:31090`, and
  injects results into `readsb` on `127.0.0.1:30104`. Compose therefore sets
  `READSB_NET_BEAST_INPUT_PORT=30104` and publishes `127.0.0.1:30104:30104`;
  without both, MLAT results never reach `tar1090`.
- `USER="0"` disables MLAT (ADS-B feed only). A real `USER` enables it and needs
  antenna lat/lon/altitude ASL accurate to ~15 m. `PRIVACY="--privacy"` hides
  the feeder name on the MLAT map.
- The feeder UUID is `/usr/local/share/adsbfi/adsbfi-uuid`; do not regenerate it
  casually or the site sees a new feeder.
- The services are independent of compose. They retry while the `adsb` profile
  is down (e.g. AIS running) and resume when it returns.

```bash
systemctl status adsbfi-feed adsbfi-mlat
ss -tn | grep -E '30004|31090'          # outbound to feed.adsb.fi
sudo journalctl -u adsbfi-feed -n 20 --no-pager
sudo journalctl -u adsbfi-mlat -n 20 --no-pager
```

Re-run `sudo bash /usr/local/share/adsbfi/git/update.sh` after config changes.
Uninstall with `sudo bash /usr/local/share/adsbfi/uninstall.sh`.

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
- `app.css` is `include_bytes!`'d into the binary and served `Cache-Control:
  no-cache`; the templates link `/app.css?v=2` to invalidate copies cached
  under the old one-day header. Bump the query only if a long-cached URL must
  be broken again.
- **Header nav** lives in `templates/partials/nav.html` (links in
  `nav_links.html`): a pill row above 860px, a native `<details>` sheet below,
  with both copies present and the inactive one hidden by `display` so the
  menu works without JS. Each page template carries an `active: &'static str`
  field (`"dashboard"`, `"today"`, …) selecting the `aria-current="page"`
  link; page-specific actions go under the tagline in `.page-actions`, never
  into the section nav.
- **Favicon:** `static/favicon.svg` is the aircraft mark on a night-ground
  tile and is the source; `static/favicon-32.png` is rasterized from it
  (`ffmpeg -i favicon.svg -vf scale=32:32 favicon-32.png`). `web.rs` serves
  both and answers `/favicon.ico` with the PNG so a browser probing the
  default path never gets the HTML error page.
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
registration/type enrichment, emergency squawk log, tar1090 map/trace deep
links, live "overhead now" panel (current alt/speed/climb/range/bearing/age from
the `last_*` columns; `/partials/live` + `/api/live`, 5 s htmx refresh; bearing
needs `RX_LAT`/`RX_LON`, wired from `READSB_LAT`/`READSB_LON` in compose), and
fleet pagination (100 rows/page, pager preserves `sort/dir/q`; hidden
`#fleet-sort`/`#fleet-dir` inputs keep the sort across the 10 s refresh, filter,
and search requests, while `#fleet-page` keeps the page across the 10 s refresh
only; `/api/aircraft` stays a full list), and fleet filter chips (category,
military, interesting, privacy PIA/LADD, emergency history, heard today, new
this week, not seen this week, regulars = 5+ visits, max-alt range;
`FleetFilter` in `stats.rs`, `filter_suffix` on sort/pager links, `db_flags`
stored from tar1090 `dbFlags`).

`db_flags` is stored from tar1090 `dbFlags`. The tar1090 service sets
`TAR1090_ENABLE_AC_DB=true`, which makes its internal readsb load
`aircraft.csv.gz` from tar1090-db and emit all four bits (military,
interesting, PIA, LADD). Without that file readsb suppresses `dbFlags`
entirely, even the military range bit (`json_out.c` gates the field on
`Modes.db`). The poller also derives the **military** bit from the ICAO ranges
in `plane-stats/src/mil.rs` (a copy of tar1090's `ranges.js` fallback, e.g.
RCAF `C2xxxx`) and ORs it into whatever `dbFlags` was sent, so military works
even if the DB download fails. The `mil_ranges_v1` key in `enrichment_meta`
guards a one-shot backfill of older rows; bump it if `mil::RANGES` changes.
`dbflags::backfill` also ORs the flags from the tar1090-db CSV baked into the
image (`TAR1090_DB_CSV`, downloaded in the Docker `csv` stage) into every
aircraft already in the ledger, so historical rows get interesting/PIA/LADD
without waiting to be re-seen. Its `enrichment_meta.key = 'tar1090_db'` stamp
is size+mtime; bump `CSV_VERSION` to refresh the CSV. The `coalesce` on upsert
preserves previously stored bits when tar1090 omits `dbFlags`.

Daily ledger (`day_aircraft`) and `/today`:

- The poller upserts one row per `(local day, hex)` in the same transaction as
  `aircraft`/`visits`: `first_ts`/`last_ts`, `polls`, and MAX
  `max_alt_ft`/`max_gs_kts`/`max_range_nm`. The day key is
  `stats::local_day(now)` derived from the poll timestamp, so a poll that
  straddles midnight is filed under the day it was taken.
- `today_summary`, `hour_histogram` (24 buckets from `poll_hour`), and
  `records_today` (today's best vs `day < today`) live in `stats.rs`.
  `new_today` returns the aircraft whose all-time `first_seen` is today.
- `/today` renders the digest; `/api/today` returns
  `{summary, hours, records, new}`. The index has a "heard today" card (from
  `Stats::heard_today`) and a Today pill.
- Data starts at deploy: there is no backfill from `visits`. `day_aircraft` is
  never pruned.
- `records_today` compares against prior days only; on the first day every
  record shows `prior: null` / "held", not "new record".
- `new` and `new_today` are scoped to today's ledger rows (joined to
  `aircraft.first_seen >= local midnight`), so `new <= unique`. On the deploy
  day they are lower than the "new today" card, which counts every aircraft
  first seen today regardless of ledger coverage.
- `poll_log.messages` is tar1090's *cumulative* counter, so `today_summary`
  sums the positive per-poll deltas (`LAG`) instead of `SUM(messages)`; never
  sum the raw column.

### Network integration

`src/network.rs` polls adsb.fi in the background; every page renders without it.

- v3 radius poll every 10 s (`NETWORK_POLL_SECS`), 100 NM
  (`NETWORK_RADIUS_NM`, clamp 10–250), 1 req/s public limit. `HEAD
  /v2/snapshot` every 35 s (405 falls back to a 5-minute GET), `/v1/stats`
  every 5 min, `/v1/coverage` every 10 min.
- `network_day_aircraft` skips `on_ground`; `network_enrichment` keeps ground
  contacts. Both start at deploy — no backfill from `visits`.
- `/coverage` diffs a 60 s rolling network set (`recent`) against local
  `aircraft.last_seen`, so one missed poll cannot create a false "unique".
- `FEED_JSON` (`/run/adsbfi-feed/aircraft.json`) is mounted read-only from the
  host feeder; the dashboard Feeder card is the only reader.
- Lock order: `network::latest()` before `db_lock`; `update_view` before the
  network DB write.

Activity (`/activity`, `/api/activity`):

- `weekday_hour_heatmap` groups the `poll_hour` rollup into 7×24 local
  weekday/hour cells with the current `TZ` offset applied in SQL. Grouping
  `poll_hour` is identical to grouping `poll_log` (it stores the same
  MAX-per-hour) with ~360× fewer rows, and its global max is the "busiest
  hour" card's value by construction.
- `range_by_day` is the best `max_range_nm` per `day_aircraft.day` for the last
  90 days (one point per day); `messages_by_day` sums the positive per-poll
  deltas of the cumulative counter for the last 30 days. `LineChart::build`
  scales either series into a fixed 640×120 SVG viewBox and the template
  inlines the path/area/dots.
- The heatmap is a CSS grid with `minmax(0, 1fr)` columns and
  `overflow: hidden`, so it never adds horizontal scroll on mobile.
- The `/today` histogram tooltip (`#hist-tip`) is a direct child of `<body>`,
  not of the panel: `.glass` uses `backdrop-filter`, which makes it the
  containing block for `position: fixed` descendants, so a tooltip inside the
  panel would be offset and stacked under later panels.

Type dex (`/types`, `/type/:code`, `/api/types`, `/api/type/:code`):

- `aircraft_types` holds one row per ICAO designator: Doc 8643 identity
  (`manufacturer`, `model`, `engine_type`, `engine_count`, `wtc`, `category`)
  plus the cached Wikipedia profile (`wiki_title`, `wiki_extract`, `wiki_image`,
  `wiki_url`) and Wikidata facts (`first_flight`, `total_produced`,
  `wingspan_m`). `type_wiki_map` is the curated designator → article table.
- `types::run` is a background task spawned next to the poller. It loads
  Doc 8643 (`POST https://doc8643.icao.int/External/AircraftTypes`; the
  endpoint needs `Content-Length: 0` or it answers 411), then the Wikipedia
  `List_of_aircraft_type_designators` wikitext, then profiles for types seen
  on the air. Doc 8643 is refreshed monthly, the designator map weekly, and a
  profile monthly; the task sleeps 1 h between cycles. Every failure is
  non-fatal: pages render with whatever is cached.
- Wikipedia requests are batched (20 titles per `pageimages|extracts|pageprops`
  query, 50 QIDs per `wbgetentities` query) and spaced 2 s apart. A 429 backs
  off 60 s and retries the batch three times; the task then stops until the
  next hourly cycle. Do not switch back to one request per type: the extracts
  API rate-limits quickly.
- The dex lists only designators actually heard. `caught` is distinct
  `aircraft.hex`; `registered` is `COUNT(*)` in `enrichment` for that
  `typecode` (the rarity denominator). `enrichment_typecode` makes that
  subquery cheap.
- `TypeDexRow` lives in `stats.rs`; `type_dex`, `type_detail`, and
  `type_dex_summary` back the pages. A designator is accepted only as 2–4
  alphanumerics (`stats::valid_designator`, shared by the route and the links),
  upper-cased before lookup.
- `/types?q=` filters the dex server-side via `TypeDexRow::matches`: a
  case-insensitive substring of designator, manufacturer, Doc 8643 model or
  Wikipedia title, so "C17", "Boeing" and "Globemaster" all find the C-17.
  The sort chips carry `q` (`q_suffix`), and the form keeps `sort` in a hidden
  input.
- Every designator the UI renders links to `/type/:code`: the fleet table, the
  live panel, the dashboard "types caught" card and recent arrivals, `/today`'s
  new list, and the aircraft profile (tagline, Model row, hero pill). Rows go
  through `AircraftRow::type_url()` / `Record::type_url()`, which return `None`
  for a free-text OpenSky `typecode` the route would 404. The type page links
  back to the fleet with `/?q=<code>` (the fleet search matches `typecode`).
- Photos are Wikipedia thumbnails (CC BY-SA; the card links the article). No
  Planespotters: its terms forbid storing/re-hosting images and it has no type
  endpoint.

Refresh model (do not "simplify" back to a whole-table tick):

- The fleet table's periodic refresh is 60 s, fetches `/partials/table/rows` and
  swaps only `#fleet-rows` (the `<tbody>`), so the table's horizontal scroll and
  the user's in-flight filters survive. It is suppressed while `#fleet-search`
  has focus and while the tab is hidden (`hx-trigger` filter).
- Every fleet control carries `hx-sync="#fleet-panel:replace"` (inherited from
  the section), so the newest request wins and stale responses cannot overwrite
  a fresh one. The tick overrides that with `queue last` so it never aborts a
  user request.
- Filter state travels in request bodies (`hx-include="#fleet-controls"`), not
  in generated `filter_suffix` query strings.
- The live panel has a **fixed** height of `16.25rem` on `.live-wrap`
  (header + 7 rows, the usual peak; the 8th scrolls) and server-renders
  `live_aircraft()` via the `live` template field — the include must never
  resolve to `rows` (the fleet page), or the first tick collapses the panel.
  Do not go back to `max-height`: the panel refreshes every 5 s and its row
  count changes, so a content-sized panel shifted every panel below it on each
  tick (iOS Safari has no scroll anchoring, so the page jumped while reading
  the fleet).
- `stats()` is cached for 10 s (`stats_cached`) because it is 16 all-time
  aggregates; `/api/stats` stays uncached.
- `poll_hour` is the hourly `MAX(aircraft_count)` rollup the cards read instead
  of grouping `poll_log`; the poller maintains it in the same transaction as
  `poll_log`, `prune` trims it, and `enrichment_meta.key = 'poll_hour_v1'`
  guards the one-shot backfill. Add a new key (v2) if the backfill must rerun.
- The three `aircraft ⋈ enrichment` aggregates use `CROSS JOIN` on purpose:
  with a plain `JOIN` SQLite scans all ~520k enrichment rows (`SCAN e`) instead
  of the ~350 aircraft rows (605 ms → 10 ms).

Country, polar coverage, records, schedule and routes (steps 9-14):

- `aircraft.country` is set from the ICAO 24-bit address blocks in
  `plane-stats/src/country.rs` (ICAO Annex 10 Table 9-1, generated once and
  committed). The poller writes it, `country_v1` in `enrichment_meta` guards the
  one-shot backfill, and the fleet table has a country select + column.
- `/countries` ranks every state of registry by airframes, with its share of
  the known ledger, distinct types, best range and last heard. Each row links
  to `/?country=XX`, the fleet filter. `/api/countries` takes the same
  `sort=count|name|recent|range` as the page and returns the rows.
- `range_sector` holds the best range per local day per 10° bearing sector
  (36 rows/day, never pruned). The poller writes it only when `RX_LAT`/`RX_LON`
  are set, excluding ground contacts; `/coverage` renders all-time vs last-30-day
  polygons and `/api/polar` returns the geometry. `rangebackfill::run` seeds it
  once from tar1090's gzipped heatmap (`GLOBE_HISTORY`, mounted read-only at
  `/globe_history`): 16-byte records of hex, lat/lon ×1e6, alt/gs; callsign
  metadata blocks and ground (`alt=-123`) are skipped. Stamp
  `range_sector_backfill_v1` in `enrichment_meta`; it never writes to tar1090.
- `/activity` adds unique-by-day (`day_aircraft`), new-by-day (`aircraft` full
  history) and network coverage % per day. Coverage is `null` when that day had
  no local polls, never 0%; the chart uses `LineChart::build_bounded(0, 100)`.
- `/records` and `/api/records` reuse `type_dex`, `records_by`, `visits`, and
  the enrichment year; no new tables.
- The aircraft profile has a 24-hour visit-start schedule, day streak (grace for
  a streak ending yesterday), and an adsbdb `origin → destination` row.
- `src/routes.rs` is memory-only by design: adsbdb's terms forbid persisting or
  exporting route data. Never add a routes table or crawl; `ROUTE_LOOKUP=off`
  disables requests.

Detailed step-by-step plan for the remaining work: `plane-stats/PLAN.md`.

Remaining ideas (ranked by value-to-effort):

1. **Daily digest push**: "today you saw N aircraft, M new, record range X nm"
   — the `/today` page exists; add Telegram/Discord delivery.
2. **Range-over-time graph** from `poll_log` (already collected) to see
   antenna/autogain changes pay off.
3. **"New to me" alert feed**: first-time hexes with a dashboard toast.
4. **Feeder tie-in**: once ADSB Exchange/ADSB.fi feeding exists, show
   "planes only you saw" (MLAT/latitude badges).
5. **Airport lens**: filter by callsign prefix (e.g. all POE/TSC flights).
