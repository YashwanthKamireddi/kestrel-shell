//! What the shell remembers between runs, in one file: `$XDG_STATE_HOME/kestrel/state.toml`,
//! one table per owner (`[tray]`, `[emoji]`, `[files]`, `[launcher]`, `[toggles]`). Never the user's
//! settings: those are config.toml. Written whole and atomically (a temporary file renamed
//! over it), so a crash mid-write never leaves it half written. The loose files kestrel used
//! before are read once and removed.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub tray: Tray,
    pub emoji: Emoji,
    pub files: Files,
    pub launcher: Launcher,
    pub toggles: Toggles,
}

/// Control-centre switches kestrel itself holds, so a restarted shell puts them back as they
/// were (Wi-Fi, Bluetooth, power mode and VPNs live in the system and keep themselves).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Toggles {
    pub dnd: bool,
    pub caffeine: bool,
    pub night_light: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Tray {
    /// Apps kept in the bar (their ids).
    pub pinned: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Emoji {
    /// Most recent first.
    pub recent: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Files {
    /// Where the file picker was last used.
    pub last_folder: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Launcher {
    /// How often each app was launched (desktop id → count), for ordering.
    pub launches: BTreeMap<String, u32>,
}

fn path() -> PathBuf {
    crate::config::state_dir().join("state.toml")
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(load()));

fn load() -> State {
    match std::fs::read_to_string(path()) {
        Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
            log::warn!("state.toml: {e}; starting fresh");
            State::default()
        }),
        Err(_) => {
            let s = migrate();
            save(&s);
            s
        }
    }
}

/// The loose files of earlier versions, folded in once and removed.
fn migrate() -> State {
    let dir = crate::config::state_dir();
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    let mut s = State::default();
    if let Some(t) = read("tray-pins") {
        s.tray.pinned = t
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
    }
    if let Some(t) = read("emoji-recent") {
        s.emoji.recent = t
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
    }
    if let Some(t) = read("files-last-folder") {
        let t = t.trim();
        if !t.is_empty() {
            s.files.last_folder = Some(t.to_string());
        }
    }
    if let Some(t) = read("launches") {
        s.launcher.launches = t
            .lines()
            .filter_map(|l| {
                let (n, id) = l.split_once(' ')?;
                Some((id.to_string(), n.parse().ok()?))
            })
            .collect();
    }
    for name in ["tray-pins", "emoji-recent", "files-last-folder", "launches"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    s
}

fn save(s: &State) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let Ok(body) = toml::to_string_pretty(s) else {
        return;
    };
    let tmp = p.with_extension("toml.tmp");
    if std::fs::write(&tmp, body).is_ok()
        && let Err(e) = std::fs::rename(&tmp, &p)
    {
        log::warn!("state.toml: {e}");
    }
}

/// Reads the remembered state.
pub fn get<T>(f: impl FnOnce(&State) -> T) -> T {
    let s = STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&s)
}

/// Changes it and writes it out (only when something really changed).
pub fn update(f: impl FnOnce(&mut State)) {
    let mut s = STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = s.clone();
    f(&mut s);
    if *s != before {
        save(&s);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trips_through_toml() {
        let mut s = super::State::default();
        s.tray.pinned.insert("vesktop".into());
        s.emoji.recent = vec!["😀".into()];
        s.files.last_folder = Some("/home/a/Pictures".into());
        s.launcher.launches.insert("kitty.desktop".into(), 3);
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<super::State>(&text).unwrap(), s);
    }
}
