# kestrel architecture

kestrel is a desktop shell for Hyprland: bar, panels, notifications, launcher, OSD, wallpaper and
app theming, in one Rust program. This document is the contract for how the code is organised.
New code follows it; code that doesn't is a bug.

## Goals

1. **Fast and quiet.** 0 % CPU when idle, nothing polls unless a feature that needs polling is
   switched on, small memory, start in well under a second.
2. **Stable.** A failing feature turns itself off and says why; the shell keeps running.
3. **Modular.** Every feature is a module that can be switched off. A switched-off module costs
   nothing: no service started, no wakeups, no memory.
4. **Customisable without recompiling.** Layout, modules and theme come from configuration; custom
   widgets and behaviour come from a Lua layer (phase 2).
5. **One design.** Every pixel comes from the component kit and the design tokens
   (`docs/DESIGN.md`).

## Layers

```
┌─────────────────────────────────────────────────────────────────┐
│ User layer        ~/.config/kestrel/   config.toml, init.lua    │  phase 2: Lua
├─────────────────────────────────────────────────────────────────┤
│ Modules           kestrel (bin)        src/modules/*            │  features
├─────────────────────────────────────────────────────────────────┤
│ Shell             kestrel (bin)        src/shell/*              │  surfaces, bar, panels host
├─────────────────────────────────────────────────────────────────┤
│ Component kit     crates/kestrel-ui    widgets, theme, motion   │  depends on iced
├─────────────────────────────────────────────────────────────────┤
│ Services          crates/kestrel-services                       │  no UI, no iced
└─────────────────────────────────────────────────────────────────┘
```

Dependencies only point down. `kestrel-services` never imports iced or anything visual;
`kestrel-ui` never imports a service.

### Services (`crates/kestrel-services`)

System integration: Hyprland (sockets), audio (PulseAudio protocol), NetworkManager, BlueZ, UPower
and power profiles, logind, MPRIS, notifications server, StatusNotifier tray, desktop entries and
icons, weather and scores, updates, system load.

Every service has the same shape:

```rust
pub fn stream(/* config */) -> impl Stream<Item = State>   // full state snapshots, deduplicated
pub async fn command(cmd: Command)                          // actions (if the service has any)
```

- Event-driven: re-read state after a (debounced) change signal. A service that has no change
  signal says so in its doc comment and states its polling interval.
- No helper processes for data. The exceptions are documented where they occur (cliphist,
  checkupdates, hyprsunset), and they run only on demand.
- Each service is behind a cargo feature of the same name, so a build can leave it out.
- Parsing is pure and unit-tested.

### Component kit (`crates/kestrel-ui`)

Design tokens (`theme`: OKLCH colour roles from the wallpaper, type, spacing, shape), motion
(springs), and the widgets every screen is built from: `Control` (the one interactive element),
`Slider`, `Reveal`, surface shapes, icons, rows, tiles, cards. Controls clip their content; nothing
can overflow its box.

### Shell (`src/shell`)

Per-monitor surfaces (wallpaper, frame strips, space reservations, the bar surface) and the
panel host: one panel at a time grows out of the bar as one shape. The shell knows nothing about
what a panel contains.

### Modules (`src/modules`)

A module is one feature. It owns:

| Part | What |
|---|---|
| `id` | the name used in config (`"workspaces"`, `"media"`, `"weather"`, …) |
| config | its `[modules.<id>]` table, with defaults |
| state + messages | its own `State`, `Message`, `update` |
| subscription | the services it listens to; only built while the module is enabled |
| views | any of: bar item, island item, panel |

The app holds one registry of modules. The bar layout (`[bar] left/center/right`) and the island
(`[island] items`) are lists of module ids; a module not listed and not needed by another module is
not enabled, and its subscription is never created.

### Stability

- Module `update` and `view` run inside a guard: a panic disables that module, logs it, and shows
  a notification; the rest of the shell continues.
- No `unwrap()`/`expect()` on anything that can fail at runtime. Errors are values; services log
  and retry with backoff.
- The process runs under systemd (`Restart=on-failure`).

### Performance rules

- Animation ticks follow the compositor's frame callbacks; nothing animates while idle.
- Static surfaces (wallpaper, frame) skip redraws (`redraw_key`); surfaces are never resized per
  frame.
- Image handles are created once and stored, never in `view`.
- Large decodes (wallpaper) happen once, at display size, off the UI thread.

## Configuration

`~/.config/kestrel/config.toml` (layout, modules, theme): every key optional, unknown keys
ignored. `~/.config/kestrel/init.lua` (`src/lua.rs`) sets the bar layout on top of it and defines
widgets: named bar items whose `update` returns text and an icon, with an optional `click`;
handlers for shell events (`kestrel.on`: workspace, window, notification, media, device,
power, battery), sent only when the thing really changes; and moments in the island
(`kestrel.island{}`).

In `config.toml`, `[bar] groups = [["privacy", "status", "power"]]` decides which neighbouring
modules share a pill, and `[modules.<id>]` adds mouse actions to a module (`right`, `middle`,
`scroll_up`, `scroll_down`: a command, or `kestrel:<command>` for kestrel's own); they fill
buttons the module does not use. A mistake in the file keeps the running settings and is
reported once with its line.

Plugins are folders in `~/.config/kestrel/plugins/<id>/`: a `plugin.toml` (name, description,
`main` script, default `[settings]`) and its Lua script, which gets the same `kestrel` API with
its own globals and its settings as `config` (defaults overridden by `[plugins.<id>]` in
config.toml; `enabled = false` turns one off). They load after init.lua; a failing plugin is
reported by name and the rest still load; editing a plugin or its settings reloads them.

App theming goes further with templates: every `~/.config/kestrel/templates/*.toml`
(`input`, `output`, optional `hook`) is filled in (`{{accent}}`, `{{ink.rgb}}`, `{{wallpaper}}`, …)
when the colours change, and its hook runs when the output changed.

Both files are watched (the directory, since editors replace files) and apply on save:
config changes rebuild only what they touch (surfaces when the frame changes, the wallpaper when
its path changes). The script runs on its own thread with its own Lua state; every call has a
1 s instruction budget, `kestrel.read` commands a 2 s limit, and errors come back as a
notification (once per failing widget). A panic on that thread ends the Lua layer only.

## Quality gate

`make check` = `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test`.
Nothing is installed that fails it.

## Decisions

- 2026-11-12: Rust + iced 0.14 on MalpenZibo's `iced_layershell` (vendored with three documented
  patches), tiny-skia CPU renderer.
- 2026-11-12: one process with per-module crash guards (not one process per applet).
- 2026-11-12: customisation = config + Lua layer (not WASM plugins, not QML).
- 2026-11-13: the bar surface is only the bar's strip (+ room for the frame's corners); panels,
  toasts and pop-ups draw on a separate full-output overlay surface (Noctalia: every panel its
  own surface). Both are created once and never resized. The bar's motion (visualiser,
  workspaces) repaints a strip, and the idle overlay repaints nothing. See docs/RESEARCH.md.
- 2026-11-13: the shell asks the compositor for things as typed actions
  (`services::compositor::Action`: focus workspace / window, cycle, exec); the Hyprland backend
  turns them into its Lua dispatchers. Another compositor is one new backend there.
