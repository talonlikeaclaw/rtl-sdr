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
