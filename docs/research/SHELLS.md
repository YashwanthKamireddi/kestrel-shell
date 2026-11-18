# How the popular shells are built (research, 2026-11-15)

Read from source (shallow clones, 2026-11-15), not from screenshots. Seven shells:

| Shell | Stack | Shape |
|---|---|---|
| illogical-impulse (end-4, "ii") | Quickshell / QML | the reference rice; Material 3, many features |
| ii-p3drovfx (P3DROVFX) | QML, fork of ii-vynx ← ii | Material 3 Expressive, the most features; "not focused on performance or stability" (its README) |
| Caelestia | QML + C++ plugin | Material 3 tokens as typed C++ config; drawers |
| Noctalia (v5) | **native C++**, Wayland + OpenGL ES, no Qt/GTK | rewritten from QML; the most disciplined architecture |
| DankMaterialShell (DMS) | QML UI + Go core (CLI, socket server) | widest compositor support, plugins, many OSDs |
| HyprPanel | AGS / TypeScript | huge settings surface; quiet since 2026-04 |
| ashell | **Rust / iced** (like kestrel) | small, modules, typed theme scale |

## 1. Architecture and modularity

**Noctalia** is the model for a native shell (its ARCHITECTURE.md):
- One poll loop; every source (Wayland, D-Bus, inotify, timers) is a `PollSource`.
- Explicit per-frame phases: PrepareFrame (animations) → Update (state) → Layout (geometry,
  structure) → Render (paint only). Setters *schedule* work: `requestUpdate`, `requestLayout`,
  `requestRedraw`, `requestFrameTick`. Nothing renders from inside Wayland dispatch.
- A UI library (`lib/`) with no shell policy, reusable by other apps; shell code may only use
  library controls (`Label`, `Button`, `Box`, ...), never raw scene nodes.
- Compositor specifics behind a `CompositorPlatform`; protocol transport has no compositor
  branches.
- Config schema split per area; settings entries declared per page in a registry; user state in
  `state.toml` under owner tables (no ad hoc state files).
- Plugins: a directory with one `plugin.toml`; entry kinds widget / shortcut / service /
  launcher provider / panel / layer; defaults declared once.
- Toast slots stay stable for a toast's life; hovered toasts hold.

**DMS** splits UI (QML) from a Go core that owns the CLI, IPC socket, clipboard, screenshots,
plugins, keybind parsing, privilege escalation. Its contributor rules: the shell runs 24/7, audit
every change for idle CPU / processes / timers / memory; **QML uses Theme tokens, never hard-coded
colours, spacing or constants**; use the shared D* widgets, never raw ones.

**Caelestia** keeps services (`Audio`, `Hypr`, `Nmcli`, `Notifs`, `Players`, ...) as singletons,
modules per surface, components shared; its config is typed C++ nodes with defaults.

**ii / p3drovfx**: services + modules + `common/widgets`; one `Appearance` singleton for every
colour, size, font and animation; config in one JSON schema. p3drovfx adds a very large feature
set on top (AI chat, phone link, routines, screen time, live draw) at the cost of stability.

**ashell**: modules per file, services, components; a theme with typed scales
(`SpaceSize`, `RadiusSize`, `FontSizeScale`) resolved by the theme, so modules never write a
number.

**kestrel today**: services crate, a module registry, panels per file, a vendored layer shell.
The structure is sound. What is missing against Noctalia/DMS is the *discipline layer*: panels
draw with raw numbers and hand-picked colours (measured below), and there is no rule that they
must use the shared components.

## 2. Design systems

| | Spacing | Radius | Type | Control heights | Motion |
|---|---|---|---|---|---|
| Noctalia | 4 8 12 16 | 3 6 9 12 (scalable) | 11 13 14 16 20 | 32 38 44 | 100 / 200 / 400 ms |
| Caelestia | 4 8 12 16 20 28 32 48 | 4 8 12 16 20 28 32 48 full | 11 12 13 15 18 28 | — | M3 durations, expressive curves |
| ii | — | scale object | 10…23 | bar 40 | M3 Expressive curves 200–650 ms |
| ashell | enum scale | enum scale | enum scale | — | — |
| **kestrel** | 283 literal spacings | **94 literal radii, 15 values** (2…20) | 11 12 13 14 16 32 | 30 (bar) | springs, hover instant |

Interaction states:
- **Material 3 / Caelestia**: one *state layer* on every interactive element — the content colour
  laid over the container at 8 % on hover, 10–12 % pressed, with a press ripple; pointing-hand
  cursor on everything clickable.
- **ii**: hover background colour animated (~200 ms), ripple on press.
- **Noctalia**: per-variant palettes (Default, Primary, Secondary, Destructive, Outline, Ghost,
  Tab) with normal / hover / pressed / disabled / selected; changes animate 100 ms ease-out-cubic.
  Ghost buttons are clear at rest, like kestrel's "Clear all" — but fade in, so nothing pops.
- **kestrel**: 87 separate `.colors(rest, hover, press)` triples and ~15 different ink alphas;
  hover snaps on instantly. A clear-at-rest button therefore flashes a pill from nothing: the
  "cheap and glitchy" hover in a screen recording (hit-testing was measured exact).

Text: Noctalia centres text on the measured cap height, never the line box, and never pre-rounds
text positions (1 px drift otherwise).

## 3. OSDs

| OSD | ii | Caelestia | Noctalia | DMS | kestrel |
|---|---|---|---|---|---|
| Volume / brightness | ✓ | ✓ | ✓ | ✓ | ✓ |
| Mic volume / mute | | option | (audio) | ✓ | — |
| Caps / Num Lock | | | ✓ | ✓ | ✓ |
| Keyboard layout | | | ✓ | ✓ | ✓ |
| Keyboard backlight | | | ✓ | | — |
| Media (track change, play/pause) | | | ✓ | ✓ | — |
| Audio output switched | | | | ✓ | — |
| Do not disturb / airplane / caffeine / power profile / touchpad | | | | ✓ each | — |
| Privacy (mic/camera/screen in use) | | | ✓ | | bar module only |
| Per-OSD on/off, position, "always show value" | | on/off | | ✓ | — |
| Click OSD → detail sheet (e.g. output picker) | | | | ✓ | click opens control centre |

## 4. Customisation

- DMS and HyprPanel: hundreds of options, per-widget settings, per-OSD switches and positions.
- Noctalia: `settings.toml` written by the GUI over defaults; settings searchable; per-widget
  named settings; palettes as versioned JSON; community templates.
- Caelestia: every token configurable (rounding, spacing, padding, font sizes, durations).
- kestrel: config.toml as single source, Settings writes it, Lua widgets/plugins, templates. Not
  yet exposed: a corner-radius scale, density, per-OSD switches.

## 5. Performance and stability

- Noctalia: native, GPU, explicit phases, async image caches, no work in render.
- DMS: idle-CPU audit rule; patched Quickshell fork for start-up.
- p3drovfx: openly unstable; Quickshell breaks on Qt updates (CachyOS case).
- kestrel: 0 % idle CPU, ~76 MB, CPU rendering with damage tracking, every frame built in
  ~0.1–2 ms (measured). Its risk is not speed but inconsistency.

## 6. What kestrel takes (in order)

1. **A design kit, enforced** (docs/DESIGN.md): typed token scales (space, radius, type, control
   height, state alphas, motion) and one component set (button variants, icon button, chip, row,
   card, field, switch, slider, segmented, section heading) with one state-layer model. Panels may
   not write raw radii, paddings or colour triples; a check fails the build if they do.
2. **Hover/press feel**: state layer that eases in and out over ~80–100 ms (fast enough not to
   read as lag, never a pop), hand cursor on clickable things, press darkening.
3. **OSDs**: mic mute/volume, media track change, audio output switched, DND / airplane /
   caffeine / power mode toggled from keys, privacy; per-kind on/off in Settings.
4. **Architecture rules written down** (as Noctalia's ARCHITECTURE.md): update vs layout vs
   redraw, services own their D-Bus names, state in state.toml owner tables, no raw drawing in
   panels.
5. Not taken: AI chat, phone link, routines, screen time (bloat, or decided against).
