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

## Step 9 — Country of registration

**Goal:** every aircraft gets its state of registry from its ICAO 24-bit
address; show it on the profile, in the fleet table, as a dashboard card, and
as a fleet filter.

**Files:** new `src/country.rs`; `src/main.rs` (mod, column, backfill);
`src/poller.rs`; `src/stats.rs`; `src/web.rs`; `templates/index.html`;
`templates/partials/table.html`; `templates/partials/table_row.html`;
`templates/partials/cards.html`; `templates/aircraft.html`; `static/app.css`;
`README.md`; `AGENTS.md`.

**Backup first:** this is a schema step.

**Data source:** ICAO Annex 10 Vol III, Table 9-1 (Amendment 92, effective
2024-07-22). A machine-readable copy is
`ibosoftnet/icao-aircraft-addresses`, file
`Hexadecimal Addresses (Amendment 92).csv` (`state,noOfAddr,startAddr,endAddr`;
`noOfAddr` is hex — ignore it and use the start/end columns). Generate a sorted
const once and commit it; do not fetch at runtime:

```rust
// (start, end, ISO 3166-1 alpha-2, state name), inclusive, sorted by start.
pub(crate) const BLOCKS: &[(u32, u32, &str, &str)] = &[
    (0x700000, 0x700fff, "AF", "Afghanistan"),
    // ... ~196 rows ...
    (0xa00000, 0xafffff, "US", "United States"),
];
```

Map the CSV's state names to ISO alpha-2 once while generating the table; use
`""` for entries with no ISO code (e.g. Kosovo) so those rows show the name
without a flag.

**`src/country.rs`:**

```rust
pub(crate) fn code_for_hex(hex: &str) -> Option<&'static str>; // ISO2
pub(crate) fn name(code: &str) -> &'static str;
pub(crate) fn flag(code: &str) -> String; // regional indicators; "" if not 2 ASCII letters
pub(crate) fn backfill(db: &Db) -> rusqlite::Result<usize>;
```

- `code_for_hex`: parse the hex as `u32`, binary-search `BLOCKS` by start
  (`partition_point`), then check `addr <= end`. Malformed/non-ICAO input
  (`~`-prefixed) returns `None`.
- `backfill`: mirror `dbflags::backfill`. If
  `enrichment_meta.key = 'country_v1'` is absent, iterate `SELECT hex FROM
  aircraft`, `UPDATE aircraft SET country = ?1 WHERE hex = ?2`, then write the
  stamp. Idempotent; bump the key if the table changes.
- Tests: known blocks (`a00000` US, `c00000` Canada, `400000` UK, `380000`
  France, `3c0000` Germany, `7c0000` Australia, `780000` China, `840000`
  Japan), inclusive bounds, unallocated `000000`, malformed input.

**Schema:** append `("aircraft", "country", "TEXT")` to `NEW_COLUMNS`.

**Poller:** compute `crate::country::code_for_hex(&hex)` and append it as the
last placeholder in the `aircraft` INSERT. On conflict use
`country = coalesce(excluded.country, aircraft.country)` so a temporarily
incomplete table never erases a known code; the stamp-driven backfill handles
corrections.

**Stats:**

- Append `a.country` to `COLS`, `country` to `AircraftRow`, and the field to
  `row_to_aircraft` (append only — indices are positional). Add
  `country_name() -> String` and `country_flag() -> String`.
- `FleetFilter.country: String`; `to_sql` adds `a.country = :country`;
  `active_labels` adds `country::name(&self.country)`.
- `CountryCount { pub code: String, pub name: String, pub count: i64 }`
  (serde) and `country_counts(db) -> Vec<CountryCount>`: `SELECT country,
  COUNT(*) FROM aircraft WHERE country IS NOT NULL GROUP BY country
  ORDER BY 2 DESC, 1`.
- `Stats.countries: i64` (`COUNT(DISTINCT country)`) and
  `Stats.top_country: Option<CountryCount>`; both are cheap enough for the
  existing 10 s `stats_cached`.

**Web:** `SortParams.country` (default empty, accept only 2 ASCII letters),
`SortParams::filter()` copies it, and `fleet_query` appends
`country=<code>` so the address bar/bookmark keeps it. `FleetView` and
`IndexTemplate` gain `countries: Vec<CountryCount>`.

**UI:**

- `index.html`: a "Country" filter row with a `<select id="f-country"
  name="country">` ("Any country" + top 30 by count), `hx-get="/partials/table"
  hx-trigger="change" hx-target="#fleet" hx-include="#fleet-controls,
  #fleet-sort, #fleet-dir"`. Always include the active country in the options
  even when it falls outside the top 30, or the select silently resets on the
  next render.
- `partials/table.html`: a `Country` header after Registration;
  `table_row.html`: flag + ISO code, `title` = full name.
- `partials/cards.html`: a Ledger card "countries" with the count and the top
  country's name.
- `aircraft.html`: an Identity row before Registration:
  `{{ row.country_flag() }} {{ row.country_name() }}`.
- `app.css`: only if the flag/code cell or the select needs alignment.

**Verify:** `cargo test` (country unit tests), `cargo check`, build, `up -d`.
`curl -s localhost:8081/api/aircraft | jq '.[0].country'`; compare
`SELECT country, COUNT(*) FROM aircraft GROUP BY 1 ORDER BY 2 DESC` with the
select options; filter to `CA` and confirm every row is Canadian; open a
Canadian profile and see the flag/name; restart and confirm the backfill does
not run twice.

**Commit:** `plane-stats: country of registration from ICAO address blocks`

---

## Step 10 — Receiver coverage polar plot

**Goal:** a polar plot of the best range heard in each 10° bearing sector, so
antenna nulls, terrain shadows, and gain/bias-tee changes are visible.

**Files:** `src/main.rs` (table); `src/poller.rs` (rollup); `src/stats.rs`
(query + chart geometry); `src/web.rs`; `templates/coverage.html`;
`static/app.css`; `README.md`; `AGENTS.md`.

**Backup first:** this is a schema step.

**Schema:**

```sql
CREATE TABLE IF NOT EXISTS range_sector (
  day TEXT NOT NULL,
  sector INTEGER NOT NULL,        -- 0..35, 10° clockwise from north
  max_range_nm REAL NOT NULL,
  polls INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (day, sector)
);
```

No extra index; the primary key covers the `day` prefix. `range_sector` grows
36 rows/day (~13k/year) and is never pruned — the all-time plot needs it.

**Poller:** in `record_poll`, read `crate::stats::rx_pos()` once; if `None`,
skip the sector rollup (the rest of the poll still runs). For each live
aircraft that is not on the ground, has `lat`/`lon`, and has a finite
`r_dst > 0`:

```rust
let bearing = crate::stats::bearing_deg(rx.0, rx.1, lat, lon);
let sector = (bearing / 10.0).floor() as i64 % 36;
```

Upsert into `range_sector` with the same `day` already computed for
`day_aircraft`:

```sql
INSERT INTO range_sector (day, sector, max_range_nm, polls)
VALUES (?1, ?2, ?3, 1)
ON CONFLICT(day, sector) DO UPDATE SET
  max_range_nm = MAX(range_sector.max_range_nm, excluded.max_range_nm),
  polls = range_sector.polls + 1
```

Exclude ground contacts: airport clutter is not coverage. `polls` counts
aircraft-poll samples for a future density overlay; the plot uses
`max_range_nm` only.

**Stats:**

```rust
pub struct PolarPoint {
    pub bearing_deg: f64, pub range_nm: f64,
    pub x: f64, pub y: f64, pub label: String,
}
pub struct PolarRing { pub range_nm: f64, pub r: f64 }
pub struct PolarChart {
    pub points: Vec<PolarPoint>, pub path: String, pub max_nm: f64,
    pub recent_points: Vec<PolarPoint>, pub recent_path: String,
    pub recent_days: u32,
    pub rings: Vec<PolarRing>, pub width: f64, pub height: f64,
    pub center: f64, pub radius: f64, pub first_day: Option<String>,
}
pub fn receiver_polar(db: &Db, now: f64) -> rusqlite::Result<PolarChart>;
```

- All-time: `SELECT sector, MAX(max_range_nm) FROM range_sector GROUP BY
  sector ORDER BY sector`.
- Recent: the same query with `WHERE day >= :cutoff` for the last 30 days
  (`recent_days = 30`). The recent polygon is the current antenna view; the
  all-time polygon is the envelope. A gain or cable change shows as the recent
  line pulling away from the envelope.
- If the all-time query returns no rows, return `points: Vec::new()`,
  `rings: Vec::new()`, both paths empty, and let the template show the empty
  state. Otherwise build all 36 slots for both series, missing sectors = 0.
- `max_nm = max(1.0, (max / 50.0).ceil() * 50.0)`; rings at 25/50/75/100%.
- Fixed geometry: `width = height = 360`, `center = 180`, `radius = 150`.
  `x = center + radius * (range/max_nm) * sin(rad)`,
  `y = center - radius * (range/max_nm) * cos(rad)` (0° = north, clockwise).
  `path = "M center,center L x0,y0 ... Z"`; all-zero data → empty path.
- `first_day = SELECT MIN(day) FROM range_sector`.
- Tests: bearing→sector boundaries (0°, 9.99°, 10°, 359.9°), north/east points
  land at the expected x/y, empty data → empty path and no NaN.

**Web:** `CoverageTemplate` gains `polar`; `coverage_page` calls
`receiver_polar(&db, now)`. Keep the polar panel **outside** the 10 s
`#coverage` partial so it is not re-queried every tick. Add `GET /api/polar`
(do not change the `/api/coverage` shape).

**Template:** a "Receiver coverage" panel above the live coverage panel:

```html
<section class="glass panel" aria-label="Receiver coverage">
  <h2>Receiver coverage</h2>
  {% if polar.points.is_empty() %}
  <p class="empty-note">No sector data yet. The plot fills in as positioned
    aircraft are heard.</p>
  {% else %}
  <div class="polar">
    <svg viewBox="0 0 360 360" role="img"
         aria-label="Best range in each 10 degree bearing sector">
      {% for ring in polar.rings %}
      <circle class="polar-ring" cx="{{ polar.center }}" cy="{{ polar.center }}"
              r="{{ ring.r }}"></circle>
      {% endfor %}
      <line class="polar-spoke" x1="{{ polar.center }}" y1="{{ polar.center }}"
            x2="{{ polar.center }}" y2="30"></line>
      <path class="polar-area" d="{{ polar.path }}"></path>
      <path class="polar-recent" d="{{ polar.recent_path }}"></path>
      {% for p in polar.points %}
      <circle class="polar-dot" cx="{{ p.x }}" cy="{{ p.y }}" r="2">
        <title>{{ p.label }} · {{ p.range_nm }} nm (all time)</title>
      </circle>
      {% endfor %}
    </svg>
    <p class="chart-note">All time (fill) vs last {{ polar.recent_days }} days
      (line){% if let Some(d) = polar.first_day %} · since {{ d }}{% endif %}</p>
  </div>
  <details class="chart-table">
    <summary>Show as table</summary>
    <!-- 36 rows: bearing, max range -->
  </details>
  {% endif %}
</section>
```

`app.css`: `.polar svg { width: 100%; max-width: 420px; height: auto; }`,
`.polar-ring { fill: none; stroke: var(--border); }`,
`.polar-spoke { stroke: var(--border); }`,
`.polar-area { fill: rgba(158, 197, 232, 0.12); stroke: var(--border); }`,
`.polar-recent { fill: none; stroke: var(--accent); }`,
`.polar-dot { fill: var(--accent); }`.

**Verify:** `curl -s localhost:8081/api/polar | jq '.max_nm, .points[0]'`;
the all-time global max should equal `SELECT MAX(max_range_nm) FROM
day_aircraft` within one poll (the same aircraft supplies both); each recent
sector is `<=` its all-time value; manually compute a bearing for a
high-range aircraft and confirm its sector; no RX position → empty state and
no `range_sector` writes; ground contacts absent; no NaN in either path; no
horizontal scroll on mobile.

**Commit:** `plane-stats: receiver coverage polar plot`

---

## Step 11 — Activity charts: traffic and coverage trend

**Goal:** `/activity` gains unique aircraft per day, new aircraft per day, and
network coverage % per day. These are the missing time series next to best
range and messages.

**Files:** `src/stats.rs`; `src/web.rs`; `templates/activity.html`;
`README.md`; `AGENTS.md`.

**No schema.**

### 11a — Unique and new per day

- `unique_by_day(db, now) -> Vec<DayValue>`: last 90 days from
  `day_aircraft`:

```sql
SELECT day, COUNT(*) FROM day_aircraft
WHERE day >= ?1 GROUP BY day ORDER BY day
```

- `new_by_day(db, now) -> Vec<DayValue>`: last 90 days from
  `aircraft.first_seen`, bucketed by local day:

```sql
SELECT strftime('%Y-%m-%d', first_seen, 'unixepoch', 'localtime') AS day,
       COUNT(*)
FROM aircraft WHERE first_seen >= ?1
GROUP BY day ORDER BY day
```

`strftime(..., 'localtime')` uses the container `TZ` and applies the DST
offset per timestamp. Do not copy `messages_by_day`'s fixed-offset trick here;
that one stores a day index, not a timestamp.

`unique_by_day` starts at deploy (the `day_aircraft` ledger), while
`new_by_day` covers the full `aircraft` history. Note that difference in the
chart caption rather than implying both start on the same day.

### 11b — Network coverage per day

```rust
pub struct CoverageDay {
    pub day: String,
    pub network: i64,
    pub local: i64,
    pub overlap: i64,
    pub cover_pct: Option<f64>, // None when the local poller was down
}
pub fn coverage_by_day(db: &Db, now: f64) -> rusqlite::Result<Vec<CoverageDay>>;
```

```sql
WITH net AS (
  SELECT day, COUNT(*) n FROM network_day_aircraft
  WHERE day >= :cutoff GROUP BY day
), loc AS (
  SELECT day, COUNT(*) l FROM day_aircraft
  WHERE day >= :cutoff AND max_alt_ft IS NOT NULL GROUP BY day
), ov AS (
  SELECT d.day, COUNT(*) o FROM day_aircraft d
  JOIN network_day_aircraft n ON n.day = d.day AND n.hex = d.hex
  WHERE d.day >= :cutoff AND d.max_alt_ft IS NOT NULL GROUP BY d.day
), up AS (
  SELECT strftime('%Y-%m-%d', ts, 'unixepoch', 'localtime') day,
         COUNT(*) polls
  FROM poll_log WHERE ts >= :cutoff_ts GROUP BY 1
)
SELECT net.day, net.n, coalesce(loc.l, 0), coalesce(ov.o, 0),
       coalesce(up.polls, 0)
FROM net
LEFT JOIN loc ON loc.day = net.day
LEFT JOIN ov  ON ov.day  = net.day
LEFT JOIN up  ON up.day  = net.day
ORDER BY net.day
```

`cover_pct = Some(overlap * 100.0 / network)` only when `polls > 0 &&
network > 0`; otherwise `None`. A day with the poller down must be a gap, not
0%. `poll_log` is pruned at 90 days, which matches the chart window. Ground
contacts are already excluded on both sides (`max_alt_ft IS NOT NULL` and the
network write path).

### 11c — Chart and template

- `Activity` gains `unique: Vec<DayValue>`, `new: Vec<DayValue>`, and
  `coverage: Vec<CoverageDay>`; `/api/activity` returns them automatically.
- `LineChart::build_bounded(series, min, max)` for the 0–100% coverage axis;
  refactor `build` to delegate with `None` bounds. A 60–80% series must sit in
  the upper half of the plot, with labels 100/50/0. Unit-test that.
- `ActivityTemplate` gains `unique`, `new`, `coverage_chart` (built from the
  `Some` days) and `coverage` (raw rows for the table).
- Add three panels after "Best range by day": "Unique aircraft by day",
  "New aircraft by day", "Network coverage by day". Reuse the existing chart
  markup and `<details>` table. The coverage table shows day, local, network,
  overlap, cover %. Caption: "Days with no local polls are omitted."

**Verify:** `curl -s localhost:8081/api/activity | jq '.coverage[0]'`; per day
`overlap <= min(local, network)` and `cover_pct` matches; a day with the
poller stopped is absent/`None`, not 0%; `unique` per day equals
`COUNT(*) FROM day_aircraft`; `new` sums to the `aircraft` count in the window;
a one-point series renders flat; no mobile horizontal scroll.

**Commit:** `plane-stats: traffic and coverage trends on activity`

---

## Step 12 — Records and rarities page

**Goal:** `/records` collects the fun extremes the ledger already has but only
shows one at a time: one-hit wonders, longest overhead pass, steepest
climb/descent, oldest airframes, rarest types, most-heard callsigns.

**Files:** `src/stats.rs`; `src/web.rs`; new `templates/records.html`; nav
pills in `index.html`, `today.html`, `activity.html`, `types.html`,
`coverage.html`; `static/app.css`; `README.md`; `AGENTS.md`.

**No schema.**

**Stats structs:**

```rust
pub struct RarityRow {
    pub hex: String, pub registration: Option<String>,
    pub typecode: Option<String>, pub callsign: Option<String>,
    pub first_seen: f64, pub last_seen: f64, pub sightings: i64,
}
pub struct VisitRecord {
    pub hex: String, pub registration: Option<String>,
    pub typecode: Option<String>, pub start_ts: f64, pub end_ts: f64,
    pub duration_secs: f64, pub polls: i64,
}
pub struct YearRecord {
    pub hex: String, pub registration: Option<String>,
    pub typecode: Option<String>, pub year: i64,
}
pub struct CallsignRecord {
    pub callsign: String, pub airframes: i64, pub sightings: i64,
}
pub struct RarestType {
    pub designator: String, pub name: String,
    pub caught: i64, pub registered: i64, pub pct: Option<f64>,
}
pub struct Records {
    pub one_hit_count: i64,
    pub one_hits: Vec<RarityRow>,
    pub longest_visits: Vec<VisitRecord>,
    pub steepest_climbs: Vec<Record>,
    pub steepest_descents: Vec<Record>,
    pub oldest: Vec<YearRecord>,
    pub rarest_types: Vec<RarestType>,
    pub top_callsigns: Vec<CallsignRecord>,
}
pub fn records(db: &Db) -> rusqlite::Result<Records>;
```

**Queries:**

- One-hit wonders: `WHERE a.sightings = 1 ORDER BY a.last_seen DESC LIMIT 20`
  plus `COUNT(*)`. Join `enrichment`/`network_enrichment` for registration and
  type.
- Longest visits: `SELECT v.hex, v.start_ts, v.end_ts,
  v.end_ts - v.start_ts AS dur, v.polls, coalesce(...) FROM visits v
  LEFT JOIN ... ORDER BY dur DESC LIMIT 10`. A visit is a continuous pass
  (10-minute gap threshold), so duration is bounded by actual presence. Show
  `polls` next to it.
- Climb/descent: refactor `record()` into `records_by(conn, col, limit)` and
  call it with `max_baro_rate DESC` and `min_baro_rate ASC`. Keep the existing
  single-record callers working.
- Oldest airframes: `coalesce(nullif(e.built,''), n.year)` → first four
  characters → integer. Keep only 4-digit years in `1900..=current`, order
  ascending, limit 10. `built` may be `YYYY-MM-DD` or `YYYY`; `substr(...,1,4)`
  handles both. `YearRecord::year_str()` must return a plain year, not
  `fmt_num`'s `2,001`.
- Rarest types: reuse `type_dex()`, filter `caught > 0`, sort by
  `rarity_rank()` ascending, take 10; use `display_name()` and link to
  `/type/:code`.
- Top callsigns: `SELECT callsign, COUNT(DISTINCT hex), SUM(sightings) FROM
  aircraft_callsigns GROUP BY callsign ORDER BY 2 DESC, 3 DESC LIMIT 10`.
- Accessors: `VisitRecord::duration_str()` reuses the `fmt_duration` logic
  (move it from `web.rs` into `stats.rs` or duplicate it);
  `YearRecord::year_str()` returns the plain integer.

**Web:** `GET /records` and `GET /api/records`; `RecordsTemplate { records,
tar1090 }`. Add a "Records" pill to every existing nav and the new page.

**Template:** sections with the existing `.cards` / `.table-wrap` classes:
"Rare catches" (one-hit count card + table), "Longest overhead", "Climb and
descent" (two small tables), "Oldest airframes", "Rarest types", "Most-heard
callsigns". Link hexes to `/aircraft/:hex` and designators to `/type/:code`;
every section gets an empty state.

**Verify:** one-hit count + multi-hit count = total aircraft; longest visit
duration equals `end_ts - start_ts`; climb/descent signs match the profile
rows; `2001`, `2001-01-01`, and blank years parse correctly; rarest types
exclude zero-caught and links resolve; `/api/records` is valid JSON; an empty
database renders empty states.

**Commit:** `plane-stats: records and rarities page`

---

## Step 13 — Aircraft schedule and streaks

**Goal:** the aircraft profile answers "when does this one usually pass?" and
"how many days have I heard it?".

**Files:** `src/stats.rs`; `src/web.rs`; `templates/aircraft.html`;
`static/app.css`; `README.md`; `AGENTS.md`.

**No schema.**

**Stats:**

```rust
pub struct Schedule {
    pub hours: Vec<HourBucket>,      // 24, visit starts by local hour
    pub max_count: i64,              // for bar heights
    pub days_heard: i64,
    pub current_streak: i64,
    pub longest_streak: i64,
    pub typical_hour: Option<u32>,
    pub first_day: Option<String>,
    pub last_day: Option<String>,
}
pub fn schedule_for(db: &Db, hex: &str) -> rusqlite::Result<Schedule>;
```

- Visits: `SELECT start_ts FROM visits WHERE hex = ?1` (all rows; do not reuse
  the 200-row `visits_for` cap). Bucket with `chrono::Local` hour.
- Days: `SELECT DISTINCT day FROM day_aircraft WHERE hex = ?1 ORDER BY day`;
  parse `NaiveDate`.
- Pure helper `streaks(days: &[NaiveDate], today: NaiveDate) -> (current,
  longest)`: longest = max run of consecutive days (`succ_opt`); current = run
  ending at `today`, or at `today - 1` if not heard today yet (grace so the
  streak does not reset at midnight); else 0.
- `typical_hour` = argmax of `hours`, ties → earliest.
- Tests: empty, one day, consecutive run, gap, current ending yesterday,
  duplicate days.
- Note: `visits` and `day_aircraft` both start at deploy; there is no older
  history.

**Web:** `AircraftTemplate` gains `schedule`; `aircraft_profile` calls
`schedule_for`; `/api/aircraft/:hex` gains `schedule`.

**Template:** a "Schedule" panel before Visits:

- 24 CSS bars, one per local hour, height from
  `h.pct(schedule.max_count)`; labels every 3 hours; `title` = hour + visit
  count.
- Rows: days heard, current streak, longest streak, usual hour, first/last
  day.
- `<details>` table of all 24 hours for accessibility.

`app.css`:

```css
.sched-bars { display: grid; grid-template-columns: repeat(24, minmax(0, 1fr));
              align-items: end; height: 80px; gap: 2px; }
.sched-bar { height: var(--h); background: var(--accent); border-radius: 2px; }
```

**Verify:** `sum(hours) == SELECT COUNT(*) FROM visits WHERE hex = ?`;
`days_heard` equals the distinct-day count; streak unit tests; a profile with
no visits shows an empty state; `/api/aircraft/<hex>` includes `schedule`;
bars do not overflow on mobile.

**Commit:** `plane-stats: aircraft schedule and streaks`

---

## Step 14 — Flight routes (adsbdb)

**Goal:** the aircraft profile shows `origin → destination` for the callsign,
with the airline when known.

**Terms gate:** adsbdb's README states that the flight-route data "may not be
copied, published, or incorporated into other databases without the explicit
permission of David J Taylor". Therefore this step **must not** add a SQLite
table, a bulk crawl, or an export. Keep a bounded in-memory cache only, for
display on the profile. If persistence or a route page is wanted later, ask
permission first.

**Files:** new `src/routes.rs`; `src/main.rs` (mod + spawn); `src/web.rs`
(profile field, optional partial); `templates/aircraft.html`; new
`templates/partials/route.html` (optional); `docker-compose.yml`;
`README.md`; `AGENTS.md`.

**Config:**

| Variable | Default | Meaning |
|---|---|---|
| `ROUTE_LOOKUP` | `on` | `off` disables all requests |
| `ROUTE_API` | `https://api.adsbdb.com/v0` | API base |
| `ROUTE_TTL_SECS` | `604800` | positive cache TTL (7 days) |
| `ROUTE_NEG_TTL_SECS` | `2592000` | negative cache TTL (30 days) |
| `ROUTE_POLL_SECS` | `60` | resolver cycle |

**Data:**

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct Route {
    pub callsign: String,
    pub airline: Option<String>,
    pub origin_iata: String, pub origin_icao: String, pub origin_name: String,
    pub dest_iata: String, pub dest_icao: String, pub dest_name: String,
    pub fetched_at: f64,
}
struct CachedRoute { route: Option<Route>, fetched_at: f64 }
static CACHE: LazyLock<RwLock<HashMap<String, CachedRoute>>>;
```

**Resolver (`routes::run(db)`):**

1. If `ROUTE_LOOKUP=off`, log once and return.
2. Every `ROUTE_POLL_SECS`, collect candidates:
   `SELECT DISTINCT last_flight FROM aircraft WHERE last_seen > now - 3600
   AND last_flight IS NOT NULL`, plus
   `SELECT callsign FROM aircraft_callsigns WHERE last_seen > now - 7*86400
   GROUP BY callsign ORDER BY MAX(last_seen) DESC LIMIT 200`.
3. Normalize: trim, uppercase, keep only 2–8 `[A-Z0-9]`; dedup. Skip entries
   whose positive or negative cache entry is still fresh.
4. `GET {ROUTE_API}/callsign/{callsign}` with a 10 s timeout and
   `User-Agent: plane-stats/0.1 (personal ADS-B dashboard)`.
   - 200 → parse `response.flightroute`; `airline.name` is optional;
     `origin`/`destination` must have `iata_code`, `icao_code`, `name`.
   - 404 → negative cache (`route: None`).
   - 429 → sleep 60 s and stop this cycle.
   - Other errors → log and retry next cycle; do not cache.
5. Sleep 1 s between requests. Prune expired entries and cap the cache at
   5,000 (drop the oldest `fetched_at`).

**Read path:** `pub fn for_hex(db: &Db, hex: &str) -> Option<Route>` reads the
aircraft's `last_flight` (fallback: most recent `aircraft_callsigns` row),
normalizes, and looks up the cache. No network on the request path.

**Web:** `AircraftTemplate` gains `route: Option<routes::Route>`;
`aircraft_profile` calls `routes::for_hex`; `/api/aircraft/:hex` gains
`route`. In `aircraft.html`, add an Identity row after Operator callsign:

```html
{% if let Some(r) = route %}
<tr><th>Route</th><td>{{ r.origin_iata }} → {{ r.dest_iata }} ·
  {{ r.origin_name }} → {{ r.dest_name }}{% if let Some(a) = r.airline %} ·
  {{ a }}{% endif %}</td></tr>
{% endif %}
```

Show "Route data: adsbdb.com" with a link whenever a route is rendered.
Optional: `/partials/route/:hex` with `hx-get`/`hx-trigger="load"` so a
profile opened before the first resolver cycle fills in without a manual
refresh; the partial returns the row or an empty string.

**Verify:** with `ROUTE_LOOKUP=on`, wait one cycle, open a profile for a known
airline callsign (e.g. `ACA123`) and see `YYZ → YVR`; `curl
/api/aircraft/<hex> | jq '.route'`; logs show one request per callsign and no
repeat within the TTL; an unknown callsign is negative-cached; `ROUTE_LOOKUP=off`
makes no requests; the API being down leaves pages rendering; `SELECT name
FROM sqlite_master WHERE name LIKE '%route%'` returns nothing.

**Commit:** `plane-stats: flight routes from adsbdb (memory cache)`

---

## Rollout order for steps 9–14

1. **Step 9** country (schema + poller; backup first).
2. **Step 10** polar plot (schema + poller; backup first).
3. **Step 11** activity charts (read-only).
4. **Step 12** records page (read-only).
5. **Step 13** schedule/streaks (read-only).
6. **Step 14** routes (external dependency + terms gate).

Each step is its own change set: `cargo test` on the host, `cargo check`,
build, `up -d`, exercise the surface, update `README.md`/`AGENTS.md`, then
commit. Do not bundle steps.

Docs to touch per step:

- **9:** README data model (`aircraft.country`) and fleet filter; AGENTS
  country bullet (source table, `country_v1` stamp).
- **10:** README `/api/polar` and `range_sector`; AGENTS polar bullet
  (10° sectors, ground excluded, all-time + 30-day overlay).
- **11:** README `/api/activity` fields; AGENTS activity bullet (coverage
  gaps are `null`, not 0%).
- **12:** README `/records` and `/api/records`; AGENTS records bullet.
- **13:** README profile schedule; AGENTS schedule/streak bullet.
- **14:** README config rows (`ROUTE_*`) and the adsbdb terms note; AGENTS
  routes bullet (memory-only, no SQLite table, 1 req/s).

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
| 9. Country of registration | done | — |
| 10. Receiver coverage polar plot | done | — |
| 11. Activity traffic + coverage trends | done | — |
| 12. Records and rarities page | done | — |
| 13. Aircraft schedule and streaks | done | — |
| 14. Flight routes (adsbdb, memory cache) | done | — |
