//! The freedesktop places and lists, read and written directly (no `xdg-mime` or
//! `xdg-user-dir` processes): the base directories, the user folders (Pictures…) and the
//! default app for each type (mimeapps.list).
//!
//! The default app is chosen by the mime-apps spec as GIO (GTK, Thunar) reads it: the chosen
//! default for the type, else the user's added associations, else what apps list in the caches
//! (xdg-utils' `xdg-mime` skips the added associations; otherwise the same steps). The write
//! follows xdg-mime's make_default_generic, so the file stays as every tool expects.

use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn var_dir(name: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map_or_else(default, PathBuf::from)
}

fn var_list(name: &str, default: &str) -> Vec<PathBuf> {
    let v = std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into());
    v.split(':')
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn config_home() -> PathBuf {
    var_dir("XDG_CONFIG_HOME", || home().join(".config"))
}

pub fn config_dirs() -> Vec<PathBuf> {
    var_list("XDG_CONFIG_DIRS", "/etc/xdg")
}

pub fn data_home() -> PathBuf {
    var_dir("XDG_DATA_HOME", || home().join(".local/share"))
}

pub fn data_dirs() -> Vec<PathBuf> {
    var_list("XDG_DATA_DIRS", "/usr/local/share:/usr/share")
}

/// The user's data directory, then the system ones (where apps, icons and sounds live).
pub fn data_all() -> Vec<PathBuf> {
    let mut v = vec![data_home()];
    v.extend(data_dirs());
    v
}

/// A user folder (`"PICTURES"`, `"DOWNLOAD"`…) as user-dirs.dirs names it, as `xdg-user-dir`
/// reports it: the environment first, then the file, else the home folder.
pub fn user_dir(name: &str) -> PathBuf {
    let key = format!("XDG_{name}_DIR");
    if let Some(v) = std::env::var_os(&key).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let file = std::fs::read_to_string(config_home().join("user-dirs.dirs")).unwrap_or_default();
    user_dir_in(&file, &key).unwrap_or_else(home)
}

fn user_dir_in(file: &str, key: &str) -> Option<PathBuf> {
    file.lines().find_map(|l| {
        let v = l.trim().strip_prefix(key)?.trim_start().strip_prefix('=')?;
        let v = v.trim().trim_matches('"');
        Some(match v.strip_prefix("$HOME") {
            Some(rest) => home().join(rest.trim_start_matches('/')),
            None => PathBuf::from(v),
        })
    })
}

const DEFAULTS: &str = "[Default Applications]";
const ADDED: &str = "[Added Associations]";
const REMOVED: &str = "[Removed Associations]";

/// `type=` in a mimeapps.list's [Default Applications]: the first such line.
fn default_in(text: &str, mime: &str) -> Option<String> {
    entry_in(text, DEFAULTS, mime)
}

/// `type=` in one section of a mimeapps.list: the first such line.
fn entry_in(text: &str, section: &str, mime: &str) -> Option<String> {
    let prefix = format!("{mime}=");
    let mut in_default = false;
    for l in text.lines() {
        if l.starts_with(section) {
            in_default = true;
        } else if l.starts_with('[') {
            in_default = false;
        } else if in_default && let Some(v) = l.strip_prefix(&prefix) {
            return Some(v.to_string());
        }
    }
    None
}

/// The app (desktop file id) that opens `mime`, as `xdg-mime query default` answers: the
/// chosen default, else an app that lists the type.
pub fn default_app(mime: &str) -> Option<String> {
    chosen_app(mime).or_else(|| listing_app(mime))
}

/// Every mimeapps.list's text, most important first: the config directories, then the data
/// directories' applications, each desktop's own list before the shared one.
fn mimeapps_lists() -> Vec<String> {
    let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|d| !d.is_empty())
        .map(|d| format!("{}-", d.to_lowercase()))
        .chain([String::new()])
        .collect();
    let mut dirs: Vec<PathBuf> = std::iter::once(config_home())
        .chain(config_dirs())
        .collect();
    dirs.extend(data_all().into_iter().map(|d| d.join("applications")));
    dirs.iter()
        .flat_map(|d| {
            desktops
                .iter()
                .filter_map(|p| std::fs::read_to_string(d.join(format!("{p}mimeapps.list"))).ok())
        })
        .collect()
}

/// The first installed app of a `a.desktop;b.desktop;` list, not in `removed`.
fn first_installed(list: &str, removed: &[String]) -> Option<String> {
    list.split(';')
        .find(|a| {
            !a.is_empty()
                && !removed.iter().any(|r| r == a)
                && crate::apps::desktop_file(a).is_some()
        })
        .map(str::to_string)
}

/// The default chosen for `mime` in a mimeapps.list, if any.
pub fn chosen_app(mime: &str) -> Option<String> {
    // 1. [Default Applications], the first list naming the type with an installed app.
    mimeapps_lists()
        .iter()
        .find_map(|t| first_installed(&default_in(t, mime)?, &[]))
}

/// An installed app that lists `mime` (defaults.list, mimeinfo.cache, else the desktop files,
/// the highest InitialPreference first): no one chose it, it only says it can.
pub fn listing_app(mime: &str) -> Option<String> {
    // The apps the user took away from the type.
    let lists = mimeapps_lists();
    let removed: Vec<String> = lists
        .iter()
        .filter_map(|t| entry_in(t, REMOVED, mime))
        .flat_map(|v| v.split(';').map(str::to_string).collect::<Vec<_>>())
        .collect();
    // 2. [Added Associations]: apps the user (or an "Open with" choice) added for the type, as
    // GIO reads them before the caches (xdg-mime skips them).
    if let Some(app) = lists
        .iter()
        .find_map(|t| first_installed(&entry_in(t, ADDED, mime)?, &removed))
    {
        return Some(app);
    }
    // 3. defaults.list and mimeinfo.cache (with the menu prefix, then without).
    let menu = std::env::var("XDG_MENU_PREFIX").unwrap_or_default();
    let line = format!("{mime}=");
    for dir in data_all() {
        for prefix in [menu.as_str(), ""] {
            for name in ["defaults.list", "mimeinfo.cache"] {
                let p = dir.join("applications").join(format!("{prefix}{name}"));
                let Ok(text) = std::fs::read_to_string(p) else {
                    continue;
                };
                if let Some(app) = text
                    .lines()
                    .find_map(|l| first_installed(l.strip_prefix(&line)?, &removed))
                {
                    return Some(app);
                }
            }
        }
    }
    // 4. Any desktop file listing the type, the highest InitialPreference first.
    let wanted = format!("{mime};");
    let mut best: Option<(i64, String)> = None;
    for dir in data_all() {
        let mut stack = vec![dir.join("applications")];
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
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                if !text.contains(&wanted) {
                    continue;
                }
                let pref = text
                    .lines()
                    .find_map(|l| l.split_once("InitialPreference=").map(|(_, v)| v.trim()))
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                if best.as_ref().is_none_or(|(b, _)| pref > *b)
                    && let Some(name) = p.file_name()
                    && !removed.iter().any(|r| **r == *name.to_string_lossy())
                {
                    best = Some((pref, name.to_string_lossy().into_owned()));
                }
            }
        }
    }
    best.map(|(_, id)| id)
}

/// `text` (a mimeapps.list) with `mime=app` in [Default Applications], replacing the line that
/// was there, as xdg-mime's make_default_generic writes it.
fn with_default(text: &str, mime: &str, app: &str) -> String {
    let prefix = format!("{mime}=");
    let entry = format!("{prefix}{app}");
    let mut out: Vec<String> = Vec::new();
    let (mut in_default, mut added, mut found, mut blanks) = (false, false, false, 0usize);
    for l in text.lines() {
        let mut suppress = false;
        let mut line = l.to_string();
        if l.starts_with("[Default Applications]") {
            in_default = true;
            found = true;
        } else if l.starts_with('[') {
            if !added && in_default {
                out.push(entry.clone());
                added = true;
            }
            in_default = false;
        } else if l.is_empty() {
            suppress = true;
            blanks += 1;
        } else if in_default && !added && l.starts_with(&prefix) {
            line.clone_from(&entry);
            added = true;
        }
        if !suppress {
            out.extend(std::iter::repeat_n(String::new(), blanks));
            blanks = 0;
            out.push(line);
        }
    }
    if !added {
        if !found {
            out.push(String::new());
            out.push("[Default Applications]".into());
        }
        out.push(entry);
    }
    out.extend(std::iter::repeat_n(String::new(), blanks));
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// Makes `app` the default for each of `types` in the user's mimeapps.list (a link is followed
/// to its file, as xdg-mime does). Written whole and renamed in, so a reader never sees half.
pub fn set_default(types: &[&str], app: &str) -> std::io::Result<()> {
    let list = config_home().join("mimeapps.list");
    let file = std::fs::canonicalize(&list).unwrap_or(list);
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    for t in types {
        text = with_default(&text, t, app);
    }
    write_whole(&file, &text)
}

fn write_whole(file: &Path, text: &str) -> std::io::Result<()> {
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = file.with_extension("list.kestrel-new");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_read_from_its_section_only() {
        let t = "[Added Associations]\ntext/plain=a.desktop;\n\n[Default Applications]\ntext/plain=b.desktop\n";
        assert_eq!(default_in(t, "text/plain").as_deref(), Some("b.desktop"));
        assert_eq!(default_in(t, "text/html"), None);
    }

    #[test]
    fn writing_replaces_adds_and_creates_like_xdg_mime() {
        let t = "[Default Applications]\ntext/plain=b.desktop\n\n[Added Associations]\nx=y\n";
        assert_eq!(
            with_default(t, "text/plain", "c.desktop"),
            "[Default Applications]\ntext/plain=c.desktop\n\n[Added Associations]\nx=y\n"
        );
        assert_eq!(
            with_default(t, "image/png", "v.desktop"),
            "[Default Applications]\ntext/plain=b.desktop\nimage/png=v.desktop\n\n[Added Associations]\nx=y\n"
        );
        assert_eq!(
            with_default("", "image/png", "v.desktop"),
            "\n[Default Applications]\nimage/png=v.desktop\n"
        );
    }

    #[test]
    fn user_dirs_expand_home() {
        let f = "# comment\nXDG_PICTURES_DIR=\"$HOME/Pictures\"\n";
        assert_eq!(
            user_dir_in(f, "XDG_PICTURES_DIR"),
            Some(home().join("Pictures"))
        );
        assert_eq!(user_dir_in(f, "XDG_MUSIC_DIR"), None);
    }
}

#[cfg(test)]
mod live {
    /// Prints this machine's answers next to xdg-mime's (run by hand: `--ignored`). They differ
    /// only where xdg-mime skips [Added Associations], which the mime-apps spec and GIO read.
    #[test]
    #[ignore]
    fn same_answers_as_xdg_mime() {
        for k in crate::defaults::KINDS {
            for t in k.types {
                let ours = super::default_app(t);
                let theirs = std::process::Command::new("xdg-mime")
                    .args(["query", "default", t])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty());
                let mark = if ours == theirs { "" } else { "  <- differs" };
                println!("{t}: {ours:?} {theirs:?}{mark}");
            }
        }
    }
}
