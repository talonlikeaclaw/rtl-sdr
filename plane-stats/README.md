# plane-stats

Read-only ledger and dashboard for every aircraft the RTL-SDR hears. A small
Rust service (axum + tokio + rusqlite) polls tar1090's `aircraft.json` every
10 s, stores what it sees in SQLite, and serves a server-rendered web UI plus a
JSON API on port **8081**.

Part of the `sdr` Docker Compose project; runs under the `adsb` profile next to
readsb and tar1090. The local and network pollers are the only writers.

## Pages

| Path             | What it shows                                                                                                          |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `/`              | Fleet table: search, sort, filter chips, pagination (100/page), live "overhead now" panel, summary cards               |
| `/aircraft/:hex` | Aircraft profile: identity/country, visits, schedule/streaks, callsigns, records, route, registration/type enrichment, emergency history, tar1090 map/trace links |
| `/today`         | Daily digest: unique/new aircraft, hour histogram, records vs prior days                                               |
| `/coverage`      | Receiver polar coverage plus local vs adsb.fi within 100 NM: overlap, only-you, missed                                  |
| `/activity`      | 7×24 weekday/hour heatmap, range/messages/unique/new/network-coverage charts                                           |
| `/types`         | Type dex: every ICAO type designator heard, with photo and catch stats                                                 |
| `/type/:code`    | Type profile: ICAO Doc 8643 specs, Wikipedia extract/photo, Wikidata facts, airframes seen                             |
| `/records`       | One-hit wonders, longest overhead passes, steepest climbs/descents, oldest airframes, rarest types, top callsigns      |
| `/countries`     | Every state of registry heard, ranked by airframes, with share, types, best range and last heard                       |

tar1090 deep links (`?icao=<hex>`, `?showTrace=<hex>`) render only while an
aircraft is live (last seen within 60 s).

## JSON API

All endpoints return JSON and are read-only.

| Path                            | Content                                              |
| ------------------------------- | ---------------------------------------------------- |
| `/api/aircraft`                 | Full fleet list (no pagination)                      |
| `/api/aircraft/:hex`            | One aircraft                                         |
| `/api/live`                     | Aircraft heard in the last 60 s                      |
| `/api/emergencies`              | Emergency squawk log                                 |
| `/api/today`                    | `{summary, hours, records, new}`                     |
| `/api/coverage`                 | Local vs network live sets                           |
| `/api/feed`                     | Feeder client, network API and snapshot status       |
| `/api/activity`                 | Heatmap and range/messages/unique/new/coverage series |
| `/api/polar`                    | Receiver polar coverage by 10° sector                |
| `/api/records`                  | Records and rarities                                 |
| `/api/types`, `/api/type/:code` | Type dex and one designator                          |
| `/api/countries`                | States of registry with aircraft, share, types, best range and last heard |
| `/api/stats`                    | All-time aggregates (uncached; UI uses a 10 s cache) |

HTMX partials live under `/partials/{table,table/rows,cards,emergencies,live,coverage,feed}`.

## Run it

From the repository root:

```bash
docker compose --profile adsb up -d plane-stats
curl -s localhost:8081/api/live | head -c 400
```

Rebuild after code changes (templates and `static/` are baked into the binary):

```bash
docker compose --profile adsb build plane-stats   # ~4 min on the Pi
docker compose --profile adsb up -d plane-stats
docker logs --tail 20 plane-stats
```

## Configuration

| Variable            | Default                                       | Purpose                                                    |
| ------------------- | --------------------------------------------- | ---------------------------------------------------------- |
| `DB_PATH`           | `/data/planes.db`                             | SQLite database (WAL). Parent directory is created         |
| `AIRCRAFT_JSON_URL` | `http://tar1090:80/data/aircraft.json`        | Poll source                                                |
| `ENRICHMENT_CSV`    | `/usr/share/plane-stats/aircraftDatabase.csv` | OpenSky metadata CSV, baked into the image                 |
| `TAR1090_URL`       | derived from request `Host` (:8080)           | Browser-facing tar1090 base for deep links                 |
| `RX_LAT`, `RX_LON`  | unset                                         | Receiver position; enables the live panel's bearing column and network poller |
| `ADSB_FI_API`       | `https://opendata.adsb.fi/api`                | adsb.fi v3 aircraft/snapshot API base                      |
| `ADSB_FI_WEB_API`   | `https://api.adsb.fi`                         | adsb.fi stats/coverage API base                            |
| `NETWORK_RADIUS_NM` | `100`                                         | Network comparison radius, clamped 10–250 NM               |
| `NETWORK_POLL_SECS` | `10`                                          | v3 aircraft poll interval (minimum 5 s)                    |
| `FEEDER_CHECK_SECS` | `35`                                          | Feeder snapshot check interval (minimum 10 s)              |
| `NETWORK_STATS_SECS`| `300`                                         | Network totals refresh interval (minimum 60 s)             |
| `NETWORK_COVERAGE_SECS` | `600`                                     | Nearby-feeder coverage refresh interval (minimum 60 s)     |
| `FEED_JSON`         | `/run/adsbfi-feed/aircraft.json`              | Host feeder status file, mounted read-only                 |
| `ROUTE_LOOKUP`      | `on`                                          | `off` disables all adsbdb requests                         |
| `ROUTE_API`         | `https://api.adsbdb.com/v0`                   | adsbdb API base                                            |
| `ROUTE_TTL_SECS`    | `604800`                                      | Positive route cache TTL (7 days)                          |
| `ROUTE_NEG_TTL_SECS`| `2592000`                                     | Negative route cache TTL (30 days)                         |
| `ROUTE_POLL_SECS`   | `60`                                          | Route resolver cycle                                       |
| `TZ`                | container default                             | Local day/hour bucketing (`chrono::Local`)                 |
| `RUST_LOG`          | `plane_stats=info`                            | Log filter                                                 |

In Compose, `RX_LAT`/`RX_LON` are wired from `READSB_LAT`/`READSB_LON`.

## Data model

SQLite tables are created on boot (`CREATE TABLE IF NOT EXISTS`); schema changes
are additive via `NEW_COLUMNS` in `src/main.rs`.

- `aircraft` — one row per hex: first/last seen, sightings, callsign/squawk/
  category, state of registry (`country`, from ICAO address blocks), all-time
  records, last position/track, `db_flags`, last live state (`last_*` columns
  drive the overhead panel).
- `visits` — one row per continuous pass (a gap > 10 min starts a new visit).
- `aircraft_callsigns` — callsigns seen per hex.
- `poll_log` / `poll_hour` — per-poll and hourly aircraft counts; `poll_log` is
  pruned at 90 days.
- `day_aircraft` — per local day per hex: first/last seen, polls, max
  altitude/speed/range. Never pruned.
- `network_day_aircraft` — per local day per hex from adsb.fi, airborne only:
  first/last seen, polls, max altitude/speed/range, MLAT flag. Never pruned.
- `network_enrichment` — adsb.fi registration/type/description/owner/year cache.
- `enrichment` — OpenSky registration/type/owner data (~520k rows), plus
  `enrichment_meta` stamps for the CSV and background-task TTLs.
- `emergency_log` — squawk/emergency events.
- `aircraft_types` / `type_wiki_map` — cached Doc 8643 specs and Wikipedia
  profiles for the type dex.
- `range_sector` — best range per local day per 10° bearing sector (36 rows/day,
  never pruned); feeds `/coverage` and `/api/polar`. A one-shot backfill reads
  tar1090's persisted heatmap (`tar1090_globe_history` mounted at
  `/globe_history`) for the days before plane-stats started recording sectors.

`aircraft` and `visits` are never pruned. The database lives on the named
volume `sdr_plane_stats_db`.

Network data comes from [adsb.fi](https://adsb.fi/); the poller sends
`plane-stats/0.1 (adsb.fi feeder)` and stays within the public 1 req/s limit.

Flight routes come from [adsbdb.com](https://www.adsbdb.com/) under a
memory-only cache: routes are never written to SQLite or exported, per the
provider's terms. Set `ROUTE_LOOKUP=off` to disable the resolver.

### Enrichment reload

The ~94 MB OpenSky CSV is re-inserted only when its size+mtime differs from the
stamp in `enrichment_meta`. First boot after an image/CSV change takes ~1 min;
later boots are seconds. The CSV is downloaded in its own Docker stage, so
source rebuilds reuse it. Bump `CSV_VERSION` to force a fresh download:

```bash
docker compose --profile adsb build --build-arg CSV_VERSION=2 plane-stats
```

To force a reload without a rebuild, delete the stamp:

```sql
DELETE FROM enrichment_meta;
```

There is no `sqlite3` in the image; use a throwaway container.

## Type dex background task

`types::run` refreshes ICAO Doc 8643 monthly, the Wikipedia designator map
weekly, and profiles for types heard monthly. Requests are batched (20 titles /
50 QIDs) and spaced 2 s apart; a 429 backs off 60 s and retries three times,
then waits for the next hourly cycle. Every failure is non-fatal — pages render
with whatever is cached.

## Development

`cargo` works on the host (Nix-provided); Docker is the deployment path.

```bash
cd plane-stats
cargo check
cargo build --release
```

Templates (`templates/`) and `static/` are compiled into the binary with
`include_bytes!`, so a rebuild is required after editing them. `Cargo.lock` is
committed on purpose.

Local run against a live tar1090:

```bash
DB_PATH=./planes.db \
AIRCRAFT_JSON_URL=http://localhost:8080/data/aircraft.json \
TAR1090_URL=http://localhost:8080 \
cargo run
```

## Operations

Back up the volume before schema work:

```bash
docker run --rm -v sdr_plane_stats_db:/data -v "$PWD":/backup alpine \
  tar czf /backup/planes.tgz -C /data .
```

A nightly copy also lands in `/mnt/truenas/talon/pi-backups/daily/` (03:00
timer). Do not put the SQLite database on CIFS — locking is unreliable.

## More context

- `../AGENTS.md` — build loop, schema rules, tar1090 quirks, refresh model.
- `PLAN.md` — step-by-step roadmap and progress table.
