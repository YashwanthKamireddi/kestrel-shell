//! GSettings, read and written directly (no `gsettings` processes, no GLib in the shell).
//!
//! A key's value is the user's own from dconf's database (`$XDG_CONFIG_HOME/dconf/user`), else
//! the schema's default from the compiled schemas (`glib-2.0/schemas/gschemas.compiled` in the
//! data directories, vendor overrides already applied). Both are GVDB files. A change is sent
//! to the dconf service (`ca.desrt.dconf.Writer.Change`) as GSettings itself sends it: a
//! serialised `a{smv}` changeset of full key paths, so every app sees it and is told.
//!
//! The database layouts follow GLib's gsettingsschema.c (a schema's table holds `.path` and,
//! per key, a tuple whose first item is the default) and dconf's dconf-changeset.c.

use std::collections::HashMap;
use std::path::PathBuf;
use zbus::zvariant::{OwnedValue, Value};
use zgvariant::Variant;

fn compiled_schemas() -> Vec<PathBuf> {
    // GSETTINGS_SCHEMA_DIR first, as GLib reads it, then each data directory.
    let mut v: Vec<PathBuf> = std::env::var_os("GSETTINGS_SCHEMA_DIR")
        .map(|d| vec![PathBuf::from(d)])
        .unwrap_or_default();
    v.extend(
        crate::xdg::data_all()
            .into_iter()
            .map(|d| d.join("glib-2.0/schemas")),
    );
    v.into_iter()
        .map(|d| d.join("gschemas.compiled"))
        .filter(|f| f.is_file())
        .collect()
}

/// A schema's path in dconf (`/org/gnome/desktop/interface/`) and the key's default.
fn schema_key(schema: &str, key: &str) -> Option<(String, OwnedValue)> {
    for file in compiled_schemas() {
        let Ok(f) = gvdb::read::File::from_file(&file) else {
            continue;
        };
        let Ok(root) = f.hash_table() else {
            continue;
        };
        let Ok(t) = root.get_hash_table(schema) else {
            continue;
        };
        let path = t
            .get_value(".path")
            .ok()
            .and_then(|v| String::try_from(unwrap_variant(v)).ok())?;
        let data = t.get_value(key).ok()?;
        // (default, (code, extra)…): the default comes first.
        let default = match unwrap_variant(data) {
            Value::Structure(s) => s.into_fields().into_iter().next()?,
            other => other,
        };
        return Some((path, unwrap_variant(default).try_to_owned().ok()?));
    }
    None
}

fn unwrap_variant(v: Value<'_>) -> Value<'_> {
    match v {
        Value::Value(inner) => unwrap_variant(*inner),
        v => v,
    }
}

fn user_db() -> PathBuf {
    crate::xdg::config_home().join("dconf/user")
}

/// The user's own value at a full dconf path, if they set one.
fn user_value(path: &str) -> Option<OwnedValue> {
    let f = gvdb::read::File::from_file(&user_db()).ok()?;
    let t = f.hash_table().ok()?;
    let v = t.get_value(path).ok()?;
    unwrap_variant(v).try_to_owned().ok()
}

/// A key's current value: the user's, else the schema's default.
pub fn get(schema: &str, key: &str) -> Option<OwnedValue> {
    let (path, default) = schema_key(schema, key)?;
    Some(user_value(&format!("{path}{key}")).unwrap_or(default))
}

pub fn get_string(schema: &str, key: &str) -> Option<String> {
    String::try_from(get(schema, key)?).ok()
}

pub fn get_f64(schema: &str, key: &str) -> Option<f64> {
    f64::try_from(get(schema, key)?).ok()
}

pub fn get_bool(schema: &str, key: &str) -> Option<bool> {
    bool::try_from(get(schema, key)?).ok()
}

pub fn get_i64(schema: &str, key: &str) -> Option<i64> {
    let v = get(schema, key)?;
    match &*v {
        Value::I32(i) => Some(i64::from(*i)),
        Value::U32(u) => Some(i64::from(*u)),
        Value::I64(i) => Some(*i),
        Value::U64(u) => i64::try_from(*u).ok(),
        Value::I16(i) => Some(i64::from(*i)),
        Value::U16(u) => Some(i64::from(*u)),
        Value::U8(u) => Some(i64::from(*u)),
        _ => None,
    }
}

/// A value to store, in the key's own type.
#[derive(Clone, Debug, PartialEq)]
pub enum Set {
    Str(String),
    Bool(bool),
    F64(f64),
    /// Whole numbers: stored as the key's integer type (`i`, `u`…).
    Int(i64),
}

/// `v` as a value of the same type as `like` (the key's default), if it fits.
fn typed(v: &Set, like: &Value<'_>) -> Option<Variant<'static>> {
    Some(match (v, like) {
        (Set::Str(s), Value::Str(_)) => Variant::from(s.clone()),
        (Set::Bool(b), Value::Bool(_)) => Variant::from(*b),
        (Set::F64(f), Value::F64(_)) => Variant::from(*f),
        (Set::Int(i), Value::F64(_)) => Variant::from(*i as f64),
        (Set::Int(i), Value::I32(_)) => Variant::from(i32::try_from(*i).ok()?),
        (Set::Int(i), Value::U32(_)) => Variant::from(u32::try_from(*i).ok()?),
        (Set::Int(i), Value::I64(_)) => Variant::from(*i),
        (Set::Int(i), Value::U64(_)) => Variant::from(u64::try_from(*i).ok()?),
        (Set::Int(i), Value::I16(_)) => Variant::from(i16::try_from(*i).ok()?),
        (Set::Int(i), Value::U16(_)) => Variant::from(u16::try_from(*i).ok()?),
        (Set::Int(i), Value::U8(_)) => Variant::from(u8::try_from(*i).ok()?),
        _ => return None,
    })
}

/// The changeset dconf's writer takes: `a{smv}` (full path → the value, or nothing to reset),
/// in GVariant's serialised form (dconf_changeset_serialise).
fn changeset(path: &str, value: Option<Variant<'static>>) -> Result<Vec<u8>, String> {
    let mut m: HashMap<String, Option<Variant<'static>>> = HashMap::new();
    m.insert(path.to_string(), value);
    let ctx = zgvariant::serialized::Context::new(zgvariant::LE, 0);
    zgvariant::to_bytes(ctx, &m)
        .map(|d| d.bytes().to_vec())
        .map_err(|e| e.to_string())
}

/// Sets a key (in its schema's type), as `gsettings set` does.
pub async fn set(schema: &str, key: &str, v: Set) -> Result<(), String> {
    let (path, default) =
        schema_key(schema, key).ok_or_else(|| format!("no key {schema} {key}"))?;
    let value =
        typed(&v, &default).ok_or_else(|| format!("{schema} {key}: {v:?} is not its type"))?;
    let blob = changeset(&format!("{path}{key}"), Some(value))?;
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    conn.call_method(
        Some("ca.desrt.dconf"),
        "/ca/desrt/dconf/Writer/user",
        Some("ca.desrt.dconf.Writer"),
        "Change",
        &(blob,),
    )
    .await
    .map(|_| ())
    .map_err(|e| format!("{schema} {key}: {e}"))
}

pub async fn set_string(schema: &str, key: &str, v: &str) -> Result<(), String> {
    set(schema, key, Set::Str(v.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_schema_default() {
        // GNOME's interface schema ships with GTK; skipped where it is absent.
        if let Some((path, v)) = schema_key("org.gnome.desktop.interface", "cursor-size") {
            assert_eq!(path, "/org/gnome/desktop/interface/");
            assert!(matches!(&*v, Value::I32(_)));
        }
    }

    #[test]
    fn changeset_is_gvariant_a_smv() {
        // A string value: the dict entry is the key, then the maybe-variant; GLib's own
        // serialisation of {'/a/b': <'x'>} as a{smv}.
        let b = changeset("/a/b", Some(Variant::from("x"))).unwrap();
        assert!(b.starts_with(b"/a/b\0"));
        assert!(b.windows(2).any(|w| w == b"x\0"));
        assert!(b.windows(2).any(|w| w == b"\0s"));
    }
}

#[cfg(test)]
mod live {
    /// Reads keys here and through the gsettings tool (by hand: `--ignored`).
    #[test]
    #[ignore]
    fn same_values_as_gsettings() {
        for (schema, key) in [
            ("org.gnome.desktop.interface", "cursor-theme"),
            ("org.gnome.desktop.interface", "cursor-size"),
            ("org.gnome.desktop.interface", "text-scaling-factor"),
            ("org.gnome.desktop.interface", "gtk-theme"),
            ("org.gnome.desktop.interface", "font-name"),
            ("org.gnome.desktop.interface", "color-scheme"),
            ("org.gnome.desktop.sound", "theme-name"),
            ("org.gnome.desktop.privacy", "remember-recent-files"),
            ("org.gnome.desktop.privacy", "recent-files-max-age"),
            ("org.gnome.desktop.privacy", "old-files-age"),
        ] {
            let ours = super::get(schema, key).map(|v| format!("{:?}", *v));
            let theirs = std::process::Command::new("gsettings")
                .args(["get", schema, key])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            println!("{schema} {key}: ours {ours:?} | gsettings {theirs}");
        }
    }
}

#[cfg(test)]
mod live_write {
    /// Writes values the keys already have (by hand: `--ignored`).
    #[tokio::test]
    #[ignore]
    async fn writes_through_dconf() {
        let theme = super::get_string("org.gnome.desktop.interface", "cursor-theme").unwrap();
        super::set_string("org.gnome.desktop.interface", "cursor-theme", &theme)
            .await
            .unwrap();
        super::set(
            "org.gnome.desktop.privacy",
            "old-files-age",
            super::Set::Int(30),
        )
        .await
        .unwrap();
    }
}
