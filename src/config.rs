//! `~/.config/kestrel/config.toml`. Every field is optional; unknown keys are ignored so the
//! file can grow without breaking older builds.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub wallpaper: Option<String>,
    pub frame: Frame,
    /// Workspaces always shown, even when empty.
    pub workspaces: u32,
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
            frame: Frame::default(),
            workspaces: 5,
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

pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}
