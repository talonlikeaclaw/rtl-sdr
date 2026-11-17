# plane-stats roadmap

Step-by-step plan for the next features. Read `AGENTS.md` first; it has the
build loop, schema rules, and tar1090 quirks.

## How to use this file

- **One step per change set.** Build, verify, then stop for a manual commit.
  Do not bundle steps.
- Update the progress table when a step lands.
- Every schema change is additive (`ALTER TABLE ... ADD COLUMN` in
  `main.rs` `NEW_COLUMNS`, or `CREATE TABLE IF NOT EXISTS`). No destructive
  migrations.
- Take a backup before a schema step:
  `sudo /usr/local/bin/pi-backup.sh` (or wait for the 03:00 timer).

## Current state

Built and working:

- Aircraft profile page (visits, callsigns, records, enrichment).
- OpenSky registration/type enrichment.
- Emergency squawk log.
- tar1090 deep links: `tar1090_base()` in `web.rs` (`TAR1090_URL` env, else
  request `Host` :8080), `AircraftRow::map_url()` / `trace_url()`, `Map` column
  in the fleet table, map/trace pills on the profile.
- Live "overhead now" panel: current alt/speed/climb/range/bearing/age from the
  `last_*` columns, `/partials/live` + `/api/live`, 5 s htmx refresh.
- Fleet pagination: `aircraft_sorted(..., limit, offset)` + `aircraft_count`,
  100 rows/page, pager preserving `sort/dir/q`, page kept across the 10 s
  refresh (`#fleet-controls` form + hidden `page`). `/api/aircraft` stays a
  full list.
- Fleet filter chips: `FleetFilter` in `stats.rs` (search, `cat`, `mil`,
  `today`, `new7`, `alt_min`/`alt_max`) compiles to a named-param `WHERE`;
  chip row in `#fleet-controls`, carried to the sort headers and pager by
  `hx-include="#fleet-controls"` (no generated query suffix), and
  `db_flags` stored from tar1090 `dbFlags` (bit 0 = military).
- Daily ledger + `/today` digest: `day_aircraft` (one row per local day per
  hex, MAX alt/speed/range, `polls`, first/last ts) is upserted by the poller
  in the same transaction as `aircraft`/`visits`; `today_summary`,
  `hour_histogram`, `records_today`, `new_today` in `stats.rs`; `/today` page
  + `/api/today`; index gets a "heard today" card and a Today pill. Data
  starts at deploy — there is no backfill from `visits`; `new` is scoped to
  today's ledger rows so it never exceeds `unique`.
- Activity page: `weekday_hour_heatmap` (7×24 peak aircraft from `poll_hour`,
  local offset applied in SQL), `range_by_day` (best range per local day from
  `day_aircraft`, 90 days), `messages_by_day` (positive deltas of tar1090's
  cumulative counter, 30 days); `/activity` renders a CSS-grid heatmap and two
  inline SVG line charts, `/api/activity` returns the raw series. The busiest
  heatmap cell equals the "busiest hour" card by construction.
- Type dex: `/types` card grid and `/type/:code` profile with a Wikipedia
  photo + extract, ICAO Doc 8643 specs, Wikidata facts, and local catch stats
  (caught / OpenSky-registered, first/last seen, records, airframes,
  operators). `aircraft_types` + `type_wiki_map`; `types::run` refreshes Doc
  8643 monthly, the Wikipedia designator map weekly, and profiles monthly in
  batched requests. `/api/types`, `/api/type/:code`.
- Type links: every rendered designator (fleet, live panel, dashboard cards
  and recent arrivals, `/today`, aircraft profile) links to `/type/:code`;
  the type page links back to `/?q=<code>`.

Refresh model (60 s tbody-only tick, `hx-sync`, `poll_hour` rollup, cached
`stats()`) is documented in `AGENTS.md` under "plane-stats status".

Known loose ends:

- `plane-stats/Cargo.lock` is present and `plane-stats/Dockerfile` copies it;
  commit it so image builds stay reproducible.
- `enrichment_meta` stamps the CSV; delete its row to force a reload. The CSV
  lives in a separate `csv` Docker stage, so source rebuilds reuse it. Bump
  `CSV_VERSION` to force a fresh download.

## Build / verify loop

```bash
docker compose --profile adsb build plane-stats   # ~4 min on the Pi
docker compose --profile adsb up -d plane-stats
docker logs --tail 20 plane-stats
```

`cargo check` also works on the host (Nix-provided cargo/rustc). Enrichment
reload is skipped when the CSV is unchanged, so restarts are seconds.

---

## Step 2 — Live "overhead now" panel

**Goal:** a 5 s-refreshing board of currently airborne aircraft with current
altitude, speed, range, bearing, climb, and message age.

**Files:** `main.rs`, `poller.rs`, `stats.rs`, `web.rs`, `templates/index.html`,
new `templates/partials/live.html`, `docker-compose.yml`.

**Schema (append to `NEW_COLUMNS`):**

```rust
("aircraft", "last_alt_ft", "INTEGER"),
("aircraft", "last_gs_kts", "REAL"),
("aircraft", "last_range_nm", "REAL"),
("aircraft", "last_baro_rate", "REAL"),
("aircraft", "last_msg_age", "REAL"),
```

**Poller:** add the columns to the INSERT and `ON CONFLICT SET`. Reuse existing
placeholders (`?6` alt, `?7` gs, `?8` r_dst, `?13` baro_rate) plus one new
placeholder for `a.seen`. `Alt::Ground` → alt `0`; missing → `NULL`.

**Stats:** append the fields to `COLS`, `AircraftRow`, `row_to_aircraft`
(append only — indices are positional). Add:

```sql
WHERE a.last_seen > :cutoff
ORDER BY a.last_range_nm IS NULL, a.last_range_nm ASC
LIMIT 50
```

Plus `bearing_deg(rx_lat, rx_lon, lat, lon)`.

**Compose:** `RX_LAT: ${READSB_LAT}`, `RX_LON: ${READSB_LON}`.

**Web:** `/partials/live`, `/api/live`; panel above the fleet with
`hx-trigger="every 5s"`. Columns: callsign, type, alt, speed, climb, range,
bearing, age, map link.

**Verify:** `curl -s localhost:8081/api/live | python3 -m json.tool`. Alt/speed/
range match tar1090 for the same hex; age is message age (`a.seen`), not poll
age; rows leave after 60 s.

**Commit:** `plane-stats: live overhead panel with current state`

---

## Step 3 — Fleet pagination

**Goal:** stop rendering every aircraft on each 10 s tick.

**Files:** `stats.rs`, `web.rs`, `partials/table.html`, `index.html`.

- `aircraft_sorted(..., limit, offset)` + `aircraft_count(db, search)`.
- `SortParams.page`, page size 100, clamped. Search/sort reset to page 1.
- Wrap search + hidden `page` in `<form id="fleet-controls">`; the 10 s refresh
  uses `hx-include="#fleet-controls"` so it keeps the page.
- Pager: `1–100 of N`, prev/next preserving `sort/dir/q`.

**Verify:** page boundaries, last-page remainder, search from page 5 → page 1,
stable ordering across pages, `?page=2` differs.

**Commit:** `plane-stats: paginate fleet table`

---

## Step 4 — Filter chips

**Goal:** category / military / interesting / privacy / emergency-history /
seen-today / new-this-week / dormant / regulars / altitude filters that compose
with search and sort.

**Files:** `main.rs`, `poller.rs`, `stats.rs`, `web.rs`, `index.html`,
`partials/table.html`.

**Schema:** `("aircraft", "db_flags", "INTEGER")`. Poller stores `a.db_flags`.

**Stats:** replace the `?1` search SQL with named params (`:q :cat :mil :today
:new7 :interesting :privacy :emergency :regular :dormant :alt_min :alt_max`)
and a `Vec<String>` of WHERE clauses.

- `mil` → `(a.db_flags & 1) = 1` (tar1090 `planeObject.js` uses `dbFlags & 1`).
- `interesting` → `(a.db_flags & 2) = 2`.
- `privacy` → `(a.db_flags & 12) != 0` (PIA bit 2, LADD bit 3).
- `emergency` → `EXISTS (SELECT 1 FROM emergency_log el WHERE el.hex = a.hex)`.
- `regular` → at least 5 rows in `visits` for the hex (distinct passes).
- `dormant` → `a.last_seen < :dormant_start` (not seen for 7 days).
- `today` → `a.last_seen >= :today_start` (heard today, not first seen).
- `new7` → `a.first_seen >= :now - 604800`.
- `cat` → `a.last_category`; UI groups `A*` as "fixed wing".

**UI:** chip row inside `#fleet-controls`; active chips styled; toggles
`/partials/table` targeting `#fleet`.

**Verify:** each chip alone and combined with search/sort; military returns
expected hexes.

**Commit:** `plane-stats: fleet filters (category, military, recency, altitude)`

---

## Step 5 — `day_aircraft` + `/today` digest

**Goal:** the end-of-day page: what I saw today, what was new, which records
fell.

**Schema:**

```sql
CREATE TABLE IF NOT EXISTS day_aircraft (
  day TEXT NOT NULL,
  hex TEXT NOT NULL,
  first_ts REAL NOT NULL,
  last_ts  REAL NOT NULL,
  polls INTEGER NOT NULL DEFAULT 1,
  max_alt_ft INTEGER,
  max_gs_kts REAL,
  max_range_nm REAL,
  PRIMARY KEY (day, hex)
);
CREATE INDEX IF NOT EXISTS day_aircraft_hex ON day_aircraft(hex);
```

**Poller:** upsert per live aircraft with `day = Local::now().format("%Y-%m-%d")`;
extend `last_ts`/`polls`, MAX the metrics.

**Stats:** `today_summary`, `new_today`, `records_today` (today's max vs
`day < today`), `hour_histogram`.

**Web:** `/today` + `/api/today`; index gets a "today" card. Data starts at
deploy (no backfill from `visits`).

**Verify:** `curl -s localhost:8081/api/today | python3 -m json.tool`; `new`
equals the "new today" card; `unique >= new`.

**Commit:** `plane-stats: daily ledger and /today digest`

---

## Step 6 — Activity heatmap + range history

**Files:** `stats.rs`, `web.rs`, new `templates/activity.html`, `main.rs`.

- `weekday_hour_heatmap` (7×24 from `poll_log`, local offset applied in SQL).
- `range_by_day` from `day_aircraft`.
- `/activity`: CSS-grid heatmap, SVG messages sparkline, SVG range line.
- `/api/activity`.

**Verify:** busiest cell matches the existing "busiest hour" card; one range
point per day; no mobile horizontal scroll.

**Commit:** `plane-stats: activity heatmap and range history`

---

## Step 7 — Aircraft type dex

**Goal:** a Pokédex for airframes: one card/page per ICAO type designator with
a photo, a short history, specs, and your catch stats. Replaces the old
notifications + polish steps (the squawk/operator polish can come back as a
small follow-up).

**Files:** new `src/types.rs`, `src/main.rs` (schema + boot task), `src/stats.rs`
(type queries), `src/web.rs` (routes), new `templates/types.html`,
`templates/type.html`, `static/app.css`, `templates/index.html`.

**Data (free, no key):**

- **ICAO Doc 8643** — `POST https://doc8643.icao.int/External/AircraftTypes`
  (empty body, ~1.6 MB, 7,261 rows / 2,614 designators): canonical
  `Designator → ModelFullName, ManufacturerCode, EngineType, EngineCount, WTC,
  AircraftDescription`. Fetch at boot into `aircraft_types`; skip when fresh
  (stamp in `enrichment_meta`, like the CSV). Never fatal.
- **Wikipedia** — the `List_of_aircraft_type_designators` wikitext gives a
  curated `designator → article` map (237 codes, e.g. `B738 → Boeing 737 Next
  Generation`). Codes it misses (mostly GA/helicopters) use the Doc 8643
  manufacturer + model as a title guess; the API follows normalisation and
  redirects. Profiles are fetched in batches of 20 titles
  (`prop=pageimages|extracts|pageprops&pithumbsize=640`) for the 640 px photo,
  intro extract and QID. CC BY-SA 4.0 — link the article on every card.
- **Wikidata** — `wbgetentities` in batches of 50 QIDs for first flight
  (P606), total produced (P1092) and wingspan (P2050). Render only the facts
  that exist.
- **Planespotters** (optional, aircraft profile only) —
  `GET https://api.planespotters.net/pub/photos/hex/<hex>` returns one photo.
  Free, no key, but strict terms: descriptive User-Agent with a contact, JSON
  cached ≤24 h, images fetched by the browser from their CDN (never stored or
  re-hosted), photographer credit + link to the photo page next to the image.
  There is no type endpoint — use a representative airframe's photo.

**Schema:** `aircraft_types` (designator PK, manufacturer, model, engine_type,
engine_count, wtc, category, wiki_title, wiki_extract, wiki_image, wiki_url,
first_flight, total_produced, wingspan_m, fetched_at);
`CREATE INDEX idx_enrichment_typecode ON enrichment(typecode)` for the rarity
denominator.

**Queries (`stats.rs`):** `type_dex()` — per designator: distinct hex,
sightings, first/last seen, max alt/speed/range, top operator, plus the OpenSky
registered count as the rarity denominator; `type_aircraft(code)` — airframes
seen, newest first.

**Routes:** `/types`, `/type/:code`, `/api/types`, `/api/type/:code`.

**UI:** `/types` card grid (photo, code, name, `caught / registered`, first
seen; sort by count, rarity, name); `/type/:code` hero photo + extract + fact
chips + "your catches" stats + airframe list; index "types caught" card; nav
pill.

**Verify:** `/api/types` lists every seen type; `/type/B738` renders photo,
extract and stats; boot with the network down still serves pages; no
Planespotters image bytes are stored.

**Commit:** `plane-stats: aircraft type dex`

---

## Step 8 — adsb.fi network integration

**Goal:** use the feeder API for four surfaces: a live local-vs-network
coverage page, network enrichment fallback, a feeder status card, and a
network line in the daily digest.

**Non-goals:** no historical API (there is none), no global snapshot storage,
no network positions injected into tar1090. tar1090 stays the local receiver
view; plane-stats owns the comparison.

**API facts (verified 2026-11-17):**

- Public, 1 req/s: `GET {base}/v3/lat/{lat}/lon/{lon}/dist/{dist}` (≤250 NM).
  Response is `{ac:[...]}`; fields include `hex`, `flight`, `r`, `t`, `desc`,
  `ownOp`, `year`, `alt_baro`, `gs`, `lat`, `lon`, `dst`, `dir`, `seen`,
  `seen_pos`, `mlat`, `squawk`, `category`, `track`, `baro_rate`.
- Feeder, 1 req/30 s: `GET {base}/v2/snapshot` (all ~7.7k aircraft, ~1 MB
  gzipped). `HEAD` returns 200 for a recognized feeder IP and is cheap.
- Website endpoints (undocumented, cache): `https://api.adsb.fi/v1/stats`
  (`beastFeeders`, `mlatFeeders`, `totalAircraft`) and `/v1/coverage`
  (`user`, `lat`, `lon` per feeder).
- Terms: personal/non-commercial; cite adsb.fi and link
  <https://adsb.fi/> on every page that renders network data.

**Decisions:**

- Use the public v3 radius endpoint for the live view (10 s, small response,
  `dst`/`dir`), not the global snapshot (1 MB, 1/30 s). The snapshot is used
  only for the cheap HEAD feeder check.
- Diff a 60 s rolling network set, not one poll, so a single missed poll does
  not create a false "unique" row.
- Exclude ground contacts from coverage and the digest; airport ground
  clutter is not a coverage gap.
- Keep tar1090 local-only. Network data lives in plane-stats.

### 8.0 Foundation — network poller, schema, shared state

**Files:** new `src/network.rs`; `src/main.rs` (mod + schema + spawn);
`src/poller.rs` (`Alt` -> `pub(crate)`); `docker-compose.yml`.

**Config (new env, defaults in compose):**

| Variable | Default | Meaning |
|---|---|---|
| `ADSB_FI_API` | `https://opendata.adsb.fi/api` | Open-data base |
| `ADSB_FI_WEB_API` | `https://api.adsb.fi` | Website stats/coverage base |
| `NETWORK_RADIUS_NM` | `100` | v3 query radius, clamp 10..250 |
| `NETWORK_POLL_SECS` | `10` | v3 poll interval, min 5 |
| `FEEDER_CHECK_SECS` | `35` | `HEAD /v2/snapshot` interval |
| `NETWORK_STATS_SECS` | `300` | `/v1/stats` cache |
| `NETWORK_COVERAGE_SECS` | `600` | `/v1/coverage` cache |
| `FEED_JSON` | `/run/adsbfi-feed/aircraft.json` | Host feed status file |

If `RX_LAT`/`RX_LON` are unset, log `network poller disabled` and return; the
rest of the app is unaffected.

**Schema (add to the `CREATE TABLE IF NOT EXISTS` batch in `main.rs`):**

```sql
CREATE TABLE IF NOT EXISTS network_day_aircraft (
  day TEXT NOT NULL,
  hex TEXT NOT NULL,
  first_ts REAL NOT NULL,
  last_ts  REAL NOT NULL,
  polls INTEGER NOT NULL DEFAULT 1,
  max_alt_ft INTEGER,
  max_gs_kts REAL,
  max_range_nm REAL,
  mlat_pos INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (day, hex)
);
CREATE INDEX IF NOT EXISTS network_day_aircraft_hex ON network_day_aircraft(hex);
CREATE TABLE IF NOT EXISTS network_enrichment (
  hex TEXT PRIMARY KEY,
  registration TEXT,
  typecode TEXT,
  description TEXT,
  owner TEXT,
  year TEXT,
  updated_at REAL NOT NULL
);
```

Ground contacts are not written to `network_day_aircraft` (their
`max_alt_ft` would be NULL); they are still written to `network_enrichment`.
This keeps the coverage metric about airborne aircraft.

**`src/network.rs`:**

- `#[derive(Deserialize)] struct V3Response { ac: Vec<V3Aircraft> }`.
- `V3Aircraft` maps `r` -> `registration`, `t` -> `typecode`, `desc` ->
  `description`, `ownOp` -> `owner`, `year` -> `year`, `alt_baro` -> `Alt`
  (reuse `poller::Alt`), plus `hex`, `flight`, `gs`, `lat`, `lon`, `dst`,
  `dir`, `seen`, `seen_pos`, `mlat`, `squawk`, `category`, `track`,
  `baro_rate`.
- `NetworkAircraft` is the normalized row: `hex`, `flight`, `registration`,
  `typecode`, `description`, `owner`, `year`, `alt_ft`, `on_ground`,
  `gs_kts`, `lat`, `lon`, `dst_nm`, `dir_deg`, `seen`, `mlat_pos`.
- `NetworkView { fetched_at, ok, error, radius_nm, aircraft:
  HashMap<String, NetworkAircraft>, recent: HashMap<String, f64>,
  status: NetworkStatus }`.
  `recent` is the 60 s rolling set: insert every airborne hex on each poll,
  prune entries older than `LIVE_WINDOW_SECS`. It is what the coverage page
  diffs, so a single poll that misses an aircraft does not create a false
  "unique".
- `static VIEW: LazyLock<RwLock<Arc<NetworkView>>>`; `pub fn latest() ->
  Arc<NetworkView>` clones the Arc, not the map.
- `pub async fn run(db: Db)`:
  1. Read `stats::rx_pos()`; if none, log and return.
  2. Loop: `GET {ADSB_FI_API}/v3/lat/{lat}/lon/{lon}/dist/{radius}` with
     `User-Agent: plane-stats/0.1 (adsb.fi feeder)`, 10 s timeout.
  3. On 200: normalize, update `VIEW` (replace the Arc), write
     `network_day_aircraft` + `network_enrichment` in one transaction.
  4. On 429: set `ok=false`, `error="rate limited"`, sleep 60 s.
  5. On other errors: set `ok=false`, keep the last good `aircraft`/`recent`
     so pages show stale data with a banner.
  6. Every `FEEDER_CHECK_SECS`: `HEAD {ADSB_FI_API}/v2/snapshot`; record
     `snapshot_ok`, `snapshot_at`, `snapshot_status`. HEAD is cheap and 200
     proves the feeder IP is recognized. If HEAD ever returns 405, fall back
     to a 5-minute GET and discard the body.
  7. Every `NETWORK_STATS_SECS`: `GET {ADSB_FI_WEB_API}/v1/stats`; cache
     `beast_feeders`, `mlat_feeders`, `total_aircraft`.
  8. Every `NETWORK_COVERAGE_SECS`: `GET {ADSB_FI_WEB_API}/v1/coverage`;
     count feeders within `radius + 50` NM; cache. These two are
     undocumented website endpoints; failures are non-fatal.
- `NetworkStatus` is serialized for `/api/feed`: `last_poll_at`, `ok`,
  `error`, `aircraft`, `radius_nm`, `snapshot_ok`, `snapshot_at`,
  `snapshot_status`, `totals`, `nearby_feeders`, `nearby_feeders_at`.
- `pub(crate) fn distance_nm(lat1, lon1, lat2, lon2) -> f64` (haversine) for
  the nearby-feeder count; unit-test one known pair.

**Write path:** use `crate::db_lock(&db)` and a short transaction. Upsert
`network_day_aircraft` with `day = stats::local_day(now)`, `first_ts`/`last_ts`,
`polls + 1`, `MAX` alt/gs/range, `mlat_pos = MAX(...)`. Skip `on_ground`.
Upsert `network_enrichment` with `coalesce` so a missing field never erases a
known one.

**Main:** `mod network;` and `tokio::spawn(network::run(db.clone()));` next to
the poller and type task, after `set_rx_pos`.

**Compose:** add the env above and mount the feed status file read-only:

```yaml
volumes:
  - plane_stats_db:/data
  - /run/adsbfi-feed:/run/adsbfi-feed:ro
environment:
  ADSB_FI_API: ${ADSB_FI_API:-https://opendata.adsb.fi/api}
  NETWORK_RADIUS_NM: ${NETWORK_RADIUS_NM:-100}
  NETWORK_POLL_SECS: ${NETWORK_POLL_SECS:-10}
  FEED_JSON: /run/adsbfi-feed/aircraft.json
```

**Verify:** `docker logs -f plane-stats` shows `network poll ok: N aircraft`
every 10 s and no 429; the HEAD check logs 200; `network_day_aircraft` has
rows for today; `network_enrichment` has rows. Kill the network (bad
`ADSB_FI_API` in a local run) and confirm the poller logs the error and keeps
serving.

**Commit:** `plane-stats: adsb.fi network poller and schema`

### 8.1 Unique/missed coverage page

**Goal:** `/coverage` shows, for the last 60 s, what only your receiver heard
and what the network heard that you missed.

**Files:** `src/stats.rs`, `src/web.rs`, new `templates/coverage.html`, new
`templates/partials/coverage.html`, `templates/index.html` (nav),
`static/app.css`.

**Query (`stats::network_coverage(db, now)`):**

- Local live:
  `SELECT a.hex, a.last_flight, coalesce(nullif(e.registration,''), n.registration), coalesce(nullif(e.typecode,''), n.typecode), a.last_alt_ft, a.last_gs_kts, a.last_range_nm, a.last_lat, a.last_lon, a.last_seen FROM aircraft a LEFT JOIN enrichment e ON e.icao24 = a.hex LEFT JOIN network_enrichment n ON n.hex = a.hex WHERE a.last_seen > :cutoff AND (a.last_alt_ft IS NULL OR a.last_alt_ft > 0)`.
- Network: `network::latest()`; use `recent` for membership and `aircraft`
  for details; skip `on_ground`.
- `overlap = local ∩ recent`; `unique = local - recent`; `missed = recent -
  local`.
- `CoverageRow { hex, flight, registration, typecode, alt_ft, gs_kts,
  dst_nm, dir_deg, seen, mlat_pos, last_seen_local }` with
  `map_url(base)`, `trace_url(base)`, `adsb_url()`.
- `NetworkCoverage { fetched_at, age_secs, ok, error, stale, radius_nm,
  local_count, network_count, overlap, unique, missed, unique_rows,
  missed_rows }`.
- `stale = !ok || age_secs > LIVE_WINDOW_SECS`. Stale keeps the last rows but
  the template shows "last network poll Xs ago" and no "0 missed" claim.

**Routes:** `/coverage` (page), `/partials/coverage` (10 s htmx refresh),
`/api/coverage`.

**Template:** hero "Coverage" + tagline "Local receiver vs adsb.fi within
{{ radius }} NM"; a status line; five cards (local live, network, overlap,
only you, missed); two tables:

- "Only you saw" — hex, callsign, registration, type, alt, range, last seen,
  map/trace.
- "Network saw, you missed" — hex, callsign, registration, type, alt, range,
  bearing, age, MLAT badge, adsb.fi link. No tar1090 link: the aircraft is
  not in the local receiver's data.

Empty states explain the 60 s window. Footer: "Network data: adsb.fi" link.

**Nav:** add a Coverage pill to `index.html`, `today.html`, `activity.html`,
`types.html`; the aircraft profile keeps its Dashboard link.

**Verify:** at the same moment, `curl` v3 and
`localhost:8080/data/aircraft.json` and compare counts; an aircraft in both
must be overlap, not unique. Force a network error and confirm the banner/stale
state. Check the 10 s partial refresh does not reset scroll.

**Commit:** `plane-stats: unique/missed coverage page`

### 8.2 Network enrichment fallback

**Goal:** fill registration/type/owner/description/year from adsb.fi when
OpenSky has no row or a blank field.

**Files:** `src/stats.rs`, `templates/aircraft.html`, `README.md`.

**Merge (`stats::enrichment_for`):**

- Query `enrichment` as today; query `network_enrichment` by hex.
- If both absent -> `None`.
- If OpenSky present: `registration = e.registration.or(n.registration)`,
  `typecode = e.typecode.or(n.typecode)`, `owner = e.owner.or(n.owner)`,
  `built = e.built.or(n.year)`, `description = n.description`.
- If OpenSky absent: return `Enrichment` with `registration`, `typecode`,
  `owner`, `built = year`, `description`; other fields `None`.
- Add `description: Option<String>` to `Enrichment`.

**Fleet table:** in `COLS`, use
`coalesce(nullif(e.registration,''), n.registration)` and
`coalesce(nullif(e.typecode,''), n.typecode)`; add
`LEFT JOIN network_enrichment n ON n.hex = a.hex` to `FROM`. The positional
count and `row_to_aircraft` do not change. Add `n.registration`,
`n.typecode`, `n.description`, `n.owner` to the search clause. Update
`record()` the same way for the record cards.

**Profile:** add a "Description" row to the Identity table; "Built" already
renders `m.built`, which now carries the network year when OpenSky has none.

**Type dex:** do not coalesce here yet. `TYPE_DEX_SQL` uses OpenSky's
`typecode` as the rarity denominator; mixing network-only typecodes changes
the meaning of `caught / registered`. Note it as a follow-up.

**Verify:** pick a hex with no OpenSky row but a `network_enrichment` row;
`/aircraft/<hex>` shows registration/type/owner/year. Pick a hex where OpenSky
has a blank owner; the network owner appears. Search the fleet by the
network-only owner/registration and confirm the row is found. Confirm OpenSky
still wins when both exist.

**Commit:** `plane-stats: network enrichment fallback`

### 8.3 Feeder status card

**Goal:** one panel that answers "is my feed alive and is adsb.fi seeing it?"

**Files:** `src/stats.rs`, `src/web.rs`, new `templates/partials/feed.html`,
`templates/index.html`, `static/app.css`, `README.md`.

**Status (`stats::feeder_status(now)`):**

- `feed`: read `FEED_JSON`; parse `{now, messages, aircraft}`; `age_secs` from
  the file mtime (fallback `now - payload.now`); `aircraft_count`,
  `messages`. Missing/unreadable -> `error`.
- `network`: `network::latest().status` (last poll age, ok/error, aircraft
  count, radius, snapshot 200, totals, nearby feeders).
- `local_poller`: `last_poll_age(db, now)` (already exists).

**Routes:** `/partials/feed`, `/api/feed`.

**UI:** a "Feeder" panel on the dashboard after Summary:
`<div id="feed" hx-get="/partials/feed" hx-trigger="load, every 30s"></div>`.
The partial is a compact grid:

- Feed client: `OK · N aircraft · M messages` or "status file missing".
- Network API: `OK · N aircraft within R NM · Xs ago` or the error.
- Feeder recognized: `yes · HEAD 200 · Xs ago` or "no".
- Network totals: `7,479 feeders · 6,779 MLAT · 6,941 aircraft`.
- Nearby feeders: `N within R+50 NM` (optional).

All network values are cached by the poller; the partial does no network I/O.

**Verify:** `curl -s localhost:8081/api/feed | python3 -m json.tool`; run with
`FEED_JSON=/nonexistent` and confirm the "missing" state; force a bad API base
and confirm the error state, not a blank card.

**Commit:** `plane-stats: feeder status card`

### 8.4 Daily digest network line

**Goal:** `/today` says how much of the network's local traffic you covered.

**Files:** `src/stats.rs`, `templates/today.html`, `README.md`.

**Digest (`stats::today_summary`):** add a nested `NetworkDigest`:

```rust
pub struct NetworkDigest {
    pub seen: i64,          // network_day_aircraft today (airborne)
    pub local_seen: i64,    // day_aircraft today, max_alt_ft IS NOT NULL
    pub only_network: i64,  // network - local
    pub only_local: i64,    // local - network
    pub overlap: i64,
    pub cover_pct: Option<f64>, // overlap / seen
    pub ok: bool,
    pub age_secs: Option<f64>,
    pub radius_nm: f64,
}
```

Queries (all `WHERE day = ?1`):

- `seen = COUNT(*) FROM network_day_aircraft`
- `local_seen = COUNT(*) FROM day_aircraft WHERE max_alt_ft IS NOT NULL`
- `only_network = network_day_aircraft LEFT JOIN day_aircraft ... WHERE d.hex IS NULL`
- `only_local = day_aircraft LEFT JOIN network_day_aircraft ... WHERE n.hex IS NULL AND d.max_alt_ft IS NOT NULL`
- `overlap = day_aircraft JOIN network_day_aircraft ... WHERE d.max_alt_ft IS NOT NULL`
- `cover_pct = overlap * 100 / seen` when `seen > 0`, else `None`.
- `ok = network::latest().status.ok && age_secs <= 60`; `radius_nm` from the
  view.

Ground contacts are already excluded by the 8.0 write path
(`network_day_aircraft` skips `on_ground`); `day_aircraft.max_alt_ft` is NULL
for ground, so the `max_alt_ft IS NOT NULL` filters keep the local side
like-for-like.

**Template:** add a fifth card to "At a glance":
`{{ summary.network.seen }}` "network saw" with
`{{ summary.network.local_seen }}` "you saw"
(`{{ summary.network.cover_pct }}%` of the network) and
`{{ only_local }} only you · {{ only_network }} only network`.
If `!summary.network.ok`, show "network data unavailable" and omit the
percentage. Add a footnote: "Within {{ radius }} NM; ground contacts excluded."

**API:** `/api/today` gains `summary.network`.

**Verify:** `curl -s localhost:8081/api/today | python3 -m json.tool`; check
`only_local + overlap == network.local_seen` and
`only_network + overlap == network.seen`. On a day with no network rows,
`cover_pct` is `null` and the template says unavailable, not 0%.

**Commit:** `plane-stats: daily digest network coverage`

### 8.5 Docs, terms, rollout

- `README.md`: add `/coverage`, `/api/coverage`, `/api/feed`; config table
  rows; `network_day_aircraft` / `network_enrichment` data-model bullets;
  "Network data: adsb.fi" citation.
- `AGENTS.md`: add a "Network integration" subsection under plane-stats
  status: v3 10 s, HEAD 35 s, stats 5 min, coverage 10 min, 1 req/s public /
  1 req/30 s snapshot; ground contacts excluded; `network_day_aircraft`
  starts at deploy; `FEED_JSON` mount.
- `feeders/adsbfi/README.md`: note that plane-stats reads
  `/run/adsbfi-feed/aircraft.json` and hosts `/coverage`.
- Terms: every page that renders network data links <https://adsb.fi/>; the
  poller sends a `plane-stats/0.1` User-Agent.

**Rollout order:** 8.0 (foundation) -> 8.1 (coverage) -> 8.2 (enrichment) ->
8.3 (feeder card) -> 8.4 (digest). Each is a separate build/verify/commit;
8.1 is the highest-value surface, 8.2/8.3 are cheap, 8.4 depends on 8.0's
day table.

---

## Cross-cutting notes

- **Positional columns:** `aircraft_sorted` reads `COLS` via `row_to_aircraft`.
  Append new fields to the end of `COLS`, `AircraftRow`, and `row_to_aircraft`.
- **tar1090 links:** `?icao=<hex>` selects, `?showTrace=<hex>` shows the track.
  Base is `TAR1090_URL` or the request `Host` with port 8080. Per-aircraft
  links render only while `is_live()` is true (`last_seen` within
  `LIVE_WINDOW_SECS`, 60 s); the header always links to the bare map. Never add
  `hideSidebar` (tar1090 persists it in its own localStorage).
- **`dbFlags`:** bit 0 military, 1 interesting, 2 PIA, 3 LADD.
- **`aircraft.json`:** `r_dst` (nm), `seen` (message age s), `seen_pos`,
  `dbFlags`, `category`. Poller keeps `seen <= 60`.
- **Time:** `chrono::Local` (container `TZ=America/Montreal`). `poll_log` is
  pruned at 90 days; `aircraft` and `visits` are never pruned.
- **DB volume:** `sdr_plane_stats_db`. Backups go to
  `/mnt/truenas/talon/pi-backups` via `/usr/local/bin/pi-backup.sh`
  (daily 03:00 timer). The backup excludes `enrichment` and the NAS credentials.
- **Don't** put the Docker data-root or the SQLite DB on CIFS; overlay2 and
  SQLite locking are unreliable there.

## Progress

| Step | Status | Commit |
|---|---|---|
| 1. Deep links | done | 047272d |
| 2. Live panel | done | `plane-stats: live overhead panel with current state` |
| 3. Pagination | done | `plane-stats: paginate fleet table` |
| 4. Filters | done | `plane-stats: fleet filters (category, military, recency, altitude)` |
| 5. `day_aircraft` + `/today` | done | `plane-stats: daily ledger and /today digest` |
| 6. Heatmap + range history | done | `plane-stats: activity heatmap and range history` |
| 7. Type dex | done | — |
| 8a. Network poller + schema | done | `plane-stats: adsb.fi network poller and schema` |
| 8b. Unique/missed coverage page | done | `plane-stats: unique/missed coverage page` |
| 8c. Network enrichment fallback | done | `plane-stats: network enrichment fallback` |
| 8d. Feeder status card | done | `plane-stats: feeder status card` |
| 8e. Daily digest network coverage | done | `plane-stats: daily digest network coverage` |
