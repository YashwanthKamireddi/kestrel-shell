//! Per-app window rules and per-device pointer speeds, kept in config.toml and written into
//! the settings file.

use super::*;

/// A rule for one app's windows, made in Settings > Windows. Sizes are shares of the screen
/// (as in the default rules.lua), so a rule fits any monitor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowRule {
    /// The window class it matches, exactly.
    pub class: String,
    /// Open floating (true), tiled (false), or as Hyprland decides (None).
    pub float: Option<bool>,
    /// A floating window's size: "small", "medium" or "large".
    pub size: Option<String>,
    /// A floating window opens in the middle of the screen.
    pub center: bool,
    /// The workspace it opens on.
    pub workspace: Option<u32>,
    /// Its opacity, percent (100: opaque).
    pub opacity: Option<u32>,
    /// A floating window stays on top on every workspace.
    pub pin: bool,
}

/// Size presets as shares of the monitor (the medium and large of the default rules.lua).
pub const RULE_SIZES: [(&str, &str, f32, f32); 3] = [
    ("small", "Small", 0.35, 0.40),
    ("medium", "Medium", 0.45, 0.45),
    ("large", "Large", 0.60, 0.65),
];

impl WindowRule {
    /// Its name in Hyprland: one rule per app, replaced (or switched off) by name.
    pub(super) fn name(class: &str) -> String {
        let safe: String = class
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        format!("kestrel-app-{safe}")
    }

    /// The class as a whole-match pattern (dots and other regex characters taken literally).
    pub(super) fn pattern(class: &str) -> String {
        let mut p = String::from("^(");
        for c in class.chars() {
            if "\\.+*?()|[]{}^$".contains(c) {
                p.push('\\');
            }
            p.push(c);
        }
        p.push_str(")$");
        p
    }

    /// Its `hl.window_rule` line.
    pub fn lua(&self) -> String {
        let mut fields = vec![
            format!("name = {}", lua_str(&Self::name(&self.class))),
            format!(
                "match = {{ class = {} }}",
                lua_str(&Self::pattern(&self.class))
            ),
        ];
        match self.float {
            Some(true) => fields.push("float = true".into()),
            Some(false) => fields.push("tile = true".into()),
            None => {}
        }
        if self.float == Some(true) {
            if let Some((_, _, w, h)) = RULE_SIZES
                .iter()
                .find(|(k, ..)| Some(*k) == self.size.as_deref())
            {
                fields.push(format!(
                    "size = {{ \"(monitor_w*{w:.2})\", \"(monitor_h*{h:.2})\" }}"
                ));
            }
            if self.center {
                fields.push("center = true".into());
            }
            if self.pin {
                fields.push("pin = true".into());
            }
        }
        if let Some(n) = self.workspace {
            fields.push(format!("workspace = \"{n}\""));
        }
        if let Some(o) = self.opacity.filter(|o| *o < 100) {
            fields.push(format!("opacity = {:.2}", o as f32 / 100.0));
        }
        format!("hl.window_rule({{ {} }})", fields.join(", "))
    }

    /// The line that switches an app's rule off (live; the file then no longer has it).
    pub(super) fn lua_off(class: &str) -> String {
        format!(
            "hl.window_rule({{ name = {}, match = {{ class = {} }}, enabled = false }})",
            lua_str(&Self::name(class)),
            lua_str(&Self::pattern(class))
        )
    }

    /// What it does, in a few words ("Floats · Medium · Centred · Workspace 3").
    pub fn summary(&self) -> String {
        let mut v: Vec<String> = Vec::new();
        match self.float {
            Some(true) => {
                v.push("Floats".into());
                if let Some((_, n, ..)) = RULE_SIZES
                    .iter()
                    .find(|(k, ..)| Some(*k) == self.size.as_deref())
                {
                    v.push((*n).into());
                }
                if self.center {
                    v.push("Centred".into());
                }
                if self.pin {
                    v.push("On top".into());
                }
            }
            Some(false) => v.push("Tiles".into()),
            None => {}
        }
        if let Some(n) = self.workspace {
            v.push(format!("Workspace {n}"));
        }
        if let Some(o) = self.opacity.filter(|o| *o < 100) {
            v.push(format!("{o}% opacity"));
        }
        if v.is_empty() {
            "No changes".into()
        } else {
            v.join(" · ")
        }
    }

    pub(super) fn toml(&self) -> toml_edit::Table {
        let mut t = toml_edit::Table::new();
        t.insert("class", toml_edit::value(self.class.clone()));
        if let Some(f) = self.float {
            t.insert("float", toml_edit::value(f));
        }
        if let Some(sz) = &self.size {
            t.insert("size", toml_edit::value(sz.clone()));
        }
        if self.center {
            t.insert("center", toml_edit::value(true));
        }
        if let Some(n) = self.workspace {
            t.insert("workspace", toml_edit::value(i64::from(n)));
        }
        if let Some(o) = self.opacity {
            t.insert("opacity", toml_edit::value(i64::from(o)));
        }
        if self.pin {
            t.insert("pin", toml_edit::value(true));
        }
        t
    }
}

pub(super) fn window_rules_in(doc: &toml::Table) -> Vec<WindowRule> {
    doc.get("window_rules")
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    let t = v.as_table()?;
                    let num = |k: &str| {
                        t.get(k)
                            .and_then(toml::Value::as_integer)
                            .and_then(|n| u32::try_from(n).ok())
                    };
                    let flag = |k: &str| t.get(k).and_then(toml::Value::as_bool).unwrap_or(false);
                    Some(WindowRule {
                        class: t.get("class")?.as_str()?.to_string(),
                        float: t.get("float").and_then(toml::Value::as_bool),
                        size: t.get("size").and_then(|v| v.as_str()).map(str::to_string),
                        center: flag("center"),
                        workspace: num("workspace"),
                        opacity: num("opacity"),
                        pin: flag("pin"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The window rules made in Settings.
pub fn window_rules() -> Vec<WindowRule> {
    let doc: toml::Table = std::fs::read_to_string(crate::config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    window_rules_in(&doc)
}

pub(super) fn keep_window_rules(list: &[WindowRule]) {
    let mut arr = toml_edit::ArrayOfTables::new();
    for r in list {
        arr.push(r.toml());
    }
    crate::config::set(&[], "window_rules", toml_edit::Item::ArrayOfTables(arr));
    write_lua();
}

/// Sets an app's rule (replacing its old one): live at once for windows opened from now on,
/// and kept.
pub async fn set_window_rule(rule: WindowRule) {
    crate::services::hyprland::eval(rule.lua()).await;
    let mut list = window_rules();
    match list.iter_mut().find(|r| r.class == rule.class) {
        Some(r) => *r = rule,
        None => list.push(rule),
    }
    keep_window_rules(&list);
}

/// Removes an app's rule: switched off live, gone from the file.
pub async fn remove_window_rule(class: String) {
    crate::services::hyprland::eval(WindowRule::lua_off(&class)).await;
    let mut list = window_rules();
    list.retain(|r| r.class != class);
    keep_window_rules(&list);
}

/// Pointer speed for one device (`[[devices]]` in config.toml → `hl.device` in settings.lua),
/// over the global one.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceSpeed {
    pub name: String,
    pub sensitivity: f32,
}

impl DeviceSpeed {
    pub(super) fn lua(&self) -> String {
        format!(
            "hl.device({{ name = {}, sensitivity = {:.2} }})",
            lua_str(&self.name),
            self.sensitivity
        )
    }
}

pub(super) fn devices_in(doc: &toml::Table) -> Vec<DeviceSpeed> {
    doc.get("devices")
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    let t = v.as_table()?;
                    Some(DeviceSpeed {
                        name: t.get("name")?.as_str()?.to_string(),
                        sensitivity: t
                            .get("sensitivity")
                            .and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
                            .unwrap_or(0.0) as f32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The per-device speeds set in Settings.
pub fn device_speeds() -> Vec<DeviceSpeed> {
    let doc: toml::Table = std::fs::read_to_string(crate::config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    devices_in(&doc)
}

/// A device's speed: live at once; kept too when `keep` (a slider let go).
pub async fn set_device_speed(d: DeviceSpeed, keep: bool) {
    crate::services::hyprland::eval(d.lua()).await;
    if !keep {
        return;
    }
    let mut list = device_speeds();
    match list.iter_mut().find(|x| x.name == d.name) {
        Some(x) => *x = d,
        None => list.push(d),
    }
    let mut arr = toml_edit::ArrayOfTables::new();
    for x in &list {
        let mut t = toml_edit::Table::new();
        t.insert("name", toml_edit::value(x.name.clone()));
        t.insert(
            "sensitivity",
            toml_edit::value(f64::from((x.sensitivity * 100.0).round() / 100.0)),
        );
        arr.push(t);
    }
    crate::config::set(&[], "devices", toml_edit::Item::ArrayOfTables(arr));
    write_lua();
}
