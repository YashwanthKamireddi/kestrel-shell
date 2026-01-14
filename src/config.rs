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
    let p = self::path();
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let line = format!("wallpaper = {}", toml::Value::String(path.to_string()));
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            let key = l.trim_start();
            if !found && (key.starts_with("wallpaper ") || key.starts_with("wallpaper=")) {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        // Top-level keys must come before the first [table].
        let at = out
            .iter()
            .position(|l| l.trim_start().starts_with('['))
            .unwrap_or(out.len());
        out.insert(at, line);
    }
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, out.join("\n") + "\n");
}

pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}
