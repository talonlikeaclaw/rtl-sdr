---
name: plane-stats
description: Every aircraft heard on 1090 MHz — first seen, last seen, records.
colors:
  sky-blue: "#9ec5e8"
  sky-blue-dim: "rgba(158, 197, 232, 0.55)"
  signal-mint: "#6ee7b7"
  emergency-amber: "#fbbf24"
  instrument-text: "#e8edf4"
  data-text: "rgba(232, 237, 244, 0.85)"
  muted-slate: "rgba(203, 213, 225, 0.6)"
  faint-slate: "#8b97a8"
  hairline-stroke: "rgba(255, 255, 255, 0.09)"
  glass-top: "rgba(255, 255, 255, 0.055)"
  glass-bottom: "rgba(255, 255, 255, 0.018)"
  table-well: "rgba(6, 10, 16, 0.32)"
  night-ground: "#070a0f"
typography:
  display:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "clamp(1.35rem, 3vw, 1.7rem)"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.015em"
  headline:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "clamp(1.4rem, 2.4vw, 1.7rem)"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "-0.02em"
  title:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 600
    letterSpacing: "0.16em"
  body:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "0.84rem"
    fontWeight: 400
  label:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "0.72rem"
    fontWeight: 600
    letterSpacing: "0.11em"
  mono:
    fontFamily: "ui-monospace, SF Mono, Menlo, Consolas, monospace"
    fontSize: "0.78rem"
    fontWeight: 400
rounded:
  sm: "8px"
  md: "10px"
  lg: "13px"
  xl: "16px"
  pill: "999px"
spacing:
  hair: "2px"
  xs: "0.45rem"
  sm: "0.75rem"
  md: "1.25rem"
  lg: "2.25rem"
components:
  panel:
    backgroundColor: "{colors.glass-top}"
    rounded: "{rounded.xl}"
    padding: "1.2rem 1.35rem 1.35rem"
  card:
    backgroundColor: "{colors.glass-top}"
    rounded: "{rounded.lg}"
    padding: "1rem 1.1rem 1.05rem"
  pill:
    backgroundColor: "{colors.glass-top}"
    textColor: "{colors.muted-slate}"
    rounded: "{rounded.pill}"
    padding: "0.4rem 0.8rem"
  chip:
    backgroundColor: "{colors.glass-top}"
    textColor: "{colors.muted-slate}"
    rounded: "{rounded.pill}"
    padding: "0.32rem 0.72rem"
  chip-active:
    backgroundColor: "{colors.sky-blue}"
    textColor: "#07121f"
    rounded: "{rounded.pill}"
    padding: "0.32rem 0.72rem"
  input-search:
    backgroundColor: "{colors.glass-top}"
    textColor: "{colors.instrument-text}"
    rounded: "{rounded.md}"
    padding: "0.55rem 0.9rem"
    width: "100%"
  table-header:
    textColor: "{colors.faint-slate}"
    typography: "{typography.label}"
    padding: "0.5rem 0.8rem"
  table-cell:
    textColor: "{colors.data-text}"
    padding: "0.5rem 0.8rem"
---

# Design System: plane-stats

## Overview

**Creative North Star: "The Flight Deck"**

plane-stats is a dark instrument panel that happens to be a web page. The ground is
near-black (`#070a0f`) with a faint 72px grid that fades out at the edges; three
low-opacity orbs sit behind it, fixed and unmoving; every panel floats above that
ground as a frosted gauge face — a 160° white gradient at 5.5% falling to 1.8%,
blurred, with a hairline of light along its top edge and a faint ring set 5px outside
it, like the bezel of a cockpit dial. The header is closed by a 1px dashed line: a
runway centerline.

The voice is night-watch quiet. This screen is looked at in a dark room, at a glance,
by one person checking what is overhead and what was new today. So the interface never
raises its own voice: no illustration, no empty decoration, no second accent. Colour is
carried almost entirely by data — Sky Blue for anything the operator can act on or
read as a series, Signal Mint for what is live or improving, Emergency Amber for one
thing only. Everything else is frosted white at low alpha.

Restraint here is not absence of design; it is the precision of a panel where each
mark has a job. Numbers are tabular so they do not jitter as they update. Uppercase is
reserved for labels at 0.7–0.75rem with 0.11–0.16em tracking, so the eye can skip
them. Panels are the composition: the page is a vertical stack of gauge faces, and the
only thing that moves is the data inside them.

**Key Characteristics:**
- Frosted glass panels over a near-black instrument ground; depth is material, not state.
- One cool data hue (Sky Blue), one liveness hue (Signal Mint), one alarm hue (Emergency Amber).
- System sans at three registers; mono only for machine strings (hex codes, footer).
- Tabular numerals everywhere a value can change.
- State changes are colour, border and background — never scale, bounce or translate.

## Colors

The palette is a single cool data hue and two functional signal hues over tinted
near-black. There is no warm neutral anywhere: every neutral is slate pushed toward
blue.

### Primary
- **Sky Blue** (#9ec5e8): links, active sort column, chart line and dots, heatmap peak
  cell, the one selected filter chip, the 2px accent tick on cards and panel eyebrows.
  It is the only hue allowed to mean "this is data or this is interactive".
- **Sky Blue Dim** (rgba(158, 197, 232, 0.55)): the hex column in tables, focus borders
  on inputs, the panel-eyebrow bar, the heatmap's mid steps. Used where the full accent
  would compete with the numeral next to it.

### Secondary
- **Signal Mint** (#6ee7b7): the live waveform in the header, the record-up value on
  `/today`, the hovered histogram bar, the peak-cell outline on the heatmap. It marks
  liveness and improvement — never a general success state.

### Tertiary
- **Emergency Amber** (#fbbf24): emergency squawk codes, and nothing else.

### Neutral
- **Night Ground** (#070a0f): body background, under a vertical gradient
  `#070a0f → #080b11 → #06080c` plus two radial tints (blue at the top, teal at the
  bottom-right) at 10% and 5%.
- **Instrument Text** (#e8edf4): headings, card numerals (`#f1f5f9`), body copy.
- **Data Text** (rgba(232, 237, 244, 0.85)): table cells.
- **Muted Slate** (rgba(203, 213, 225, 0.6)): taglines, pill and chip labels, pager text.
- **Faint Slate** (#8b97a8): panel eyebrows, table headers, axis labels, card sublabels.
  Solid, not alpha — at 50% alpha it measured 2.7:1 on the panel and the label layer
  failed WCAG AA. Solid it measures 6.0:1 on panels and 5.4:1 on cards.
- **Hairline Stroke** (rgba(255, 255, 255, 0.09)): every border and divider.
- **Glass Top / Glass Bottom** (rgba(255,255,255,0.055) → rgba(255,255,255,0.018)): the
  160° gradient that makes every surface.
- **Table Well** (rgba(6, 10, 16, 0.32)): the recessed background behind tables and charts.

### Named Rules
**The Rarity Rule.** Sky Blue never fills a surface except the one selected chip. It
marks data and interaction; it is not a background.

**The Amber Rule.** Amber means an emergency squawk. No other state, badge or warning
may borrow it.

**The Mint Rule.** Mint marks liveness or a broken record. It is not a generic success
colour and never appears on a static value.

## Typography

**Display Font:** system-ui stack (`system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`)
**Body Font:** the same stack
**Label/Mono Font:** `ui-monospace, "SF Mono", Menlo, Consolas, monospace`

**Character:** one voice in three registers. The sans does all reading and all
numerals; mono appears only where the value is a machine string (ICAO hex, the footer
credential line). Nothing is decorative: the hierarchy is built from size, weight,
tracking and case, not from a second family.

### Hierarchy
- **Display** (600, clamp(1.35rem, 3vw, 1.7rem), line-height 1.1, -0.015em): page titles
  (`plane-stats`, `Today`, `Activity`).
- **Headline** (600, clamp(1.4rem, 2.4vw, 1.7rem), line-height 1.15, -0.02em,
  tabular-nums): card numerals — the biggest thing on the page after the title.
- **Title** (600, 0.75rem, 0.16em, uppercase, Faint Slate): panel eyebrows, each with a
  2px Sky Blue Dim bar before it.
- **Body** (400, 0.84rem): table content; 0.8rem for taglines and empty notes, 0.78rem
  for pager and link cells, 0.75rem for chart captions.
- **Label** (600, 0.72rem, 0.11em, uppercase, Faint Slate): table headers, card
  sublabels, filter-group labels.
- **Mono** (400, 0.78rem): ICAO hex in the first table column, the footer line.

### Named Rules
**The Tabular Rule.** Every number that can change — card numerals, table cells,
histogram axes, chart labels — sets `font-variant-numeric: tabular-nums`. A value that
shifts the layout when it ticks is a bug.

**The One Voice Rule.** No second sans family. If a form control does not inherit the
stack, fix the control; do not add a font.

**The Tracked Caps Rule.** Uppercase only at 0.7–0.75rem with at least 0.1em tracking.
Body copy and data are never uppercase.

### Browser surfaces

The parts we did not draw still ship with the design, and their defaults belong to no
palette here. Three are themed from the tokens:

- **Selection** is `rgba(158, 197, 232, 0.3)` with Instrument Text on top. Chrome's
  default is a saturated blue from outside the system.
- **The caret** is full Sky Blue. Left at `auto` it inherits the text colour, which makes
  a focused search field look inert.
- **The link underline** is `1px` at `0.18em` offset. The UA default lands on the
  descenders of 13px text.

Scrollbars are left native: `color-scheme: dark` already draws them in the right family,
and a hand-rolled track would be the one surface that is neither the browser's nor the
system's.

## Layout

One column, 1160px max, centred. Body padding is
`clamp(1.25rem, 4vw, 2.75rem) clamp(1rem, 4vw, 2.5rem) 4rem`, so the shell breathes on a
desktop and still fits a phone without a second layout.

- **Header** wraps: brand block left, action pills right, separated by `1rem 2rem` and
  closed by the dashed runway line.
- **Panels** stack with `1.25rem` between them and `1.2rem 1.35rem 1.35rem` internal
  padding.
- **Page order** answers the product's two questions first: *Overhead now* → *Summary* →
  *All-time fleet* → *Recent new arrivals* → *Emergency log*. The live table leads because
  "what is in the sky" is the question the operator opens the page with.
- **Card grid** is `repeat(auto-fit, minmax(10.5rem, 1fr))` with `0.75rem` gaps — the
  count of stat cards changes the rows, never the column width. Cards are grouped under
  `.group-label` eyebrows (*Right now* / *Ledger* / *Records*) with a hairline rule
  running to the right edge, so thirteen tiles read as three ideas.
- **Tables** live in a `.table-wrap` with `overflow-x: auto` and a 10px radius; the page
  never scrolls horizontally, the table does. A `local`/`scroll` background pair draws a
  14px vignette at each edge — sized to the cell padding, so it never reaches the content.
  The `local` left layer travels with the content and fades out at the start; the `scroll`
  right layer is pinned to the edge. It is a soft well edge, not a "more to the right"
  indicator: an attachment pair cannot express "visible unless at the end", and at 24px
  the right layer covered the last column's links once the table was scrolled fully.
- **The fleet table pins its hex column** (`position: sticky; left: 0`) so the row keeps
  its identity while the remaining twelve columns scroll under it on a phone.
- **A table that can grow without bound is capped** with `scrollbar-gutter: stable` and a
  sticky header. Two cases, one pattern: the live panel at `40vh` (a changing row count
  must not push the rest of the page) and the `/today` "New today" tail at `60vh` (a busy
  day adds hundreds of first sightings; uncapped, 154 rows made a 5,940px digest). Every
  row stays reachable by scrolling — nothing is hidden behind a link that does not exist.
- **The fleet filter label column** is `min-width: 7.4rem`, not the `6.4rem` it started
  at: "Max altitude" measures 108px and wrapped at the old basis, which made that one row
  taller than the others. `flex: 0 0 auto` with a `min-width` keeps every label on one
  edge and lets a longer one grow visibly instead of wrapping silently.
- **Charts** use a two-column grid, `auto minmax(0, 1fr)`, with a 2.1rem y-axis column;
  the SVG scales to width with `height: auto`.
- **Breakpoint:** one, at 640px — panel padding drops to `1rem 0.9rem 1.1rem`, card
  minimum to 9rem, blur to 14px, the `.meta` suffix in the tagline is hidden, filter
  labels take a full line above their chips, and heat cells keep a 14px floor.

## Elevation & Depth

Layered glass, and it is the identity. Every surface is the same material: a 160°
white gradient at 5.5%→1.8%, `backdrop-filter: blur(20px) saturate(135%)`, a 1px inset
top highlight, one deep shadow, and a 1px outline set 5px out — the instrument bezel.
A 1px gradient hairline sits along the top edge of each `.glass` panel, brightest in the
middle, as the highlight of a glass face.

Depth is material, not state: nothing lifts, scales or shadows on hover. A row changes
its background by 3.5% white; a header cell by 5%. That is the whole elevation
vocabulary.

### Shadow Vocabulary
- **panel-lift** (`0 18px 44px -26px rgba(0, 0, 0, 0.85)`): every `.glass` panel and card.
- **inset-highlight** (`inset 0 1px 0 rgba(255, 255, 255, 0.1)`): the top edge of panels,
  cards, pills and chips.
- **bezel-ring** (`outline: 1px solid rgba(255, 255, 255, 0.035); outline-offset: 5px`):
  `.glass` only.
- **tip-shadow** (`0 8px 24px -12px rgba(0, 0, 0, 0.9)`): the histogram tooltip.
- **chip-active-glow** (`0 0 0 1px rgba(158,197,232,0.25), 0 4px 14px -8px rgba(158,197,232,0.7)`):
  the selected filter chip, the only element that glows.

### Named Rules
**The Bezel Rule.** The 5px-offset outline belongs to `.glass` panels. Cards, pills,
chips and inputs get the inset highlight and nothing else.

**The No-Lift Rule.** Hover and focus change colour, border or background. They never
change transform, size or shadow.

## Shapes

A radius ladder, not a single radius: 16px panels, 13px cards, 10px inputs, tables and
charts, 8px altitude fields, 999px pills and chips. The ladder is what makes a card read
as sitting inside a panel.

Component-local geometry sits outside the ladder and is not a step: 3px heat cells, 2px
top corners on histogram bars, 1px waveform caps, 7px on the chart tooltip, 12px on the
logo tile.

Recurring geometry:
- **2px accent ticks** — the bar before a panel eyebrow, the left-edge marker on each
  card, the gap between heatmap cells.
- **1px dashed rule** under the header — the runway centerline, at 14% white.
- **Circles** appear only as the three fixed orbs and the waveform bars' rounded caps.
- **Hairline borders everywhere**: 1px at 9% white on panels and controls, 4.5% on table
  rows, 5% on wells.

## Components

### Pills (header navigation)
Small frosted capsules, 999px radius, `0.4rem 0.8rem` padding, 0.72rem/500 at 0.04em
tracking, Muted Slate text. They are links, not buttons: no fill change on hover, the
text brightens. The live pill carries the four-bar Signal Mint waveform, each bar
2.5px wide with a 2px gap, animating `scaleY(0.3 → 1)` on a 1.3s ease-in-out loop with
staggered negative delays.

### Chips (fleet filters)
Same capsule as a pill but smaller (`0.32rem 0.72rem`, 0.72rem/500). Unselected: glass
gradient, Hairline Stroke border, Muted Slate. Hover: text to Instrument Text, border to
18% white. Selected: **filled Sky Blue with `#07121f` text** plus the chip-active glow —
the only filled accent surface in the system. The real checkbox is visually hidden
(`clip: rect(0 0 0 0)`), so focus lands on the label via `:focus-visible` with a 2px
Sky Blue Dim outline at 2px offset.

### Cards (stat cards)
13px radius, `1rem 1.1rem 1.05rem` padding, glass gradient, inset highlight. A 2px Sky
Blue Dim tick at the top-left edge, 0.7rem tall. Inside: numeral (Headline, tabular),
sublabel (Label, uppercase, Faint Slate), and an optional `who` line at 0.72rem in Muted
Slate that links to the aircraft profile in Sky Blue.

### Panels
The composition unit. `.glass` + `1.2rem 1.35rem 1.35rem` padding + `1.25rem` bottom
margin. Each opens with an uppercase eyebrow carrying a 2px accent bar, and its content
is a table-wrap, a card grid, or a chart.

### Tables
Recessed in a Table Well with a 10px radius. Header cells: Label type, Faint Slate, 2.5%
white background, `user-select: none`; sortable headers show a pointer cursor, brighten
on hover, and the active one turns Sky Blue with a ▾/▴ glyph. Rows: 0.84rem tabular, Data
Text, 4.5% white bottom border, hover background 3.5% white. The first column is mono at
0.78rem in Sky Blue Dim — the hex code is the row's identity.

### Inputs
Search field: full width, 10px radius, glass gradient, 0.85rem, placeholder in Muted
Slate, focus border in Sky Blue Dim (no ring, no glow). Altitude fields: 8px radius,
5.2rem wide, right-sized numerals. Both take a 2px Sky Blue Dim halo on focus
(`box-shadow`), because a 1px border shift alone is not a focus indicator.

### Sortable headers
A sortable `<th>` contains a real `<button class="sort">`, not a bare `hx-get` on the
cell. The button inherits the header's type, carries the `▾`/`▴` glyph as a `::after` on
`.active`, and takes a 2px Sky Blue Dim `:focus-visible` outline at 3px offset. The `<th>`
carries `aria-sort="ascending|descending"` only when it is the active column. Sorting must
never be mouse-only.

### Pager
`Prev` and `Next` are `<button type="button">` for the same reason the sort headers are:
an `<a>` carrying only `hx-get` and no `href` takes no focus and has no link role, which
left the pager mouse-only. The button strips its chrome (`border: 0; background: none;
font: inherit`) so it reads as the text it replaced, keeps the 24×24 hit area from the
shared padding/negative-margin pair, and takes a 2px Sky Blue Dim `:focus-visible` outline.
At the first or last page the control is a `disabled` button, not a dead link; the
accessible name is `Prev page` / `Next page`, which contains the visible label.

### Status pill (poller health)
The fourth header pill is the only element allowed to report the receiver's state. It
carries `.status` plus one of `.live` / `.stale` / `.down`:

| State | Trigger | Colour | Waveform |
|---|---|---|---|
| live | last poll < 30 s | Signal Mint dot | animating |
| stale | < 5 min | Emergency Amber dot and border | frozen at 30% |
| down | ≥ 5 min, or no polls | red dot and border | frozen at 30% |

The visible label is `live · 4 s ago`. When the state is not `live`, the page also emits a
single `role="alert"` sentence (`Receiver stalled — last poll 6 m ago`) and nothing
otherwise, so a screen reader hears the transition once rather than every tick.

### Filter summary
When any filter is on, `partials/table.html` opens with a `.filter-summary` line —
`Showing Military · Heard today` — and a `Clear all` link to `/`. Each chip also carries
its matching row count in `.chip-n`, so a flag that matches nothing says `0` instead of
returning an empty table with no explanation.

### Error page
`templates/error.html` is the one error surface, and every failure reaches it: an unknown
hex, an unmatched path (the router's fallback), and a 500 from a locked database or a
failed render. All of them are a page in the same shell as every other page — axum's
default is a bare `404 Not Found` in plain text, which is the one thing this app does not
have. A 404 carries the Display title `Not in the ledger`, a Muted Slate tagline naming
the problem, and one panel whose eyebrow is `What happened` over a single `.empty-note`
sentence: what happened, then the way back. A 500 keeps the shell and swaps the title for
`Something broke` and the sentence for the recovery (`docker logs plane-stats`). No
illustration, no apology, no second accent. `/api/*` answers JSON instead, so a client
never has to parse a page.

A path segment is user input: `/aircraft/<anything>` validates as one to eight hex digits
before it reaches the database or the page, and the rejected text is never echoed back.

### Connection notice
`#net-note` is a fixed line at the bottom of the viewport, a direct child of `<body>` —
`.glass` uses `backdrop-filter`, which makes it the containing block for fixed
descendants, so a notice inside a panel would be positioned against the panel. htmx leaves
a failed swap in place and says nothing, so a 500 or a dropped connection used to look
exactly like a receiver that had gone quiet: every panel froze on its last good data with
no explanation. The notice names the failure and the state of the data (`These panels are
showing their last good data.`), carries the same amber hairline as the poller pill, and
clears on the next successful request. It is `role="status"`, so it is announced rather
than only seen.

### Empty states
An empty table body is a state, not a blank. `.empty-row` gives it room, the Muted Slate
voice, and the one action that leaves it: a filtered fleet offers `Clear them`, an empty
ledger says when the first row lands, an empty emergency log names the squawks that would
fill it. The live panel is the exception that proves the rule — an empty list means "clear
skies" only while polls are landing, so with the poller stalled or down the same cell
carries `PollerHealth::sentence()` in Emergency Amber instead. An empty panel must never
let a dead receiver read as a quiet sky.

A key-value panel is the same rule. The profile's Identity panel is one table of
`<th>`/`<td>` pairs, and an aircraft with no OpenSky row used to render a single
`Registration` row holding `-` — a one-column table saying nothing. It now renders one
`.empty-row` across both columns: `No registration data. This hex is not in the OpenSky
database — the ledger still has every time the receiver heard it.` The panel still answers
the question it exists to answer, which is "why is this blank".

### Charts and heatmap (signature)
The `/today` histogram is a 24-column CSS grid, 120px tall, with three hairline
gridlines at 0/50/100% and bars filled with a Sky Blue → Sky Blue Dim vertical gradient;
the hovered or focused column's bar turns Signal Mint and a fixed-position tooltip
appears. The `/activity` heatmap is a 7×24 grid with a 2.4rem day-label column, cells
`clamp(14px, 2vw, 22px)` tall at 3px radius, filled in five alpha steps of Sky Blue
(0.16 / 0.3 / 0.46 / 0.66 / 1.0) over a 3.5% white base, with the peak cell outlined in
Signal Mint. The day labels are `justify-content: flex-end` so they form one right edge
against the grid; left-aligned, the `0.3rem` gutter fell *after* the text and the labels
floated in a 38px column. Line charts are inline SVG in a fixed 640×120 viewBox: 7% white
gridlines, a 12% Sky Blue area fill, a 1.6px Sky Blue line with round joins, and 3px dots
that turn Signal Mint on hover. All strokes use `vector-effect: non-scaling-stroke`.

Every chart ships two paths to the same numbers, because neither one covers touch,
keyboard and screen reader alone:

- **The grid is one tab stop.** `/app.js` gives each cell `data-tip` and an
  `aria-label`, collapses the cells to a roving `tabindex`, and moves with
  ←/→/↑/↓ plus Home/End. One shared `#chart-tip` follows hover, focus and tap.
  A tap is the only path on touch, since `title` never appears there.
- **A `<details class="chart-table">` holds the data as a real table** with
  `<caption>` and `scope` headers. It is the screen reader's path, the touch
  user's fallback when a cell is 10px wide, and the only way to read an exact
  value rather than a shade of blue.

The heatmap is an ARIA `grid`: `role="row"` wrappers use `display: contents` so
the 25-column CSS grid is untouched, and each cell carries the full slot in its
label ("Mon 14:00 · 5 aircraft") so the sparse hour header never has to be
announced.

## Copy

The voice is night-watch quiet: plain sentences, no exclamation, no marketing. One
operator reads this at a glance in a dark room, so every label names the thing it
counts and the thing it does.

### Glossary

One noun per concept, everywhere. These are the words; anything else is a bug.

| Word | Means | Never |
|---|---|---|
| **overhead** | heard within the last 60 s | visible, airborne, live (except the poller pill) |
| **heard** | the receiver decoded a message from it; the verb for counts and states (`heard today`, `most heard`) | spotted, tracked |
| **seen** | the ledger's record of when (`first seen`, `last seen`, `polls seen`, `not seen this week`) | as a verb for receiving |
| **polls seen** | per-aircraft count of 10 s poll cycles (≈ airtime) | sightings, hits |
| **poll cycle** | one pass of the poller over `aircraft.json` | poll, when a count of aircraft is nearby |
| **new today** | first heard today, ever | new to me, new to the receiver |
| **visit** | one continuous pass, split by a 5 min gap | session, flight |
| **map** / **trace** | tar1090 deep links for a hex | trail, history, flight path |

### Rules

- **A numeral carries its unit.** `7` over `busiest hour` says nothing; `7 aircraft`
  does. Cards put the unit in `<span class="unit">`, table headers put it in
  parentheses (`Max alt (ft)`). A supporting line that counts the same thing still names
  it: `558 new in the last 7 days`, not `558 in the last 7 days`.
- **A label that names the direction does not repeat it.** `min_baro_rate` is stored as
  the most negative climb, so `Max descent (ft/min)` renders its magnitude (`1408`).
  `Max climb` keeps the explicit sign (`+2176`, `-704`) because "Climb" does not name a
  direction and an aircraft that only ever descended has a negative max.
- **Name the action, not the gesture.** `Open live map`, `Show trace`, `Clear all`.
  A link's text must survive being read out of context, so table links carry an
  `aria-label` with the hex: `Show c0603c trace`.
- **An empty state says which kind of empty it is.** No filters and no rows is
  `No aircraft recorded yet.`; filters with no match is `No aircraft match these
  filters.`; a chart with no data names what will fill it.
- **A missing value is `-`**, in tables and key-value lists alike. Not `unknown`,
  not `n/a`.
- **An error page is a page.** `templates/error.html` states what was not found,
  what that implies, and the way back. A bare `unknown aircraft c0603c` body is not
  an error message.
- **Jargon stays where the audience owns it.** `squawk`, `hex`, `nm`, `kt`, `dBFS`,
  `PIA`/`LADD` and `Cat` are the operator's own words. `fix` is not: it reads as a
  repair. Say `position` or `data`.

## Do's and Don'ts

### Do:
- **Do** build every surface from the 160° glass gradient and the 1px Hairline Stroke; a
  flat fill is only for the Table Well and the selected chip.
- **Do** set `font-variant-numeric: tabular-nums` on every changing value.
- **Do** open each panel with a tracked uppercase eyebrow and its 2px accent bar.
- **Do** use Sky Blue Dim for focus borders and the hex column, full Sky Blue for links
  and active state.
- **Do** keep the 640px breakpoint the only layout break, and let tables scroll
  horizontally inside `.table-wrap` rather than the page.
- **Do** respect `prefers-reduced-motion` — the stylesheet already collapses all
  animation and transition durations.
- **Do** keep every functional label at 11px or larger and at 4.5:1 or better; the
  detector's `undersized-ui-text` and `low-contrast` rules are the floor, not a style
  opinion.
- **Do** give every auto-updating region a `<caption class="sr-only">` and `scope="col"`
  headers, and keep the skip link first in `<body>`.
- **Do** give every text-sized control a 24×24 hit area (WCAG 2.5.8) by growing its
  padding into the surrounding whitespace with an equal negative margin: `.pager-link`,
  `td a`, `.chart-table > summary`, and `th .sort` (whose padding matches the `<th>`'s,
  so the whole header cell is the target). The layout must not move.
- **Do** let the card grid reflow on its own width. `.card-group` is a container and
  `.cards` drops to 2 columns below 30rem and 1 below 10rem, so 200% text collapses the
  grid instead of clipping every numeral.

### Don't:
- **Don't** leave a bare `<a>` unstyled: `color-scheme: dark` hands it Chrome's UA
  lavender, which is not in the palette. The global `a { color: var(--accent) }` rule
  exists for exactly this.
- **Don't** use Emergency Amber for anything but emergency squawks. The poller's `stale`
  state is the one exception, and it is a status, not an alarm.
- **Don't** treat the emergency log as an alert. It is an archive of events that
  happened — the owner's words — so it stays a normal panel at the bottom with no toast,
  no badge and no assertive live region.
- **Don't** add scale, translate or bounce transitions. State changes are colour, border
  and background only.
- **Don't** size a card grid by viewport width alone. A fixed 2- or 4-column count clips
  the content at 200% text on every screen; the container query is what keeps 1.4.10
  (Reflow) true. Every `.cards` needs a `.card-group` parent, or it has no container to
  query.
- **Don't** let a form field sit under 16px on a phone. iOS zooms the viewport on focus
  and never zooms back; the mobile rule at the end of the stylesheet is the floor.
- **Don't** convey a chart's values through `title` alone, and never ship a chart without
  its `<details class="chart-table">` twin. `title` does not appear on tap or on focus.
- **Don't** animate the background orbs, or add `will-change` to them. Every
  `backdrop-filter` surface above them re-samples its backdrop each frame: measured
  3–5 fps with the drift and 43–61 without, on the Pi. Static orbs are a performance
  contract, not a style choice.
- **Don't** put `backdrop-filter` on a `.card` or the search field. Both only ever sit
  on a `.glass` panel, whose backdrop is already blurred, so the second blur is
  invisible and costs a raster each. Blur belongs to the surfaces that meet the raw
  body: `.glass`, `.pill`, `.mark`.
- **Don't** fill a panel, card or row with Sky Blue; the selected chip is the single
  filled accent surface.
- **Don't** place a tooltip inside a `.glass` panel: `backdrop-filter` makes the panel the
  containing block for `position: fixed`, so the tooltip lands offset and under later
  panels. Keep it a direct child of `<body>`.
- **Don't** add a second sans family, a display serif, or an icon set; the inline SVG
  aircraft mark is the only logo asset.
