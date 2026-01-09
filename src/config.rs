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
        }
    }
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
