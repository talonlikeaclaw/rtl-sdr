---
version: 1
slug: "countries"
primary_target: "countries"
related_targets: ["route:/","route:/api/countries"]
---

# Surface brief: /countries

## Scope

- New route `/countries`, JSON twin `/api/countries`.
- Surface mode: Operate. One operator scans the ledger, then opens a country's aircraft.
- Related surfaces: the dashboard countries card and the shared section nav.

## Direction contract

THESIS: The registry is a ranked ledger, not a choropleth. One table holds every state of registry the receiver has decoded, ordered by airframes; the share bar is the only graphic, because a country's weight in the ledger is the question.

OWN-WORLD: The Flight Deck, inherited unchanged: near-black instrument ground, a frosted glass panel, a recessed table well, Sky Blue data marks, tracked-caps labels and tabular numerals. No map, no flags beyond the registry flag already used on the dashboard, no second accent.

STORY: The operator sees the full registry list, how much of the ledger each country is, how many types it brought, its best range, and when it was last heard, then opens that country's aircraft in the fleet.

FIRST VIEWPORT: Brand, nav, then one full-width panel. The "States of registry" eyebrow opens onto sort chips and a six-column table; the top registry row carries the widest share bar. Country names are links to `/?country=XX`; count, share, types, best range and last heard follow.

FORM: Ranked table with an inline share bar. Position: extension of the established ledger surfaces (Types, Records), no new visual world.

seed: none (established world — extension, no roll)

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.
