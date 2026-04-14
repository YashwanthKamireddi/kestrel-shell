//! Startup apps: the freedesktop autostart entries (~/.config/autostart and
//! /etc/xdg/autostart), run at login by systemd's xdg-autostart generator. Turning a system
//! entry off writes a user copy with `Hidden=true` (the standard override); apps added here are
//! copies of their desktop file in ~/.config/autostart.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// File name ("org.kde.kdeconnect.daemon.desktop").
    pub id: String,
    pub name: String,
    pub icon: String,
    pub enabled: bool,
    /// Added by the user (removable); else a system entry (only switched off).
    pub user_only: bool,
}

fn user_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/autostart"))
}

const SYSTEM: &str = "/etc/xdg/autostart";

fn fields(path: &Path) -> std::collections::HashMap<String, String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut on = false;
    let mut f = std::collections::HashMap::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            on = l == "[Desktop Entry]";
            continue;
        }
        if on && let Some((k, v)) = l.split_once('=') {
            f.entry(k.trim().to_string())
                .or_insert_with(|| v.trim().to_string());
        }
    }
    f
}

/// Shown here only when it would run in this session (no OnlyShowIn for another desktop).
fn for_here(f: &std::collections::HashMap<String, String>) -> bool {
    let desk = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Hyprland".into());
    let here = |list: &str| {
        list.split(';')
            .any(|d| !d.is_empty() && desk.split(':').any(|x| x == d))
    };
    if let Some(only) = f.get("OnlyShowIn")
        && !here(only)
    {
        return false;
    }
    !f.get("NotShowIn").is_some_and(|n| here(n))
}

pub fn list() -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let user = user_dir();
    let mut ids = std::collections::BTreeSet::new();
    for dir in user.iter().map(PathBuf::as_path).chain([Path::new(SYSTEM)]) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.ends_with(".desktop") {
                    ids.insert(n);
                }
            }
        }
    }
    for id in ids {
        let up = user.as_ref().map(|d| d.join(&id)).filter(|p| p.exists());
        let sp = Path::new(SYSTEM).join(&id);
        let sys = sp.exists().then_some(sp);
        let main = sys.as_ref().or(up.as_ref()).cloned();
        let Some(main) = main else { continue };
        let base = fields(&main);
        if !for_here(&base) {
            continue;
        }
        let over = up.as_ref().map(|p| fields(p));
        let hidden = over
            .as_ref()
            .unwrap_or(&base)
            .get("Hidden")
            .is_some_and(|v| v == "true");
        // kestrel's own override for the tray watcher stays out of sight.
        if id == "xapp-sn-watcher.desktop" {
            continue;
        }
        out.push(Entry {
            name: over
                .as_ref()
                .and_then(|o| o.get("Name").cloned())
                .or_else(|| base.get("Name").cloned())
                .unwrap_or_else(|| id.trim_end_matches(".desktop").to_string()),
            icon: base.get("Icon").cloned().unwrap_or_default(),
            enabled: !hidden,
            user_only: sys.is_none(),
            id,
        });
    }
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// Turns an entry on or off.
pub fn set_enabled(id: &str, on: bool) -> Result<(), String> {
    let dir = user_dir().ok_or("no home folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let up = dir.join(id);
    let sys = Path::new(SYSTEM).join(id);
    let src = if up.exists() { up.clone() } else { sys.clone() };
    let text = std::fs::read_to_string(&src).map_err(|e| e.to_string())?;
    // A system entry turned back on: its override goes (back to the system's own).
    if on && sys.exists() && up.exists() {
        let overridden = fields(&up);
        if overridden.get("Hidden").is_some_and(|v| v == "true") {
            return std::fs::remove_file(&up).map_err(|e| e.to_string());
        }
    }
    let mut out = String::new();
    let mut in_entry = false;
    let mut done = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            if in_entry && !done {
                out += &format!("Hidden={on_off}\n", on_off = !on);
                done = true;
            }
            in_entry = l == "[Desktop Entry]";
        }
        if in_entry && l.starts_with("Hidden=") {
            if !done {
                out += &format!("Hidden={}\n", !on);
                done = true;
            }
            continue;
        }
        out += line;
        out.push('\n');
    }
    if !done {
        out += &format!("Hidden={}\n", !on);
    }
    std::fs::write(&up, out).map_err(|e| e.to_string())
}

/// Adds an installed app (its desktop file) to start at login.
pub fn add(desktop_file: &Path) -> Result<(), String> {
    let dir = user_dir().ok_or("no home folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = desktop_file.file_name().ok_or("no file name")?;
    std::fs::copy(desktop_file, dir.join(name))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Removes an app added by the user.
pub fn remove(id: &str) -> Result<(), String> {
    let p = user_dir().ok_or("no home folder")?.join(id);
    if Path::new(SYSTEM).join(id).exists() {
        return Err("A system entry can only be turned off".into());
    }
    std::fs::remove_file(p).map_err(|e| e.to_string())
}
