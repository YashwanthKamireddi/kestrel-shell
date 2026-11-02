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
    serde_json::from_str::<Vec<JBind>>(&raw).map_or_else(|_| Vec::new(), grouped)
}

/// Hyprland's binds (`j/binds`) as the cheatsheet groups them.
fn grouped(list: Vec<JBind>) -> Vec<(String, Vec<Bind>)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn binds(json: &str) -> Vec<(String, Vec<Bind>)> {
        grouped(serde_json::from_str(json).unwrap())
    }

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn binds_group_by_their_category_in_order() {
        let g = binds(
            r#"[
            {"modmask": 64, "key": "Return", "description": "Apps: Terminal", "has_description": true, "dispatcher": "exec"},
            {"modmask": 65, "key": "q", "description": "Windows: Close", "has_description": true, "dispatcher": "killactive"},
            {"modmask": 64, "key": "e", "description": "Apps: Files", "has_description": true, "dispatcher": "exec"},
            {"modmask": 0, "key": "Print", "description": "Screenshot", "has_description": true, "dispatcher": "exec"}
        ]"#,
        );
        let cats: Vec<&str> = g.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(cats, ["Apps", "Windows", "Other"]);
        assert_eq!(
            g[0].1,
            [
                Bind {
                    keys: keys(&["Super", "Enter"]),
                    what: "Terminal".into()
                },
                Bind {
                    keys: keys(&["Super", "E"]),
                    what: "Files".into()
                },
            ]
        );
        assert_eq!(g[1].1[0].keys, keys(&["Super", "Shift", "Q"]));
        assert_eq!(g[2].1[0].keys, keys(&["PrtSc"]));
    }

    #[test]
    fn undescribed_and_old_global_binds_are_left_out() {
        let g = binds(
            r#"[
            {"modmask": 64, "key": "a", "description": "", "has_description": false},
            {"modmask": 64, "key": "b", "description": "x: y", "has_description": false},
            {"modmask": 64, "key": "c", "description": "Shell: Overview", "has_description": true, "dispatcher": "global"}
        ]"#,
        );
        assert!(g.is_empty());
    }

    #[test]
    fn super_alone_on_release_is_one_key_not_two() {
        let g = binds(
            r#"[
            {"modmask": 64, "key": "SUPER_L", "description": "Shell: Launcher", "has_description": true, "release": true},
            {"modmask": 64, "key": "SUPER_L", "description": "Shell: Launcher", "has_description": true, "release": true},
            {"modmask": 12, "key": "mouse:272", "description": "Windows: Move", "has_description": true}
        ]"#,
        );
        assert_eq!(g[0].1.len(), 1, "a bind listed twice shows once");
        assert_eq!(g[0].1[0].keys, keys(&["Super"]));
        assert_eq!(g[1].1[0].keys, keys(&["Ctrl", "Alt", "Left drag"]));
    }

    #[test]
    fn key_names_read_as_on_the_keyboard() {
        assert_eq!(key_name("XF86AudioMute"), "AudioMute");
        assert_eq!(key_name("Slash"), "/");
        assert_eq!(key_name("k"), "K");
        assert_eq!(key_name("F12"), "F12");
        assert_eq!(key_name("mouse_down"), "Scroll down");
    }
}
