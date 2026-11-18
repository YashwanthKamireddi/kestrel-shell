# Design systems of the popular shells, and kestrel's (research, 2026-11-16)

Read from source (shallow clones in /tmp/research, 2026-11-16). Companion to SHELLS.md
(architecture). Question: why does kestrel look random, and how do the best shells stay one
system?

## 1. Their token scales (numbers from source)

| Shell | Radii | Spacing / padding | Text | Control heights | Bar |
|---|---|---|---|---|---|
| Caelestia (`plugin/src/Caelestia/Config/tokens.hpp`) | 4 8 12 16 20 28 32 48, full | the **same** 4…48 scale for both | 11 12 13 15 18 28 | from tokens | 40 inner width; frame 10 px, frame radius 25 |
| Noctalia (`lib/ui/style.h`) | 3 6 9 12 (one user scale factor) | 4 8 12 16; card and panel pad 14 | 11 13 14 16 20 | 32 38 44 | 34 |
| DMS (`Common/Theme.qml`) | M3 shape scale (xs 4 … l 16, full); grouped list 16 outer / 4 inner | 2 4 8 12 16 | 12 14 (+ scale) | 28 32 40 56 | n/a |
| ii (`modules/common/Appearance.qml`) | 2 6 8 12 17 23 30, full | ad hoc per widget | 10 12 13 15 16 17 19 22 23 | ad hoc | 40 |
| ashell (`src/theme.rs`, iced) | 4 8 16 32 | 4 8 12 16 24 32 48 | 8 10 12 14 16 20 32 | bar layout derived from height | config |
| HyprPanel (SCSS) | per-element variables (bar button, menu, card) | variables | variables | — | — |

What the consistent ones share:
- **One scale for everything.** Caelestia uses the identical 4-8-12-16-20-28-32-48 ladder for
  radius, spacing and padding; nothing outside it. Noctalia: four radii, four spaces, three
  control heights, five text sizes. ii (the least consistent) has nine text sizes and radii
  like 17 and 23.
- **Sizes derive, they are not chosen per widget.** Noctalia: "Bar capsule cross-size derives
  from bar thickness, not widget content." ashell: `BarLayout::from_appearance` computes every
  module's height from the bar height. So every bar item is the same height by construction.
- **Concentric corners.** Inner radius = outer radius − padding (DMS grouped lists, Caelestia
  frame → drawers, kestrel already for panel 24 / pad 12 / card 12).
- **Shell code may not draw raw shapes.** Noctalia: shell code uses only library controls
  (`Label`, `Button`, `Box`, `Separator`), never `RectNode`. One place decides how a box looks.
- **Every surface has a home.** Caelestia `modules/drawers/Panels.qml`: one window per screen
  holds the frame and every panel; each panel has a fixed edge (OSD right-centre, notifications
  top-right, launcher bottom-centre, dashboard top-centre, bar pop-outs at their bar item,
  utilities bottom-right, session/sidebar right) and they shift to make room for each other
  (the OSD moves left while the sidebar is open). All shapes are drawn as one SDF so panels
  grow out of the frame with smooth joins.
- **Global, not per-component, effects.** Noctalia: one shadow style, components only say
  shadow on/off; blur regions always published, no per-component toggles.

## 2. Kestrel today (audit, 2026-11-16)

- 21 panels, 17 different widths: 232 260 280 312 330 360 392 400 420 440 460 560 720 860 960
  1300, plus computed ones (emoji, wallpaper).
- Two parallel token sets, `theme::tok` and `theme::size` (CARD_RADIUS = R_CARD, ROW_RADIUS =
  R_ROW, CONTROL_RADIUS = R_CONTROL...). Spacing ladder 2 4 6 8 12 16 20 24 (6 and 20 are off the
  4-grid scale the others use).
- Radii in use: FULL, 12, 10, 8, 15 (CONTROL/2), 4, and literal 2 3 5 7 (lint baseline 6).
- Bar items: each sizes itself from its content; a circle (avatar), pills (clock, weather), a
  ring (headphones), a capsule group (status) with different heights and insets.
- Text sizes 11 12 13 14 16 32.

## 3. What to copy (proposal)

1. One ladder for radius, spacing and padding: 4 8 12 16 20 24 32 48 (Caelestia/M3). Drop 2 and
   6 except the 2 px grouped-list gap (DMS) and hairlines.
2. One token module (merge `size` into `tok`).
3. Bar: one item height derived from the bar height (Noctalia/ashell); every module is either
   a pill or a circle of exactly that height; groups share one capsule.
4. Panel widths from three sizes (small / medium / large) plus full-width surfaces (overview,
   keys), instead of 17 numbers.
5. Homes: a table of where every surface appears and what it moves aside for (Caelestia).
6. Text: five sizes (Noctalia: 11 13 14 16 20).
