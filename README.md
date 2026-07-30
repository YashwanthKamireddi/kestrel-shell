# kestrel

A desktop shell for Hyprland written in Rust: bar, dynamic island, control centre, launcher,
notifications, clipboard, settings and the system agents a desktop needs, in one native
program. No QML, GTK, Electron or JavaScript runtime.

kestrel is used every day on the machine it was built on. It is young, opinionated and
currently made for one setup (Hyprland 0.56+ with its Lua config, Arch Linux); see
[Status](#status) before installing.

## Why another shell

Most popular Hyprland shells (illogical-impulse, Caelestia, DankMaterialShell, HyprPanel) are
built on QML or a JavaScript runtime. They look great and move fast, but every surface pays
for a scene graph and an interpreter, and the pieces that make a desktop work (notifications,
password prompts, Wi-Fi secrets, Bluetooth pairing, the file picker) usually come from other
programs with other looks.

kestrel tries the other way round:

- **One native process.** Rust with [iced](https://github.com/iced-rs/iced) on layer-shell,
  drawn on the CPU with tiny-skia. Surfaces redraw only the pixels that changed; a static
  surface is never redrawn.
- **The system agents are part of the shell.** kestrel is the notification daemon, the polkit
  authentication agent, NetworkManager's secret agent, BlueZ's pairing agent and the
  xdg-desktop-portal file chooser, so every prompt looks and behaves like the rest of it.
- **One design system.** Every panel is built from the same small kit (spacing, corners,
  rows, sliders, controls); a lint in `make check` fails the build when a panel uses a literal
  size instead of a token.
- **Settings instead of config files.** A settings app covers the bar, appearance, displays,
  input, sound, network, Bluetooth, power, notifications, shortcuts and window rules, and
  writes Hyprland's Lua config for you. Everything is also plain TOML for those who prefer it.

## What it does

- **Bar** with floating pills or a full band: workspaces with app icons, the island, music,
  weather, AirPods battery, tray, privacy indicator, network, Bluetooth, sound, battery, power.
  Modules are configured per side and can be written in Lua.
- **Dynamic island** at the centre: the time, music with a live visualiser, volume and
  brightness, devices connecting, recordings, copies. A notification grows the island into
  its card and shrinks it back.
- **Notifications** with stacked cards per app, actions, swipe to dismiss, per-app rules
  (quiet, muted, silent) and Do not disturb.
- **Control centre**: Wi-Fi and Bluetooth with their own pages, airplane mode, night light,
  caffeine, power profiles, game mode, VPN toggles, sound mixer per app and per device.
- **Launcher**: apps, a calculator, clipboard history, emoji, file search, settings pages,
  running processes; paste a path and press Enter to open it.
- **Clipboard history, emoji picker, screenshots and recording, wallpaper picker with
  colours from the wallpaper, system monitor, weather, calendar, window overview, keyboard
  shortcut sheet.**
- **Theming of other apps**: the accent and fonts are written into GTK, Qt, kitty, hyprlock
  and starship.

## Performance

Measured on the development laptop (Intel Core i5-1245U, 16 GB, 1920×1080, Arch Linux,
Hyprland 0.56), release build, in November 2026:

| What | Measured | How |
|---|---|---|
| CPU at idle (bar, island, nothing animating) | 0.3 % of one core | `/proc/<pid>/stat` utime+stime over 20 s |
| CPU while music plays with the visualiser | 3.6 % of one core | same, live session |
| Resident memory in normal use | 85–105 MB | `VmRSS` |
| Panel opened to first frame on screen | 7–9 ms | `KESTREL_TRACE=1` timestamps, 8 panels |

These are one machine's numbers, not a benchmark against other shells. `KESTREL_TRACE=1`
prints every frame's build, draw and raster time if you want to measure your own.

## Status

- Hyprland 0.56 or newer with the **Lua** config (`~/.config/hypr/*.lua`). Other compositors
  are not supported yet.
- Packaged for Arch Linux (`packaging/arch/PKGBUILD`); other distributions need a manual
  build.
- English only. Expect breaking changes to the config.

## Install (Arch Linux)

```sh
git clone https://github.com/<you>/kestrel
cd kestrel
make install        # checks, release build, makepkg, pacman -U, restarts kestrel
systemctl --user enable --now kestrel
```

Requires `hyprland`, `pipewire`, `libpulse`, `dbus`, `ttf-rubik-vf` and `ttf-twemoji`.
Optional: `cliphist` and `wl-clipboard` (clipboard history), `hyprsunset` (night light),
`hyprlock` (lock screen).

## Configure

Open Settings (the gear in the control centre, or `kestrel open settings`), or edit
`~/.config/kestrel/config.toml`; saving applies at once. Lua scripts in
`~/.config/kestrel/init.lua` can add bar widgets and react to events, each call under a time
limit so a broken script cannot stall the shell. See [docs/CUSTOMIZE.md](docs/CUSTOMIZE.md)
and [docs/BAR.md](docs/BAR.md).

## Develop

```sh
make check          # rustfmt, the design-kit lint, clippy -D warnings, tests
tools/rig/rig.sh start && tools/rig/rig.sh run    # a nested Hyprland to try changes safely
```

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the code is organised and
[docs/DESIGN.md](docs/DESIGN.md) the design kit. `vendor/README.md` lists every change made to
the vendored iced layer-shell, tiny-skia and softbuffer, and why.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
