//! Freedesktop icon lookup: name → file, searching the user's theme, then hicolor, then
//! pixmaps. Results (including misses) are cached for the life of the process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const SIZES: &[&str] = &[
    "scalable", "48x48", "64x64", "96x96", "128x128", "256x256", "32x32", "24x24", "512x512",
    "symbolic",
];
const KINDS: &[&str] = &[
    "apps",
    "devices",
    "status",
    "panel",
    "actions",
    "places",
    "mimetypes",
];

fn theme() -> String {
    // GTK settings carry the user's chosen theme.
    let home = std::env::var("HOME").unwrap_or_default();
    for f in [
        ".config/gtk-4.0/settings.ini",
        ".config/gtk-3.0/settings.ini",
    ] {
        if let Ok(s) = std::fs::read_to_string(Path::new(&home).join(f)) {
            for line in s.lines() {
                if let Some(v) = line.trim().strip_prefix("gtk-icon-theme-name") {
                    let v = v.trim_start_matches([' ', '=']).trim();
                    if !v.is_empty() {
                        return v.to_string();
                    }
                }
            }
        }
    }
    "Adwaita".into()
}

fn roots() -> &'static Vec<PathBuf> {
    static R: OnceLock<Vec<PathBuf>> = OnceLock::new();
    R.get_or_init(|| {
        let mut bases = Vec::new();
        if let Ok(h) = std::env::var("HOME") {
            bases.push(Path::new(&h).join(".local/share/icons"));
            bases.push(Path::new(&h).join(".icons"));
        }
        let data_dirs =
            std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
            bases.push(Path::new(d).join("icons"));
        }
        let mut themes = vec![theme()];
        for t in ["hicolor", "Adwaita", "breeze"] {
            if !themes.iter().any(|x| x == t) {
                themes.push(t.into());
            }
        }
        let mut roots = Vec::new();
        for t in &themes {
            for b in &bases {
                let p = b.join(t);
                if p.is_dir() {
                    roots.push(p);
                }
            }
        }
        roots
    })
}

fn search(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    for root in roots() {
        for size in SIZES {
            for kind in KINDS {
                for (layout, ext) in [
                    (root.join(size).join(kind), "svg"),
                    (root.join(size).join(kind), "png"),
                    (root.join(kind).join(size), "svg"),
                    (root.join(kind).join(size), "png"),
                ] {
                    let f = layout.join(format!("{name}.{ext}"));
                    if f.is_file() {
                        return Some(f);
                    }
                }
            }
        }
    }
    for ext in ["svg", "png", "xpm"] {
        let f = Path::new("/usr/share/pixmaps").join(format!("{name}.{ext}"));
        if f.is_file() && ext != "xpm" {
            return Some(f);
        }
    }
    None
}

pub fn lookup(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(name) {
        return hit.clone();
    }
    let found = search(name).or_else(|| search(&name.to_lowercase()));
    cache
        .lock()
        .unwrap()
        .insert(name.to_string(), found.clone());
    found
}
