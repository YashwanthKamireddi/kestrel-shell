//! System settings (keyboard, mouse, touchpad, windows) chosen in Settings, which are
//! Hyprland options. A choice is applied at once (`hl.config` through Hyprland's eval) and kept in
//! kestrel's config.toml under `[hyprland]`, nested as `hl.config` nests them; kestrel writes
//! that table out as `~/.config/hypr/conf/settings.lua`, which hyprland.lua loads after
//! conf/input.lua, so the choices survive a restart and his own files are never rewritten.
//! Only what was changed here goes in that file.

mod options;
mod rules;
mod shortcuts;
pub use options::*;
pub use rules::*;
pub use shortcuts::*;

use std::collections::HashMap;

/// The cursor's size everywhere: Hyprland (setcursor), GTK (gsettings and settings.ini) and
/// apps drawing their own (XCURSOR_SIZE, written into settings.lua). Run at start too, since
/// the session's own setcursor (execs.lua) runs before kestrel.
pub async fn apply_cursor(size: u32) {
    use crate::services::gsettings as g;
    const I: &str = "org.gnome.desktop.interface";
    let theme = g::get_string(I, "cursor-theme")
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Adwaita".into());
    crate::services::hyprland::set_cursor(&theme, size).await;
    if let Err(e) = g::set(I, "cursor-size", g::Set::Int(i64::from(size))).await {
        log::warn!("{e}");
    }
    if let Some(home) = std::env::var_os("HOME") {
        for v in ["gtk-3.0", "gtk-4.0"] {
            let p = std::path::Path::new(&home)
                .join(".config")
                .join(v)
                .join("settings.ini");
            if let Ok(text) = std::fs::read_to_string(&p)
                && text.contains("gtk-cursor-theme-size=")
            {
                let new: String = text
                    .lines()
                    .map(|l| {
                        if l.starts_with("gtk-cursor-theme-size=") {
                            format!("gtk-cursor-theme-size={size}")
                        } else {
                            l.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                if new != text {
                    let _ = std::fs::write(&p, new);
                }
            }
        }
    }
    write_lua();
}

/// Text size in apps (GTK's text scaling: 1.0 normal).
pub async fn set_text_scale(f: f32) {
    // Two decimals, as the slider shows it.
    let f = (f64::from(f) * 100.0).round() / 100.0;
    let r = crate::services::gsettings::set(
        "org.gnome.desktop.interface",
        "text-scaling-factor",
        crate::services::gsettings::Set::F64(f),
    )
    .await;
    if let Err(e) = r {
        log::warn!("{e}");
    }
}

pub async fn text_scale() -> f32 {
    crate::services::gsettings::get_f64("org.gnome.desktop.interface", "text-scaling-factor")
        .map_or(1.0, |f| f as f32)
}

/// A value as Hyprland reports it (and as it is set).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Num(f32),
    Str(String),
}

impl Value {
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => s == "true",
        }
    }
    pub fn as_num(&self) -> f32 {
        match self {
            Value::Num(n) => *n,
            Value::Bool(b) => f32::from(u8::from(*b)),
            Value::Str(s) => s.parse().unwrap_or(0.0),
        }
    }
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Num(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
        }
    }
    fn lua(&self, kind: Kind) -> String {
        match (self, kind) {
            (_, Kind::Switch) => self.as_bool().to_string(),
            (_, Kind::Range { step, .. }) if step >= 1.0 => {
                format!("{}", self.as_num().round() as i64)
            }
            (_, Kind::Range { .. }) => format!("{:.2}", self.as_num()),
            (_, Kind::Numbers(_)) => format!("{}", self.as_num().round() as i64),
            _ => format!(
                "\"{}\"",
                self.as_str().replace('\\', "\\\\").replace('"', "\\\"")
            ),
        }
    }
    fn toml(&self, kind: Kind) -> toml_edit::Item {
        match kind {
            Kind::Switch => toml_edit::value(self.as_bool()),
            Kind::Range { step, .. } if step >= 1.0 => {
                toml_edit::value(self.as_num().round() as i64)
            }
            Kind::Range { .. } => {
                toml_edit::value(f64::from((self.as_num() * 100.0).round() / 100.0))
            }
            Kind::Choice(_) => toml_edit::value(self.as_str()),
            Kind::Numbers(_) => toml_edit::value(self.as_num().round() as i64),
        }
    }
}

/// The current value of every option in `opts`, read from Hyprland.
pub async fn read(opts: Vec<Opt>) -> HashMap<&'static str, Value> {
    let mut out = HashMap::new();
    for o in opts {
        let Some(v) = crate::services::hyprland::option(o.option).await else {
            continue;
        };
        let value = if let Some(b) = v.get("bool").and_then(serde_json::Value::as_bool) {
            Value::Bool(b)
        } else if let Some(n) = v.get("int").and_then(serde_json::Value::as_i64) {
            // Switches that Hyprland keeps as numbers (clickfinger_behavior).
            if matches!(o.kind, Kind::Switch) {
                Value::Bool(n != 0)
            } else {
                Value::Num(n as f32)
            }
        } else if let Some(n) = v.get("float").and_then(serde_json::Value::as_f64) {
            Value::Num(n as f32)
        } else if let Some(css) = v.get("css").and_then(serde_json::Value::as_str) {
            // Gaps ("5 5 5 5"): one number when every side is the same; sides set apart in his
            // own config are not offered (a slider would flatten them).
            let sides: Vec<f32> = css
                .split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect();
            match sides.first() {
                Some(first) if sides.iter().all(|x| x == first) => Value::Num(*first),
                _ => continue,
            }
        } else if let Some(s) = v.get("str").and_then(serde_json::Value::as_str) {
            Value::Str(if s == "[[EMPTY]]" {
                String::new()
            } else {
                s.to_string()
            })
        } else {
            continue;
        };
        out.insert(o.option, value);
    }
    out
}

/// Applies `v` at once (not saved): for a slider while it moves.
pub async fn apply(o: Opt, v: Value) {
    crate::services::hyprland::eval(format!("hl.config({})", nest(o.path, &v.lua(o.kind)))).await;
}

/// Applies and keeps `v`: config.toml, then conf/settings.lua written from it.
pub async fn save(o: Opt, v: Value) {
    apply(o, v.clone()).await;
    let mut tables = vec!["hyprland"];
    tables.extend(o.tables());
    crate::config::set(&tables, o.key(), v.toml(o.kind));
    write_lua();
}

/// `["input", "touchpad", "x"]` and `true` as `{ input = { touchpad = { x = true } } }`.
fn nest(path: &[&str], value: &str) -> String {
    path.iter()
        .rev()
        .fold(value.to_string(), |inner, k| format!("{{ {k} = {inner} }}"))
}

/// config.toml's `[hyprland]` table written out as conf/settings.lua.
pub fn write_lua() {
    let doc: toml::Table = std::fs::read_to_string(crate::config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    let empty = toml::Table::new();
    let t = match doc.get("hyprland") {
        Some(toml::Value::Table(t)) => t,
        _ => &empty,
    };
    let cursor = doc.get("cursor_size").and_then(toml::Value::as_integer);
    let cursor_theme = doc
        .get("look")
        .and_then(|l| l.get("cursor"))
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let shortcuts = shortcuts_in(&doc);
    let rules = window_rules_in(&doc);
    let devices = devices_in(&doc);
    let editor = doc
        .get("editor")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    if t.is_empty()
        && cursor.is_none()
        && cursor_theme.is_none()
        && editor.is_none()
        && devices.is_empty()
        && shortcuts.is_empty()
        && rules.is_empty()
    {
        return;
    }
    let mut body = String::from(
        "-- Written by kestrel (Settings) from the [hyprland] table and cursor_size in\n\
         -- ~/.config/kestrel/config.toml. Changes here are overwritten; change them there or in\n\
         -- Settings. Loaded after conf/env.lua and conf/input.lua, so these win.\n\n",
    );
    if !t.is_empty() {
        body += &format!("hl.config({})\n", lua_table(t, 0));
    }
    if let Some(n) = cursor {
        body += &format!("hl.env(\"XCURSOR_SIZE\", \"{n}\")\n");
    }
    if let Some(e) = &editor {
        body += &format!(
            "hl.env(\"EDITOR\", {e})\nhl.env(\"VISUAL\", {e})\n",
            e = lua_str(e)
        );
    }
    if let Some(t) = &cursor_theme {
        body += &format!("hl.env(\"XCURSOR_THEME\", {})\n", lua_str(t));
    }
    if !devices.is_empty() {
        body += "\n-- Pointer speed per device (Settings > Mouse & touchpad).\n";
        for d in &devices {
            body += &d.lua();
            body.push('\n');
        }
    }
    if !rules.is_empty() {
        body += "\n-- Window rules made in Settings > Windows.\n";
        for r in &rules {
            body += &r.lua();
            body.push('\n');
        }
    }
    if !shortcuts.is_empty() {
        body += "\n-- Shortcuts added in Settings > Shortcuts.\n";
        for sc in &shortcuts {
            body += &sc.lua();
            body.push('\n');
        }
    }
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let hypr = std::path::Path::new(&home).join(".config/hypr");
    // Only alongside a Hyprland config of his own (its conf/ folder made if missing).
    if !hypr.is_dir() {
        return;
    }
    let _ = std::fs::create_dir_all(hypr.join("conf"));
    let path = hypr.join("conf/settings.lua");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(body.as_str())
        && let Err(e) = std::fs::write(&path, body)
    {
        log::warn!("settings.lua: {e}");
    }
}

fn lua_table(t: &toml::Table, depth: usize) -> String {
    let pad = "    ".repeat(depth + 1);
    let mut out = String::from("{\n");
    for (k, v) in t {
        let v = match v {
            toml::Value::Table(inner) => lua_table(inner, depth + 1),
            toml::Value::Boolean(b) => b.to_string(),
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::String(s) => {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
            _ => continue,
        };
        out += &format!("{pad}{k} = {v},\n");
    }
    out += &"    ".repeat(depth);
    out += "}";
    out
}

/// `layouts()`, read once.
pub fn layouts_cached() -> &'static [(String, String)] {
    static ALL: std::sync::LazyLock<Vec<(String, String)>> = std::sync::LazyLock::new(layouts);
    &ALL
}

/// Keyboard layouts the system knows: (code, name), "us" "English (US)".
pub fn layouts() -> Vec<(String, String)> {
    let text = std::fs::read_to_string("/usr/share/X11/xkb/rules/evdev.lst").unwrap_or_default();
    let mut on = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('!') {
            on = line.trim() == "! layout";
            continue;
        }
        if on && let Some((code, name)) = line.trim().split_once(char::is_whitespace) {
            out.push((code.to_string(), name.trim().to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_lines_are_quoted() {
        let sc = Shortcut {
            keys: "SUPER + T".into(),
            name: "Say \"hi\"".into(),
            command: "notify-send hi".into(),
        };
        assert_eq!(
            sc.lua(),
            "hl.bind(\"SUPER + T\", hl.dsp.exec_cmd(\"notify-send hi\"), { description = \"Custom: Say \\\"hi\\\"\" })"
        );
    }

    #[test]
    fn xkb_choices_keep_each_other() {
        let caps = &XKB[0];
        let compose = &XKB[2];
        let o = xkb_with("ctrl:nocaps", compose, "compose:ralt");
        assert_eq!(o, "ctrl:nocaps,compose:ralt");
        assert_eq!(xkb_value(&o, caps), "ctrl:nocaps");
        let o = xkb_with(&o, caps, "");
        assert_eq!(o, "compose:ralt");
    }

    #[test]
    fn nests_like_hl_config() {
        assert_eq!(
            nest(&["input", "touchpad", "tap_to_click"], "true"),
            "{ input = { touchpad = { tap_to_click = true } } }"
        );
    }

    #[test]
    fn writes_a_lua_table() {
        let t: toml::Table = "[input]\nrepeat_rate = 40\n[input.touchpad]\nscroll_factor = 0.7\n"
            .parse()
            .unwrap();
        let lua = lua_table(&t, 0);
        assert!(lua.contains("repeat_rate = 40,"));
        assert!(lua.contains("touchpad = {"));
        assert!(lua.contains("scroll_factor = 0.7,"));
    }

    #[test]
    fn layouts_read() {
        let l = layouts();
        if !l.is_empty() {
            assert!(l.iter().any(|(c, _)| c == "us"));
        }
    }
}

#[cfg(test)]
mod rule_tests {
    use super::*;

    #[test]
    fn writes_a_window_rule() {
        let r = WindowRule {
            class: "org.gnome.Calculator".into(),
            float: Some(true),
            size: Some("medium".into()),
            center: true,
            workspace: Some(3),
            opacity: Some(90),
            pin: false,
        };
        assert_eq!(
            r.lua(),
            "hl.window_rule({ name = \"kestrel-app-org-gnome-Calculator\", match = { class = \"^(org\\\\.gnome\\\\.Calculator)$\" }, float = true, size = { \"(monitor_w*0.45)\", \"(monitor_h*0.45)\" }, center = true, workspace = \"3\", opacity = 0.90 })"
        );
        assert_eq!(
            r.summary(),
            "Floats · Medium · Centred · Workspace 3 · 90% opacity"
        );
    }
}

#[cfg(test)]
mod rebind_tests {
    #[test]
    fn keys_compare_whatever_the_order_or_case() {
        assert_eq!(super::keys_norm("SUPER + V"), super::keys_norm("v + super"));
        assert_eq!(
            super::keys_norm("CTRL + SUPER + T"),
            super::keys_norm("SUPER+CONTROL+T")
        );
        assert_ne!(
            super::keys_norm("SUPER + V"),
            super::keys_norm("SUPER + SHIFT + V")
        );
    }

    #[test]
    fn rebinds_one_written_bind_only() {
        let dir = std::env::temp_dir().join(format!("kestrel-rebind-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("keybinds.lua");
        let text = concat!(
            "hl.bind(\"SUPER + V\", kestrel(\"toggle clipboard\"), { description = \"Shell: Clipboard history\" })\n",
            "hl.bind(\"SUPER + Period\", kestrel(\"toggle emoji\"), { description = \"Shell: Emoji\" })\n",
            "for _, b in ipairs(ws) do\n    hl.bind(b[1], hl.dsp.focus({ workspace = b[2] }))\nend\n",
        );
        std::fs::write(&f, text).unwrap();
        // Taken keys are refused; a bind made in a loop is refused.
        assert!(super::rebind_in(&dir, "SUPER + V", "SUPER + Period").is_err());
        assert!(super::rebind_in(&dir, "SUPER + 1", "SUPER + F1").is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), text);
        // A written one changes, and only its keys.
        super::rebind_in(&dir, "super + v", "SUPER + SHIFT + V").unwrap();
        let after = std::fs::read_to_string(&f).unwrap();
        assert!(after.starts_with("hl.bind(\"SUPER + SHIFT + V\", kestrel(\"toggle clipboard\")"));
        assert_eq!(
            after.lines().skip(1).collect::<Vec<_>>(),
            text.lines().skip(1).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
