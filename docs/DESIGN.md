# kestrel design

kestrel is one calm, solid surface around the screen. The bar is the top edge of that surface;
every panel grows out of the bar, under the thing you clicked, and sinks back into it. Nothing
floats, nothing appears from somewhere unexpected.

This document is the contract. Every screen is built only from the tokens and components below.
If a screen needs something that isn't here, it is added here first.

## Principles

1. **One material.** Bar, frame and panels are the same opaque `base` colour and join with
   smooth concave curves. Cards and controls inside a panel sit one step lighter (`raised`).
2. **The accent means "on".** The wallpaper's accent colour is used only for active state:
   the current workspace, a switched-on tile, a slider's fill, today's date, the focused row, the
   primary button. Never as decoration, never for headings. Everything else is ink on surface.
3. **Every pixel is earned.** No placeholder text, no empty boxes, no controls that do nothing.
   If a value is unknown it is hidden, not shown as "Unknown".
4. **Motion has weight.** Panels open on a spring with a slight settle and close quickly and
   cleanly. Controls respond on hover and press within one frame. Nothing animates while idle.
5. **Concentric corners.** An inner radius is the outer radius minus the padding between them,
   so curves stay parallel.
6. **A 4 px grid.** All sizes and gaps come from the spacing scale.

## Tokens

### Colour (dark)

Computed in OKLCH from the wallpaper's dominant colour (hue `h`, chroma `c`). If the wallpaper
is nearly grey, the hue falls back to 255 (blue) at low chroma.

| Role | Lightness | Chroma | Use |
|---|---|---|---|
| `base` | 0.16 | 0.010 | bar, frame, panels |
| `raised` | 0.215 | 0.013 | cards, tiles (off), slider tracks, inputs |
| `raised_hi` | 0.25 | 0.015 | hover on raised, hover on base controls |
| `raised_press` | 0.28 | 0.016 | pressed |
| `line` | 0.30 | 0.012 | hairlines, dividers |
| `ink` | 0.95 | 0.006 | primary text and icons |
| `ink_dim` | 0.74 | 0.010 | secondary text |
| `ink_faint` | 0.54 | 0.010 | tertiary text, empty dots, disabled |
| `accent` | 0.80 | clamp(c, 0.07, 0.13) | active state |
| `on_accent` | 0.20 | 0.040 | text and icons on accent |
| `accent_soft` | 0.30 | 0.050 | selected row, open-panel button |
| `good` / `warn` / `bad` | 0.80 / 0.84 / 0.70 | 0.13 / 0.13 / 0.17 | hues 150 / 85 / 25 |

### Type

Rubik (chosen 2026-11-12). Sizes 11 (caption), 12 (bar, secondary), 13 (body), 14 (titles), 16 (panel headings),
32 (display), 88 (`text::HERO`: the lock screen's clock, docs/LOCK.md). Weights Regular 400, Medium 500, SemiBold 600. No bold, no letter-spacing tricks.

### Space and shape

One token set, `theme::tok` (`theme::size` was merged into it on 2026-11-16); panels use the
names, never numbers. `tools/lint/tokens.sh` counts literal radii, paddings, gaps, edges, alphas,
colours and icon sizes outside `src/ui` and `src/theme`; the count is zero and must stay zero.

| Tokens | Values |
|---|---|
| Spacing `XXS…XXXL` | 2 · 4 · 6 · 8 · 12 · 16 · 20 · 24 (`INDENT` 28, `HAIR` 1) |
| Radii | `R_PANEL()` 24, `R_CARD()` 12 (= 24 − `PANEL_PAD`), `R_ROW()` 10, `R_CONTROL()` 8, `R_JOIN()` 4, `FULL` (a pill). Functions since 2026-11-18: each is multiplied by `[look] radius` (default 1); pills are never scaled |
| Heights | `H_SMALL` 28, `H_CONTROL` 32, `H_ROW` 40, `H_ROW2` 48; bar `BAR` 40, its round items `BAR_ITEM` 30 |
| Panels | widths `PANEL_S` 320, `PANEL_M` 400, `PANEL_L` 560, `PANEL_XL` 860; padding `PANEL_PAD` 12; `JOIN` 16 (the concave curve to the bar) |
| Icons | `ICON_XS` 12, `ICON_S` 14, `ICON` 16, `ICON_M` 18, `ICON_L` 20, `ICON_XL` 22, `ICON_XXL` 28, `ICON_HERO` 40, `ICON_DISPLAY` 52 (the weather panel's condition). Material Symbols Rounded, filled, weight 500, from an embedded subset font (assets/fonts/kestrel-icons.ttf, built by tools/icons) |
| State layer (ink alpha) | `HOVER` .08, `PRESS` .14, `SELECTED` .14, `TONAL` .06, `FILL` .08, `FILL_STRONG` .10, `RING` .55 (a selection ring), `DISABLED` .38 |

Measures that only make sense in one place (a table's columns, a fixed button width) are named
constants in that file, shared by everything that must line up with them (header and rows).

### Motion

Motion tokens (2026-11-18), one named set in `theme::tok` like ii's presets. They are read only
through `crate::motion` (`Spring::to`, `motion::secs`, `motion::glide`), where `[motion] speed`
and `reduce` apply (docs/CUSTOMIZE.md). `tools/lint/tokens.sh` counts literal springs,
durations and rates anywhere outside `src/theme` and `src/motion.rs` (src/ui included).

| Token | Value | Use |
|---|---|---|
| `SPRING_OPEN` | ω 19.5, ζ 0.85 (~280 ms, 0.6 % overshoot) | a sheet or the island growing |
| `SPRING_CLOSE` | ω 30, ζ 0.95 (~160 ms) | closing, a press going down |
| `SPRING_SHOW` | ω 60, ζ 1 (~70 ms) | something appearing, the clipboard's glide |
| `SPRING_SNAP` | ω 24, ζ 0.86 | a press springing back, a swiped card returning |
| `SPRING_LEAD` / `SPRING_TRAIL` | ω 60 / 45, ζ 1 | the workspace pill's stretch (leading / trailing edge) |
| `HOVER_IN` / `HOVER_OUT` | 70 / 120 ms | the state layer |
| `GLIDE_SCROLL` / `GLIDE_SLIDER` | 26 / 19 per second | smooth wheel scrolling; a slider following its value |
| `BREATH` | 1.6 s | a pulsing icon |
| `FADE_IN` / `FADE_LATE` / `FADE_BOX` | 0.05–0.55 / 0.4–0.9 / 0.4–1 of progress | content fading with an opening panel / a closing panel or sheet / a closing panel's box |

Not motion, so never scaled: `HOLD_FLASH` 2.2 s, `HOLD_FLASH_LONG` 4 s, `HOLD_OSD` 1.4 s,
`HOLD_EVENT` 2.8 s (how long things stay to be read), `FLING` 0.2 s and `WHEEL_IDLE` 0.12 s
(swipe gestures), `METER_RISE` / `METER_FALL` / `METER_GLIDE` (the visualiser following sound).

The table below is the original intent; the tokens above are what the code does.

| What | How |
|---|---|
| Panel opens | spring, damping 0.82, ~340 ms to settle; content fades in after 35 % and rises 6 px |
| Panel closes | 180 ms ease-in-out; content fades out in the first 40 % |
| Panel switches | position, width and height spring together; content cross-fades |
| Hover | background fades in 110 ms ease-out |
| Press | scale to 0.96 on press, spring back on release |
| Active workspace | pill width and position spring between workspaces |
| OSD / toast | spring in from the bar, 160 ms fade out |

## Layout

```
╭─────────────────────────────────────────────────────────────────────────╮
│ ⌘  ● ━━ ● ·  kitty — ~/Projects      16:06 · Thu 12 Nov    ♪ title  ▣ ▣  ⌔ ᛒ ◖ 82%  🔔  ⏻ │
│╭───────────────────────────────────╮     ╭───────────────╮               │
││                                   ╰─────╯   calendar     │               │
```

- **Left:** launcher button, workspaces (dots; the active one is a wide accent pill), active
  window title.
- **Centre:** time (SemiBold) and date (dim). Opens the calendar.
- **Right:** now-playing chip (opens media), tray icons, a status cluster (network, Bluetooth,
  volume, battery; opens the control centre), notifications bell, power.

A button whose panel is open shows `accent_soft` behind an `accent` icon.

## Panels

| Panel | Width | Contents |
|---|---|---|
| Control centre | 360 | header (battery, lock, power); 2×3 tiles (Wi-Fi, Bluetooth, Do not disturb, Night light, Caffeine, Power mode); card of sliders (volume, microphone, brightness). Wi-Fi, Bluetooth and the volume output open sub-pages inside the panel |
| Media | 380 | art, title, artist, player; seek bar with times; previous / play-pause / next. Player chips only when two different things play at once |
| Calendar | 320 | today's time and date; month grid; today is an accent circle |
| Notifications | 380 | header with Do not disturb and Clear all; cards grouped by app; empty state |
| Tray menu | ≥ 220 | the app's menu: rows with icons, checks, separators; sub-menus slide in |
| Power | 220 | Lock, Suspend, Log out, Restart, Shut down; destructive rows confirm on a second click |
| Launcher | 560 | search field; app results with icons; `=` calculator, `>` command, `:` clipboard |
| Toasts | 380 | the notification card, under the bell, auto-dismissed |
| OSD | 240 | icon, level bar, value; under the clock |

## Enforcement and interaction states (2026-11-15)

The contract above was right; the code drifted from it. Measured on 2026-11-15: 94 literal radii
in 15 values (2…20, against the contract's 24 / 12 / 10 / 8 / pill), 214 literal paddings, 283
literal gaps and 87 hand-picked `.colors(rest, hover, press)` triples with ~15 different ink
alphas. So nothing looked quite like anything else. Popular shells prevent this by rule (Noctalia and DMS
forbid raw values); kestrel does it with a lint.

**Tokens in code.** `theme::tok` holds this document's scales as named constants: spacing
(`XXS 2 · XS 4 · S 6 · M 8 · L 12 · XL 16 · XXL 20 · XXXL 24`), radii (`PANEL 24 · CARD 12 ·
ROW 10 · CONTROL 8`, pill = half the height), control heights, state alphas, motion. Panels use
these names, never numbers.

**One state layer** (Material 3's model, as Caelestia and ii use it): every interactive element is
drawn by `ui::control`, and its hover / pressed / selected looks come from the same ink alphas
laid over its container:

| State | Ink over the container | Timing |
|---|---|---|
| Hover | 0.08 | eases in 70 ms, out 120 ms (ease-out); fast enough to read as instant, never a pop |
| Pressed | 0.14 | at once |
| Selected / open | 0.14 | at once |
| Disabled | content at 0.38 | no hover |

Fills at rest have three levels (`tok`): `TONAL` 0.06 (fields, chips, quiet tiles), `FILL` 0.08
(slider and meter tracks, picture placeholders), `FILL_STRONG` 0.10 (an icon's circle on a
card). Fields, keycaps and app icons are one kit function each (`ui::input_style`,
`ui::keycap`, `ui::app_icon`). `tools/lint/tokens.sh` also counts struct paddings
(`top: 10.0`), spacer gaps (`Space::new().height(8)`) and ink alphas, so none of them drift
back.

The motion table above said hover fades in over 110 ms; it later snapped on instantly (the "no
lag" rule), which made clear-at-rest buttons (Clear all, ×) flash a pill out of nothing as the
pointer crossed them: the "cheap and glitchy" hover seen in a recording of 2026-11-15. 70 ms keeps
it instant to the eye without the pop.

**Disabled.** A button that cannot be pressed is `ui::button(s, words, Pill, None)`: its words at
`DISABLED` (.38) and a filled button's fill a .12 ink layer (Material 3's disabled state). No
panel draws its own disabled look.

**Keyboard.** The keyboard's place is the control's pressed tone, shown at once (no ring, no
animation: keyboard selection in Raycast and Spotlight); Tab and the arrows move it
(`panels::focus_step`, `ui::focus`), scoped to the open panel or window. When its control
goes (a row removed), the same kind of control nearest its place takes it: the next row, or
the previous after the last (`ui::focus::keep`, at every panel's and Settings' root). The bar
is reached with `kestrel focus bar` (docs/CUSTOMIZE.md); a bar button shows the keyboard as
it shows the pointer (its veil lifts), since its press has no tone of its own.

**Components.** Buttons come in four variants only (Primary: accent fill · Tonal: faint ink fill ·
Ghost: clear at rest · Danger), plus the icon button, chip, row, card, section heading, field,
switch, slider and segmented control. A screen built from anything else adds it here first.

**The check.** `tools/lint/tokens.sh` counts literal radii, paddings, gaps and colour triples
outside `src/ui` and `src/theme`; `make check` fails if a count rises above the recorded baseline,
which only goes down as panels move onto the kit.

## Nothing moves under the pointer (2026-11-15)

A list that is open keeps its rows where they are: Wi-Fi networks and nearby Bluetooth devices
are sorted when their page opens, then found ones join at the end and lost ones drop out
(`control::keep_order` / `in_order`); a rescan used to reorder them by signal or name under
the pointer. A change the user started holds the picture they chose until it lands: switching a
Bluetooth mode keeps the device and the new mode shown while PipeWire rebuilds the output
(`App::mode_switch`), instead of flashing the speaker for a second.
