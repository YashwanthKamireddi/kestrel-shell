//! `~/.config/kestrel/config.toml`. Every field is optional; unknown keys are ignored so the
//! file can grow without breaking older builds.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub wallpaper: Option<String>,
    /// Folder the wallpaper picker shows (default: the current wallpaper's folder).
    pub wallpaper_dir: Option<String>,
    pub frame: Frame,
    /// Workspaces always shown, even when empty.
    pub workspaces: u32,
    /// Theme GTK, Qt, kitty, Hyprland borders and hyprlock with the shell's colours.
    pub theme_apps: bool,
    /// What the bar's pills and the panels are made of: "black" (solid) or "frosted" (a heavy
    /// blur of the wallpaper behind them, darkened so text stays easy to read).
    pub material: Material,
    /// City for the weather (looked up with Open-Meteo). No city, no weather.
    pub weather_city: Option<String>,
    /// Exact place for the weather (a neighbourhood the city search can't find); the city
    /// name is then only the label.
    pub weather_latitude: Option<f64>,
    pub weather_longitude: Option<f64>,
    /// Night light's warmth in kelvin (lower is warmer).
    pub night_light_temperature: u32,
    /// Night light on by itself from sunset to sunrise (the weather's sun times).
    pub night_light_schedule: bool,
    /// The pointer's size everywhere (Settings > Accessibility); unset: the session's own.
    pub cursor_size: Option<u32>,
    /// Do not disturb by itself between these times ("22:00" to "07:00"); off when unset.
    pub quiet_from: Option<String>,
    pub quiet_to: Option<String>,
    /// What the bar shows, where, and in which style.
    pub bar: BarConfig,
    /// How the time reads: `[clock] format = "24h" | "12h"`, `date = true`.
    pub clock: ClockConfig,
    /// Temperatures in "celsius" or "fahrenheit".
    pub weather_units: Units,
    /// Per-module extras: `[modules.clock] middle = "kitty -e khal interactive"`.
    pub modules: std::collections::BTreeMap<String, ModuleConfig>,
    /// Per-app notification rules: `[notifications] muted = ["Discord"]` (dropped),
    /// `quiet = ["Steam"]` (kept in the list, no banner).
    pub notifications: NotificationRules,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct NotificationRules {
    pub muted: Vec<String>,
    pub quiet: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum ClockFormat {
    #[default]
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "12h")]
    H12,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClockConfig {
    pub format: ClockFormat,
    /// The date beside the time in the bar.
    pub date: bool,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: ClockFormat::H24,
            date: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    #[default]
    Black,
    /// "glass" was a second, clearer look for a while; it reads as frosted now.
    #[serde(alias = "glass")]
    Frosted,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Celsius,
    Fahrenheit,
}

/// Mouse actions added to a module: a command to run, or `kestrel:<command>` for kestrel's own
/// (`kestrel:toggle control`, `kestrel:volume +5%`). They fill buttons the module itself does
/// not use; its own click stays.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ModuleConfig {
    pub right: Option<String>,
    pub middle: Option<String>,
    pub scroll_up: Option<String>,
    pub scroll_down: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BarStyle {
    /// One continuous bar joined to the screen frame.
    #[default]
    Full,
    /// Separate floating pills, one per section; no frame.
    Pills,
}

/// `[bar]`: module ids per section, in order. A module not listed is off.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct BarConfig {
    pub style: BarStyle,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    /// Pill style: modules that share one pill when they sit next to each other, e.g.
    /// `groups = [["privacy", "status", "power"]]`. Everything else stands alone.
    pub groups: Vec<Vec<String>>,
    /// The music visualiser in the time pill.
    pub visualiser: bool,
    /// The battery's percentage beside its icon.
    pub battery_percent: bool,
}

impl Default for BarConfig {
    fn default() -> Self {
        let v = |ids: &[&str]| ids.iter().map(|s| (*s).to_string()).collect();
        Self {
            style: BarStyle::Full,
            left: v(&["workspaces", "app"]),
            center: v(&["island", "airpods"]),
            right: v(&["privacy", "updates", "status", "notifications", "power"]),
            groups: vec![v(&["tray", "privacy", "status", "notifications", "power"])],
            visualiser: true,
            battery_percent: true,
        }
    }
}

impl BarConfig {
    /// Which group `id` belongs to, if any.
    pub fn group_of(&self, id: &str) -> Option<usize> {
        self.groups.iter().position(|g| g.iter().any(|m| m == id))
    }

    pub fn contains(&self, id: &str) -> bool {
        self.left
            .iter()
            .chain(&self.center)
            .chain(&self.right)
            .any(|m| m == id)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Frame {
    pub thickness: f32,
    pub radius: f32,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            thickness: 4.0,
            radius: 20.0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wallpaper: None,
            wallpaper_dir: None,
            frame: Frame::default(),
            workspaces: 5,
            theme_apps: true,
            material: Material::Black,
            weather_city: None,
            weather_latitude: None,
            weather_longitude: None,
            night_light_temperature: 4500,
            night_light_schedule: false,
            cursor_size: None,
            quiet_from: None,
            quiet_to: None,
            bar: BarConfig::default(),
            modules: std::collections::BTreeMap::new(),
            clock: ClockConfig::default(),
            weather_units: Units::Celsius,
            notifications: NotificationRules::default(),
        }
    }
}

/// kestrel's state directory (`$XDG_STATE_HOME/kestrel`): things the shell remembers.
pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_default()
        .join("kestrel")
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default();
    base.join("kestrel").join("config.toml")
}

pub fn load() -> Config {
    read().unwrap_or_else(|e| {
        log::warn!("{e}");
        Config::default()
    })
}

/// The config, or what is wrong with it: "config.toml, line 12: unknown variant `pils`,
/// expected `full` or `pills`". A missing file is the defaults, not an error.
pub fn read() -> Result<Config, String> {
    let p = path();
    let Ok(s) = std::fs::read_to_string(&p) else {
        return Ok(Config::default());
    };
    toml::from_str(&s).map_err(|e| {
        let line = e
            .span()
            .map(|r| s[..r.start.min(s.len())].lines().count().max(1));
        let what = e.message().trim().to_string();
        match line {
            Some(l) => format!("config.toml, line {l}: {what}"),
            None => format!("config.toml: {what}"),
        }
    })
}

/// The `[plugins]` part of config.toml, as text (to notice when it changes).
pub fn plugin_settings() -> String {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("plugins").map(ToString::to_string))
        .unwrap_or_default()
}

/// Saves the wallpaper choice, keeping the rest of the file (and its comments) as it is.
pub fn set_wallpaper(path: &str) {
    set(&[], "wallpaper", toml_edit::value(path));
}

/// Change one setting in config.toml in place (comments and the rest kept as they are).
/// `table` is the path of tables (`&["bar"]`, or `&[]` for a top-level key). The file is
/// watched, so the change applies at once.
pub fn set(table: &[&str], key: &str, value: toml_edit::Item) {
    let p = self::path();
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        log::warn!("config: {} does not parse; not changing it", p.display());
        return;
    };
    let mut t = doc.as_table_mut();
    for name in table {
        if !t.contains_key(name) {
            t.insert(name, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let Some(next) = t.get_mut(name).and_then(toml_edit::Item::as_table_mut) else {
            log::warn!("config: [{name}] is not a table");
            return;
        };
        t = next;
    }
    t.insert(key, value);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, doc.to_string());
}

/// A list of strings as a config value.
pub fn list(items: &[String]) -> toml_edit::Item {
    let mut a = toml_edit::Array::new();
    for i in items {
        a.push(i.as_str());
    }
    toml_edit::value(a)
}

pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}
