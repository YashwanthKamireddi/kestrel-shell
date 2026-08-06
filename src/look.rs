//! How apps look beyond colours (Settings > Appearance): their font and monospace font, the
//! icon theme and the cursor theme. Each is written everywhere apps read it, so GTK 3, GTK 4,
//! Qt and Hyprland agree:
//!
//! - fonts: gsettings (GTK 4 / libadwaita), gtk-3.0 and gtk-4.0 settings.ini, and Qt through
//!   kdeglobals and qt6ct (written by `apptheme` from the same config);
//! - icons: gsettings, settings.ini, kdeglobals `[Icons] Theme` (qt6ct follows it);
//! - cursor ("one cursor everywhere", the default env.lua): gsettings, settings.ini, the default theme
//!   (~/.local/share/icons/default), XCURSOR_THEME in settings.lua, and Hyprland's setcursor.
//!
//! The choices live in config.toml `[look]`; nothing is set until one is made.

use std::path::{Path, PathBuf};

/// The app font kestrel has always given Qt (the shell's), and the monospace one.
pub const DEFAULT_FONT: &str = crate::theme::FONT_NAME;
pub const DEFAULT_MONO: &str = "JetBrainsMono Nerd Font";

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn icon_dirs() -> Vec<PathBuf> {
    let h = home();
    let mut v = vec![h.join(".local/share/icons"), h.join(".icons")];
    v.push(PathBuf::from("/usr/share/icons"));
    v
}

/// Installed icon themes (with icons, not only cursors), by name.
pub fn icon_themes() -> Vec<String> {
    let mut out = Vec::new();
    for d in icon_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let index = std::fs::read_to_string(e.path().join("index.theme")).unwrap_or_default();
            // A theme with icon folders; "default" and hicolor are fallbacks, not choices.
            if index.contains("Directories=")
                && !matches!(name.as_str(), "default" | "hicolor")
                && !out.contains(&name)
            {
                out.push(name);
            }
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// Installed cursor themes, by name.
pub fn cursor_themes() -> Vec<String> {
    let mut out = Vec::new();
    for d in icon_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().join("cursors").is_dir() && name != "default" && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// Installed font families: all, and the monospace ones (fontconfig's own list).
pub fn fonts() -> (Vec<String>, Vec<String>) {
    let list = |args: &[&str]| -> Vec<String> {
        let out = std::process::Command::new("fc-list")
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let mut v: Vec<String> = out
            .lines()
            // "JetBrainsMono Nerd Font,JetBrainsMono NF": the first name is the family's own.
            .filter_map(|l| l.split(',').next())
            .map(|f| f.trim().replace('\\', ""))
            .filter(|f| !f.is_empty())
            .collect();
        v.sort_by_key(|n| n.to_lowercase());
        v.dedup();
        v
    };
    (list(&[":", "family"]), list(&[":spacing=100", "family"]))
}

async fn gsettings(key: &str, value: &str) {
    if let Err(e) =
        crate::services::gsettings::set_string("org.gnome.desktop.interface", key, value).await
    {
        log::warn!("{e}");
    }
}

/// Sets `key=value` in a GTK settings.ini `[Settings]` (made if missing), keeping the rest.
fn set_gtk_ini(file: &Path, key: &str, value: &str) {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let line = format!("{key}={value}");
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim_start().starts_with(&format!("{key}=")) {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        if !out.iter().any(|l| l.trim() == "[Settings]") {
            out.insert(0, "[Settings]".into());
        }
        let at = out
            .iter()
            .position(|l| l.trim() == "[Settings]")
            .map_or(out.len(), |i| i + 1);
        out.insert(at, line);
    }
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(file, out.join("\n") + "\n");
}

fn gtk_ini(key: &str, value: &str) {
    for v in ["gtk-3.0", "gtk-4.0"] {
        set_gtk_ini(
            &home().join(".config").join(v).join("settings.ini"),
            key,
            value,
        );
    }
}

/// Sets `key=value` in an ini `[section]` (KDE's kdeglobals), keeping the rest.
fn set_ini(file: &Path, section: &str, key: &str, value: &str) {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let head = format!("[{section}]");
    let mut out: Vec<String> = Vec::new();
    let (mut inside, mut done) = (false, false);
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if inside && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            inside = t == head;
        } else if inside && t.starts_with(&format!("{key}=")) {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !done {
        if !inside {
            out.push(String::new());
            out.push(head);
        }
        out.push(format!("{key}={value}"));
    }
    let _ = std::fs::write(file, out.join("\n") + "\n");
}

/// The app font and monospace font, everywhere (Qt follows at the next colour write, which
/// this starts).
pub async fn apply_fonts(font: String, size: u32, mono: String, mono_size: u32) {
    gsettings("font-name", &format!("{font} {size}")).await;
    gsettings("monospace-font-name", &format!("{mono} {mono_size}")).await;
    gtk_ini("gtk-font-name", &format!("{font} {size}"));
}

/// The icon theme, everywhere.
pub async fn apply_icons(theme: String) {
    gsettings("icon-theme", &theme).await;
    gtk_ini("gtk-icon-theme-name", &theme);
    set_ini(&home().join(".config/kdeglobals"), "Icons", "Theme", &theme);
}

/// The cursor theme, everywhere ("one cursor everywhere"): XCURSOR_THEME goes into
/// settings.lua with the rest of the [hyprland] choices (written by the caller's save).
pub async fn apply_cursor(theme: String, size: u32) {
    gsettings("cursor-theme", &theme).await;
    gtk_ini("gtk-cursor-theme-name", &theme);
    let default = home().join(".local/share/icons/default");
    let _ = std::fs::create_dir_all(&default);
    let _ = std::fs::write(
        default.join("index.theme"),
        format!("[Icon Theme]\nInherits={theme}\n"),
    );
    crate::services::hyprland::set_cursor(&theme, size).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_keys_replaced_or_added() {
        let dir = std::env::temp_dir().join(format!("kestrel-look-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("settings.ini");
        std::fs::write(
            &f,
            "[Settings]\ngtk-font-name=Cantarell 10\ngtk-theme-name=x\n",
        )
        .unwrap();
        set_gtk_ini(&f, "gtk-font-name", "Rubik 11");
        set_gtk_ini(&f, "gtk-icon-theme-name", "breeze");
        let t = std::fs::read_to_string(&f).unwrap();
        assert!(t.contains("gtk-font-name=Rubik 11\n"));
        assert!(t.contains("gtk-icon-theme-name=breeze\n"));
        assert!(t.contains("gtk-theme-name=x\n"));
        let k = dir.join("kdeglobals");
        std::fs::write(
            &k,
            "[General]\nfont=a\n\n[Icons]\nTheme=old\n\n[KDE]\nx=1\n",
        )
        .unwrap();
        set_ini(&k, "Icons", "Theme", "new");
        let t = std::fs::read_to_string(&k).unwrap();
        assert!(t.contains("[Icons]\nTheme=new\n"));
        assert!(!t.contains("Theme=old"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
