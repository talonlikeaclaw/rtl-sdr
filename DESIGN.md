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
  faint-slate: "rgba(148, 163, 184, 0.5)"
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
    fontSize: "0.72rem"
    fontWeight: 600
    letterSpacing: "0.16em"
  body:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "0.84rem"
    fontWeight: 400
  label:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
    fontSize: "0.66rem"
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
low-opacity orbs drift behind it; every panel floats above that ground as a frosted
gauge face — a 160° white gradient at 5.5% falling to 1.8%, blurred, with a hairline
of light along its top edge and a faint ring set 5px outside it, like the bezel of a
cockpit dial. The header is closed by a 1px dashed line: a runway centerline.

The voice is night-watch quiet. This screen is looked at in a dark room, at a glance,
by one person checking what is overhead and what was new today. So the interface never
raises its own voice: no illustration, no empty decoration, no second accent. Colour is
carried almost entirely by data — Sky Blue for anything the operator can act on or
read as a series, Signal Mint for what is live or improving, Emergency Amber for one
thing only. Everything else is frosted white at low alpha.

Restraint here is not absence of design; it is the precision of a panel where each
mark has a job. Numbers are tabular so they do not jitter as they update. Uppercase is
reserved for labels at 0.66–0.72rem with 0.11–0.16em tracking, so the eye can skip
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
- **Faint Slate** (rgba(148, 163, 184, 0.5)): panel eyebrows, table headers, axis labels,
  card sublabels.
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
- **Title** (600, 0.72rem, 0.16em, uppercase, Faint Slate): panel eyebrows, each with a
  2px Sky Blue Dim bar before it.
- **Body** (400, 0.84rem): table content; 0.8rem for taglines and 0.78rem for pager and
  link cells.
- **Label** (600, 0.66rem, 0.11em, uppercase, Faint Slate): table headers, card
  sublabels, filter-group labels.
- **Mono** (400, 0.78rem): ICAO hex in the first table column, the footer line.

### Named Rules
**The Tabular Rule.** Every number that can change — card numerals, table cells,
histogram axes, chart labels — sets `font-variant-numeric: tabular-nums`. A value that
shifts the layout when it ticks is a bug.

**The One Voice Rule.** No second sans family. If a form control does not inherit the
stack, fix the control; do not add a font.

**The Tracked Caps Rule.** Uppercase only at 0.66–0.72rem with at least 0.1em tracking.
Body copy and data are never uppercase.

## Layout

One column, 1160px max, centred. Body padding is
`clamp(1.25rem, 4vw, 2.75rem) clamp(1rem, 4vw, 2.5rem) 4rem`, so the shell breathes on a
desktop and still fits a phone without a second layout.

- **Header** wraps: brand block left, action pills right, separated by `1rem 2rem` and
  closed by the dashed runway line.
- **Panels** stack with `1.25rem` between them and `1.2rem 1.35rem 1.35rem` internal
  padding.
- **Card grid** is `repeat(auto-fit, minmax(10.5rem, 1fr))` with `0.75rem` gaps — the
  count of stat cards changes the rows, never the column width.
- **Tables** live in a `.table-wrap` with `overflow-x: auto` and a 10px radius; the page
  never scrolls horizontally, the table does.
- **Live panel** is capped at `40vh` with `scrollbar-gutter: stable` and a sticky header,
  so a changing row count cannot push the rest of the page.
- **Charts** use a two-column grid, `auto minmax(0, 1fr)`, with a 2.1rem y-axis column;
  the SVG scales to width with `height: auto`.
- **Breakpoint:** one, at 640px — panel padding drops to `1rem 0.9rem 1.1rem`, card
  minimum to 9rem, blur to 14px, and the `.meta` suffix in the tagline is hidden.

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

A radius ladder, not a single radius: 16px panels, 13px cards, 10px inputs, tables,
charts and tooltips, 8px altitude fields, 999px pills and chips. The ladder is what
makes a card read as sitting inside a panel.

Recurring geometry:
- **2px accent ticks** — the bar before a panel eyebrow, the left-edge marker on each
  card, the gap between heatmap cells.
- **1px dashed rule** under the header — the runway centerline, at 14% white.
- **Circles** appear only as the three drifting orbs and the waveform bars' rounded caps.
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
5.2rem wide, right-sized numerals. **Known defect:** `input.search` does not inherit
`font-family` and currently computes to Arial; it must be set to the system stack.

### Charts and heatmap (signature)
The `/today` histogram is a 24-column CSS grid, 120px tall, with three hairline
gridlines at 0/50/100% and bars filled with a Sky Blue → Sky Blue Dim vertical gradient;
the hovered or focused column's bar turns Signal Mint and a fixed-position tooltip
appears. The `/activity` heatmap is a 7×24 grid with a 2.4rem day-label column, cells
`clamp(11px, 2vw, 22px)` tall at 3px radius, filled in five alpha steps of Sky Blue
(0.16 / 0.3 / 0.46 / 0.66 / 1.0) over a 3.5% white base, with the peak cell outlined in
Signal Mint. Line charts are inline SVG in a fixed 640×120 viewBox: 7% white gridlines,
a 12% Sky Blue area fill, a 1.6px Sky Blue line with round joins, and 3px dots that turn
Signal Mint on hover. All strokes use `vector-effect: non-scaling-stroke`.

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

### Don't:
- **Don't** let a form control fall back to the UA font. `input.search` currently
  computes to Arial because it does not inherit `font-family`; fix the control to the
  system stack instead of adding a family.
- **Don't** use Emergency Amber for anything but emergency squawks.
- **Don't** add scale, translate or bounce transitions. State changes are colour, border
  and background only.
- **Don't** fill a panel, card or row with Sky Blue; the selected chip is the single
  filled accent surface.
- **Don't** place a tooltip inside a `.glass` panel: `backdrop-filter` makes the panel the
  containing block for `position: fixed`, so the tooltip lands offset and under later
  panels. Keep it a direct child of `<body>`.
- **Don't** add a second sans family, a display serif, or an icon set; the inline SVG
  aircraft mark is the only logo asset.
