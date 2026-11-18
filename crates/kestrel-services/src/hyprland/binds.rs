//! The key bindings Hyprland has, grouped as its cheatsheet groups them, with readable key
//! names.

use super::*;

/// A described key binding: the keys to press and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub keys: Vec<String>,
    pub what: String,
}

#[derive(Deserialize)]
pub(super) struct JBind {
    pub(super) modmask: u32,
    pub(super) key: String,
    #[serde(default)]
    pub(super) description: String,
    #[serde(default)]
    pub(super) has_description: bool,
    #[serde(default)]
    pub(super) dispatcher: String,
    #[serde(default)]
    pub(super) release: bool,
}

pub(super) fn key_name(k: &str) -> String {
    match k {
        "SUPER_L" | "SUPER_R" => "Super".into(),
        "Slash" => "/".into(),
        "Period" => ".".into(),
        "Comma" => ",".into(),
        "Return" => "Enter".into(),
        "Print" => "PrtSc".into(),
        "mouse_up" => "Scroll up".into(),
        "mouse_down" => "Scroll down".into(),
        "mouse:272" => "Left drag".into(),
        "mouse:273" => "Right drag".into(),
        k if k.starts_with("XF86") => k.trim_start_matches("XF86").into(),
        k if k.len() == 1 => k.to_uppercase(),
        k => k.to_string(),
    }
}

/// Described key bindings grouped by category ("Category: what it does"). Binds that call
/// the old shell's global shortcuts are left out: nothing listens to them any more.
pub async fn binds() -> Vec<(String, Vec<Bind>)> {
    let Ok(raw) = request("j/binds").await else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<JBind>>(&raw) else {
        return Vec::new();
    };
    let mut groups: Vec<(String, Vec<Bind>)> = Vec::new();
    for b in list {
        if !b.has_description || b.description.is_empty() || b.dispatcher.contains("global") {
            continue;
        }
        let (cat, what) = match b.description.split_once(':') {
            Some((c, w)) => (c.trim().to_string(), w.trim().to_string()),
            None => ("Other".to_string(), b.description.clone()),
        };
        let mut keys = Vec::new();
        for (bit, name) in [(64, "Super"), (4, "Ctrl"), (8, "Alt"), (1, "Shift")] {
            if b.modmask & bit != 0 && !(b.key.starts_with("SUPER") && bit == 64 && b.release) {
                keys.push(name.to_string());
            }
        }
        let k = key_name(&b.key);
        if !keys.contains(&k) {
            keys.push(k);
        }
        let bind = Bind { keys, what };
        match groups.iter_mut().find(|(c, _)| *c == cat) {
            Some((_, v)) => {
                if !v.contains(&bind) {
                    v.push(bind);
                }
            }
            None => groups.push((cat, vec![bind])),
        }
    }
    groups
}
