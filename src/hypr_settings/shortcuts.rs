//! His own keyboard shortcuts (kept in config.toml) and changing an existing binding's keys.

use super::*;

/// A shortcut added in Settings: keys ("SUPER + T"), a name, the command it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Shortcut {
    pub keys: String,
    pub name: String,
    pub command: String,
}

pub(super) fn lua_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

impl Shortcut {
    /// Its `hl.bind` line.
    pub fn lua(&self) -> String {
        format!(
            "hl.bind({}, hl.dsp.exec_cmd({}), {{ description = {} }})",
            lua_str(&self.keys),
            lua_str(&self.command),
            lua_str(&format!("Custom: {}", self.name))
        )
    }
}

pub(super) fn shortcuts_in(doc: &toml::Table) -> Vec<Shortcut> {
    doc.get("shortcuts")
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    let t = v.as_table()?;
                    Some(Shortcut {
                        keys: t.get("keys")?.as_str()?.to_string(),
                        name: t
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string(),
                        command: t.get("command")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The shortcuts added in Settings.
pub fn shortcuts() -> Vec<Shortcut> {
    let doc: toml::Table = std::fs::read_to_string(crate::config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    shortcuts_in(&doc)
}

/// Keeps `list` in config.toml (`[[shortcuts]]`) and settings.lua.
pub(super) fn keep_shortcuts(list: &[Shortcut]) {
    let mut arr = toml_edit::ArrayOfTables::new();
    for sc in list {
        let mut t = toml_edit::Table::new();
        t.insert("keys", toml_edit::value(sc.keys.clone()));
        t.insert("name", toml_edit::value(sc.name.clone()));
        t.insert("command", toml_edit::value(sc.command.clone()));
        arr.push(t);
    }
    crate::config::set(&[], "shortcuts", toml_edit::Item::ArrayOfTables(arr));
    write_lua();
}

/// Adds a shortcut: live at once, and kept.
pub async fn add_shortcut(sc: Shortcut) {
    crate::services::hyprland::eval(sc.lua()).await;
    let mut list = shortcuts();
    list.retain(|o| o.keys != sc.keys);
    list.push(sc);
    keep_shortcuts(&list);
}

/// Removes a shortcut (by its keys).
pub async fn remove_shortcut(keys: String) {
    crate::services::hyprland::eval(format!("hl.unbind({})", lua_str(&keys))).await;
    let mut list = shortcuts();
    list.retain(|o| o.keys != keys);
    keep_shortcuts(&list);
}

/// Keys as one comparable form: lower case, each key once, modifiers in a set order.
pub(super) fn keys_norm(keys: &str) -> Vec<String> {
    let mut v: Vec<String> = keys
        .split('+')
        .map(|k| match k.trim().to_lowercase().as_str() {
            "/" => "slash".to_string(),
            "enter" => "return".to_string(),
            "prtsc" => "print".to_string(),
            "control" => "ctrl".to_string(),
            "win" | "mod4" => "super".to_string(),
            o => o.to_string(),
        })
        .filter(|k| !k.is_empty())
        .collect();
    v.sort();
    v
}

/// Changes a bind's keys in his own Hyprland config: the one `hl.bind("<keys>", …)` whose keys
/// are `old` gets `new` instead (nothing else on the line changes), then Hyprland reloads.
/// Refused when that bind is not written out as keys (made in a loop), is written more than
/// once, or `new` is already a bind there.
pub async fn rebind(old: String, new: String) -> Result<(), String> {
    let conf = std::env::var_os("HOME")
        .map(|h| std::path::Path::new(&h).join(".config/hypr/conf"))
        .ok_or("no home folder")?;
    rebind_in(&conf, &old, &new)?;
    crate::services::hyprland::reload().await;
    Ok(())
}

/// The file change of `rebind`, in the config folder `conf`.
pub(super) fn rebind_in(conf: &std::path::Path, old: &str, new: &str) -> Result<(), String> {
    let files: Vec<std::path::PathBuf> = std::fs::read_dir(conf)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        // settings.lua is kestrel's own (custom shortcuts are changed there by Settings).
        .filter(|p| {
            p.extension().is_some_and(|e| e == "lua")
                && p.file_name().is_some_and(|n| n != "settings.lua")
        })
        .collect();
    let (want, new_norm) = (keys_norm(old), keys_norm(new));
    let mut hits = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let Some(rest) = line.trim_start().strip_prefix("hl.bind(\"") else {
                continue;
            };
            let Some(end) = rest.find('"') else { continue };
            let literal = &rest[..end];
            let k = keys_norm(literal);
            if k == new_norm {
                return Err(format!("{new} is already a shortcut"));
            }
            if k == want {
                hits.push((f.clone(), i, literal.to_string()));
            }
        }
    }
    let (file, line_no, literal) =
        match hits.as_slice() {
            [one] => one.clone(),
            [] => return Err(
                "That shortcut is made by a rule in your config, not written out: change it there"
                    .into(),
            ),
            _ => return Err("That shortcut is written more than once in your config".into()),
        };
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let out: Vec<String> = text
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i == line_no {
                l.replacen(
                    &format!("hl.bind(\"{literal}\""),
                    &format!("hl.bind(\"{new}\""),
                    1,
                )
            } else {
                l.to_string()
            }
        })
        .collect();
    let mut body = out.join("\n");
    if text.ends_with('\n') {
        body.push('\n');
    }
    std::fs::write(&file, body).map_err(|e| e.to_string())
}
