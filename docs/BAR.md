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

Every item answers one gesture table (`modules::gestures`, docs/SYSTEM.md): **left** opens
its place, **right** its quick action, **scroll** adjusts, **middle** opens Settings › Bar.
`[modules.<id>]` overrides any of them.

| id | Shows | Left | Right | Scroll |
|---|---|---|---|---|
| workspaces | workspaces, open apps' icons | switch workspace | — | next/previous |
| app | focused app's name | — | — | — |
| island | what is happening now: the time and date, music (its song opens the player), the weather, the visualiser; a volume/brightness slider; an incoming notification (a click drops its card); something copied | the day (weather, forecast, month) | — | — |
| clock | time (without the island) | calendar | — | — |
| media | music (beside the island: inside it) | music player | — | — |
| weather | temperature, icon (beside the island: inside it) | the day | — | — |
| airpods | AirPods battery ring | control centre › Bluetooth (each bud's charge) | — | — |
| tray | tray icons | per icon | per icon | — |
| system | CPU and memory | system monitor | — | — |
| updates | pending updates (when any) | Settings › Updates | — | — |
| privacy | one capsule: screen / camera / microphone in use | who uses them, and recently | — | — |
| network | Wi-Fi strength, Ethernet, offline | control centre › Wi-Fi | Wi-Fi on/off | — |
| bluetooth | on / connected (when on) | control centre › Bluetooth | Bluetooth on/off | — |
| volume | output level | control centre › Sound | mute | volume |
| battery | charge (optional %) | control centre › Power | next power mode | — |
| notifications | bell: unseen count, Do not disturb | control centre | Do not disturb | — |
| power | power button | power menu | — | — |

`status` (the old combined button) is read as `network, bluetooth, volume, battery`, so an
older config keeps working and shows the separate buttons.

## Pills

In the pill style each section is one pill (Noctalia's rule: its height from the bar, never
from its content). A module's `shape` changes that: `attached` (default) shares the
section's pill, `bubble` is a pill of its own, `plain` has none (Settings › Bar, or
`[modules.<id>] shape`). Music and weather beside the island live inside it, unless given
a shape of their own.
