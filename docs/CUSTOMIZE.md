# Customising kestrel

Everything here can be done from Settings; this is for doing it by hand, scripting it, or
extending kestrel. Files live in `~/.config/kestrel/`. Saving any of them applies at once.

## config.toml

The one source of truth for every choice made in Settings (Settings writes it; editing it by
hand is the same as using Settings). Copying it to another machine is the export.

| Table / key | What |
|---|---|
| `wallpaper`, `wallpaper_dir` | the wallpaper and the folder the picker shows |
| `material` | `"black"` or `"frosted"` |
| `accent` | `"#4c8df6"`; absent: from the wallpaper |
| `theme_apps` | write kestrel's colours into GTK, Qt, kitty, hyprlock, starship |
| `workspaces` | how many workspace slots the bar shows |
| `editor` | `$EDITOR` / `$VISUAL` for the session |
| `weather_city`, `weather_units` | `"celsius"` / `"fahrenheit"` |
| `night_light_temperature`, `night_light_schedule` | kelvin; sunset to sunrise |
| `cursor_size` | pointer size |
| `[bar]` | `style` (`"pills"` / `"full"`), `left`, `center`, `right` (module ids in order), `visualiser`, `battery_percent` |
| `[modules.<id>]` | `shape` (`"attached"`: shares its section's pill, the default; `"bubble"`: a pill of its own; `"plain"`: no pill), `right`, `middle`, `scroll_up`, `scroll_down` (a command, or `kestrel:<command>`) over the bar's gesture table |
| `[clock]` | `format` (`"24h"` / `"12h"`), `date`, `seconds`, `week_starts_sunday` |
| `[sound]` | `step`, `max` (volume keys), `alerts` (alert volume %), `feedback`, `screenshot` |
| `[notifications]` | `muted`, `quiet`, `silent` (app lists), `sound`, `banner` (seconds), `urgent_in_dnd` |
| `[look]` | `font`, `font_size`, `mono`, `mono_size`, `icons`, `cursor` |
| `[hyprland]` | any Hyprland option, nested as `hl.config` nests it (written to `hypr/conf/settings.lua`) |
| `[[window_rules]]` | `class`, `float`, `size` (`"small"`/`"medium"`/`"large"`), `center`, `workspace`, `opacity`, `pin` |
| `[[devices]]` | `name`, `sensitivity` (per pointing device) |
| `[[shortcuts]]` | `keys`, `name`, `command` |
| `[plugins.<id>]` | `enabled`, and the plugin's own settings |

Bar module ids: `workspaces app island clock media weather airpods tray system updates
privacy network bluetooth volume battery notifications power` (docs/BAR.md says what each
does). An older `status` is read as `network bluetooth volume battery` plus the bell.

## init.lua

Runs on its own thread with a time limit on every call, so a slow or broken script never
stalls the shell; errors show as a notification.

```lua
-- Bar sections set here replace config.toml's.
kestrel.bar { style = "pills", right = { "disk", "network", "volume", "battery", "power" } }

-- A widget: a name you can put in the bar layout.
kestrel.widget("disk", {
  interval = 60,                        -- seconds between updates (default 10)
  update = function()                   -- { text =, icon = } / a string / nil hides it
    return { icon = "hard-drive", text = kestrel.read("df --output=pcent / | tail -1") }
  end,
  click = "kitty -e btop",              -- a command, or a function
})

kestrel.on("device", function(e)
  if e.connected then kestrel.island{ icon = "headphones", title = e.name, detail = "Ready" } end
end)
```

- `kestrel.exec(cmd)` starts a program; `kestrel.read(cmd)` runs one and returns its output
  (2 s limit); `kestrel.notify(title, body)`; `kestrel.island{ icon, title, detail }`.
- Events for `kestrel.on`: `workspace {id}`, `window {class, title}`,
  `notification {app, summary, body}`, `media {title, artist, playing}`,
  `device {name, connected}`, `power {charging, percent}`, `battery {percent}`.
- Icons: a name from kestrel's set (`src/ui/icon.rs`), an icon theme name, or a file path.

## Plugins

`~/.config/kestrel/plugins/<id>/`, listed and switched on or off in Settings › Plugins.

```toml
# plugin.toml
name = "Disk"
description = "Free space on /"
main = "main.lua"        # default
[settings]               # defaults; [plugins.<id>] in config.toml overrides them
path = "/"
```

The script has the same `kestrel` API as init.lua, its own globals (plugins never collide),
and its settings as the global `config` (`config.path`).

## Theme templates

`~/.config/kestrel/templates/*.toml` theme any app with kestrel's colours:

```toml
input = "foot.ini"                     # beside this file, or a full path
output = "~/.config/foot/kestrel.ini"  # where the filled-in copy goes
hook = "pkill -USR1 foot"              # optional: run when the output changed
```

Placeholders (`#rrggbb`, or `r, g, b` with `.rgb`): `{{base}} {{surface}} {{surface_high}}
{{line}} {{ink}} {{ink_dim}} {{ink_faint}} {{accent}} {{on_accent}} {{accent_soft}} {{good}}
{{warn}} {{bad}} {{info}}`, plus `{{accent_hover}}`, `{{accent_tint}}` and `{{wallpaper}}`.

## Commands

`kestrel <command>` (what keybinds call; Settings › Developer copies them):
`toggle <panel>`, `close`, `volume up|down|mute|+N|-N`, `mic mute`, `brightness +N|-N`,
`osd caps|num`, `game-mode`, `open-default <kind> [path]`. Panels: `launcher overview
control calendar power clipboard emoji wallpaper keys capture monitor weather files media privacy
settings` (also `wifi`, `bluetooth`, `sound` for the control centre).
