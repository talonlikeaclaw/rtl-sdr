# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

One operator: the station's owner, on a Raspberry Pi 4 at home. Single-user
tool — no accounts, roles, sharing, or onboarding. Primarily opened in a
desktop browser on the LAN; also opened on a phone, so every page must survive
a narrow viewport and touch input.

## Product Purpose

Record every aircraft heard on 1090 MHz and make that record legible: what is
overhead right now, what was heard today, which aircraft are new, and which
records fell. A good day for the station is visible at a glance.

## Positioning

tar1090 answers "what is in the sky this second" and forgets. plane-stats is
the memory: a read-only ledger that outlives the map — per-aircraft first/last
seen, visit history, per-day ledgers, activity rollups, all-time records. The
mechanism a neighbouring dashboard could not copy without doing the same thing:
a poller reads tar1090's `aircraft.json` every 10 s and upserts into SQLite, so
the history is durable and queryable rather than a live view.

## Operating Context

- One RTL-SDR Blog V4L dongle, one Docker Compose profile up at a time; the
  radio is exclusive. `adsb` is the only profile built today.
- tar1090 (live map) on port 8080, plane-stats on 8081, Beast feed on 30005.
- The station runs headless on the Pi; the browser is the only interface.
- Local time is America/Montreal (`TZ`); all day/hour bucketing uses it.
- Poll cadence 10 s; "live" means heard within the last 60 s.
- Daily 03:00 backup of the DB to a NAS share; schema steps are preceded by a
  manual backup.
- Roadmap profiles (AIS, satellite, ACARS, POCSAG, radiosondes, feeders) will
  each bring their own app; plane-stats stays the ADS-B ledger.

## Capabilities and Constraints

Capabilities: fleet table with search, sort, pagination (100/page) and flag
filters (category, military, interesting, privacy PIA/LADD, emergency history,
heard today, new this week, dormant, regulars, altitude range); aircraft
profile page with visits, callsigns and records; live "overhead now" panel
(5 s); `/today` digest (summary, 24-hour histogram, records vs prior days, new
today); `/activity` (7×24 weekday/hour heatmap, range-by-day, messages-by-day);
emergency squawk log; tar1090 map/trace deep links; JSON APIs mirroring each
page; OpenSky CSV enrichment for registration, type, owner and operator.

Constraints and terminology:

- `hex` = ICAO 24-bit address; "visits" = distinct passes; "polls seen" ≈
  airtime in 10 s units; `dbFlags` bit 0 military, 1 interesting, 2 PIA,
  3 LADD.
- Only the poller writes. The web layer is read-only.
- Schema changes are additive only (`ALTER TABLE … ADD COLUMN`,
  `CREATE TABLE IF NOT EXISTS`); no destructive migrations.
- `aircraft` and `visits` are never pruned; `poll_log` is pruned at 90 days;
  `day_aircraft` is never pruned.
- No backfill: daily ledger data starts at deploy.
- Bias tee is powered (antenna LNA) and must not be disabled without asking.
- `app.css` and htmx are compiled into the binary; the stylesheet URL carries
  `?v=2` to break long-cached copies.
- Numbers shown for an aircraft must agree with tar1090 for the same hex.

## Brand Commitments

Name is `plane-stats` (lowercase, hyphenated). The incumbent tagline is "Every
aircraft heard on 1090 MHz — first seen, last seen, records." No logo assets
exist beyond an inline SVG aircraft mark. No marketing voice, no claims about
users or coverage beyond the station's own data.

## Evidence on Hand

Real: a live SQLite ledger on the Pi, the OpenSky enrichment CSV (~94 MB,
~520k rows), and the running station itself. Absences future work must not
fabricate: no other users, no testimonials, no press, no analytics, no public
URL, no coverage or accuracy claims.

## Product Principles

1. **The ledger is the product.** History is never silently lost; migrations
   are additive and reversible by backup.
2. **Current state first, detail on demand.** The first screen answers "what is
   overhead" and "what happened today" without a click.
3. **One operator.** No accounts, permissions, or first-run ceremony; a page
   that needs explaining is a page to simplify.
4. **Local truth.** Times, days and hours are the station's local time; a
   number that disagrees with tar1090 is a bug.
5. **Boring operations.** Single writer, pinned dependencies, additive schema,
   backups before schema steps.
