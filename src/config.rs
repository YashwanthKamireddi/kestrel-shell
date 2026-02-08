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
    /// City for the weather (looked up with Open-Meteo). No city, no weather.
    pub weather_city: Option<String>,
    /// Exact place for the weather (a neighbourhood the city search can't find); the city
    /// name is then only the label.
    pub weather_latitude: Option<f64>,
    pub weather_longitude: Option<f64>,
    /// What the bar shows, where, and in which style.
    pub bar: BarConfig,
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
}

impl Default for BarConfig {
    fn default() -> Self {
        let v = |ids: &[&str]| ids.iter().map(|s| (*s).to_string()).collect();
        Self {
            style: BarStyle::Full,
            left: v(&["workspaces", "app"]),
            center: v(&["island"]),
            right: v(&[
                "tray",
                "privacy",
                "updates",
                "status",
                "notifications",
                "power",
            ]),
        }
    }
}

impl BarConfig {
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
            weather_city: None,
            weather_latitude: None,
            weather_longitude: None,
            bar: BarConfig::default(),
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
    let p = path();
    match std::fs::read_to_string(&p) {
        Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
            log::warn!("{}: {e}", p.display());
            Config::default()
        }),
        Err(_) => Config::default(),
    }
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
