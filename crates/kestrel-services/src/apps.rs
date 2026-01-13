//! Installed applications from `.desktop` files, and launching them through Hyprland.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub generic: String,
    pub comment: String,
    pub icon: String,
    pub exec: String,
    pub keywords: String,
    pub terminal: bool,
    /// The window class the app's windows carry, when the entry says so.
    pub wm_class: String,
}

fn dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(d) = data_home {
        v.push(d.join("applications"));
    }
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        v.push(Path::new(d).join("applications"));
    }
    v
}

fn parse(path: &Path, id: String) -> Option<App> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut f: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            f.entry(k.trim()).or_insert(v.trim());
        }
    }
    if f.get("Type") != Some(&"Application")
        || f.get("NoDisplay") == Some(&"true")
        || f.get("Hidden") == Some(&"true")
    {
        return None;
    }
    if let Some(only) = f.get("OnlyShowIn")
        && !only.split(';').any(|d| d == "Hyprland")
    {
        return None;
    }
    if let Some(not) = f.get("NotShowIn")
        && not.split(';').any(|d| d == "Hyprland")
    {
        return None;
    }
    Some(App {
        id,
        name: f.get("Name")?.to_string(),
        generic: f.get("GenericName").unwrap_or(&"").to_string(),
        comment: f.get("Comment").unwrap_or(&"").to_string(),
        icon: f.get("Icon").unwrap_or(&"").to_string(),
        exec: f.get("Exec")?.to_string(),
        keywords: f.get("Keywords").unwrap_or(&"").to_string(),
        terminal: f.get("Terminal") == Some(&"true"),
        wm_class: f.get("StartupWMClass").unwrap_or(&"").to_string(),
    })
}

/// All visible applications; earlier directories (the user's) win over system ones.
pub fn load() -> Vec<App> {
    let mut seen = HashMap::new();
    for dir in dirs() {
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "desktop") {
                    continue;
                }
                let id = p
                    .strip_prefix(&dir)
                    .map(|r| r.to_string_lossy().replace('/', "-"))
                    .unwrap_or_default();
                if seen.contains_key(&id) {
                    continue;
                }
                seen.insert(id.clone(), parse(&p, id));
            }
        }
    }
    let mut apps: Vec<App> = seen.into_values().flatten().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// (name, icon) of the desktop entry a window class belongs to: matched against each entry's
/// StartupWMClass, then its desktop file name (also its last dotted part).
fn entry_for_class(class: &str) -> Option<(String, String)> {
    use std::sync::OnceLock;
    static ENTRIES: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    let entries = ENTRIES.get_or_init(|| {
        let mut m = HashMap::new();
        for a in load() {
            let v = (a.name.clone(), a.icon.clone());
            let id = a.id.trim_end_matches(".desktop").to_lowercase();
            // org.kde.dolphin -> dolphin, too.
            if let Some(last) = id.rsplit('.').next() {
                m.entry(last.to_string()).or_insert_with(|| v.clone());
            }
            m.insert(id, v.clone());
            if !a.wm_class.is_empty() {
                m.insert(a.wm_class.to_lowercase(), v);
            }
        }
        m
    });
    entries.get(&class.to_lowercase()).cloned()
}

/// Icon name for a window class (the class itself when no entry matches).
pub fn icon_for_class(class: &str) -> String {
    entry_for_class(class).map_or_else(|| class.to_lowercase(), |(_, icon)| icon)
}

/// The human name of the app a window belongs to. Falls back to the class itself.
pub fn name_for_class(class: &str) -> String {
    if class.is_empty() {
        return String::new();
    }
    if let Some((n, _)) = entry_for_class(class) {
        return n;
    }
    // Fallback: "org.gnome.Nautilus" -> "Nautilus", "kitty" -> "Kitty".
    let last = class.rsplit('.').next().unwrap_or(class);
    let mut c = last.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The Exec line without field codes.
pub fn command_line(app: &App) -> String {
    let mut out = String::new();
    let mut chars = app.exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some('%') = chars.next() {
                out.push('%')
            }
        } else {
            out.push(c);
        }
    }
    let cmd = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if app.terminal {
        format!("kitty -e {cmd}")
    } else {
        cmd
    }
}

/// Launch through Hyprland so the app is a child of the compositor, not of kestrel.
pub fn launch_lua(cmd: &str) -> String {
    let escaped = cmd.replace('\\', "\\\\").replace('"', "\\\"");
    format!("hl.dsp.exec_cmd(\"{escaped}\")")
}

/// Fuzzy score of `query` against `text` (higher is better); None if not all characters match
/// in order.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let tl = text.to_lowercase();
    let ql = query.to_lowercase();
    if tl.starts_with(&ql) {
        return Some(1000 - t.len() as i32);
    }
    if let Some(i) = tl.find(&ql) {
        // Word starts beat middles.
        let word = i == 0 || !t[tl[..i].chars().count() - 1].is_alphanumeric();
        return Some(if word { 800 } else { 600 } - i as i32);
    }
    let mut score = 0;
    let mut ti = 0;
    let mut last = None;
    for qc in q {
        let mut found = false;
        while ti < t.len() {
            if t[ti] == qc {
                score += if last == Some(ti.wrapping_sub(1)) {
                    8
                } else {
                    2
                };
                if ti == 0 || !t[ti - 1].is_alphanumeric() {
                    score += 6;
                }
                last = Some(ti);
                ti += 1;
                found = true;
                break;
            }
            ti += 1;
        }
        if !found {
            return None;
        }
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_beats_fuzzy() {
        assert!(score("fir", "Firefox") > score("fir", "Disk Information Reader"));
        assert!(score("ch", "Chromium") > score("ch", "Rich Text"));
        assert!(score("xyz", "Firefox").is_none());
        assert!(score("vsc", "Visual Studio Code").is_some());
    }
}
