# The bar

How kestrel's bar is built, and the rules every module follows. Researched against macOS's
menu bar (each status item its own button and menu; Settings › Control Centre: show / show
when active / don't show; reorder), COSMIC's panel (start / centre / end segments of separate
applets, added, removed and reordered in Settings) and KDE's system tray. Windows and GNOME
merge network, sound and battery into one button; kestrel does not: one function, one button.

## Principles

1. **One function, one button.** Network, Bluetooth, sound, battery and notifications are
   separate modules. Each opens its own page of the control centre (or its own panel), so a
   click goes straight to what was clicked.
2. **Every module is the same kind of thing.** A module is a file in `src/modules/` with one
   entry in the registry (`modules::MODULES`): its id, its name in Settings, its icon, its
   default section, its options, and its view. Adding a module is adding a file and a line.
3. **Settings arranges everything.** Settings › Bar edits the layout: modules in Left /
   Centre / Right, in order, moved between sections, added and removed; each module's options
   under it. Nothing needs the config file by hand (it stays the source of truth: `[bar]`).
4. **One design language.** Every module is drawn from the same pieces (`ui::control` with
   `bar()` state colours, `size::CONTROL` height, `size::ICON` icons, `text::SMALL` labels),
   so heights, padding, hover and pressed states match everywhere.
5. **Nothing costs anything while hidden.** A module only asks for the services it needs
   (`modules::needs`); a module not in the layout runs nothing.

## Modules

| id | Shows | Click | Other input |
|---|---|---|---|
| workspaces | workspaces, open apps' icons | switch workspace | scroll: next/previous |
| app | focused app's name | — | — |
| island | time, date, music visualiser, OSD | dashboard | — |
| clock | time (without the island) | calendar | — |
| media | music (pill: a bubble) | media panel | — |
| weather | temperature, icon | weather panel | — |
| airpods | AirPods battery ring | AirPods panel | — |
| tray | tray icons | per icon | per icon |
| system | CPU and memory | system monitor | — |
| updates | pending updates (when any) | Settings › Updates | — |
| privacy | one capsule: screen / camera / microphone in use; click: who uses them, and recently | — | — |
| network | Wi-Fi strength, Ethernet, offline | control centre › Wi-Fi | — |
| bluetooth | on / connected (when on) | control centre › Bluetooth | — |
| volume | output level | control centre › Sound | scroll: volume; middle: mute |
| battery | charge (optional %) | control centre › Power | — |
| notifications | bell: unseen count, Do not disturb | notification centre | — |
| power | power button | power menu | — |

`status` (the old combined button) is read as `network, bluetooth, volume, battery`, so an
older config keeps working and shows the separate buttons.

## Pills

In the pill style, neighbouring modules of one group share a pill (`[bar] groups`); each is
still its own button inside it, with its own hover. The default groups the status buttons
(network, bluetooth, volume, battery, notifications) the way macOS keeps its status items
together.
