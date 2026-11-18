//! kestrel's desktop portal backend (`org.freedesktop.impl.portal.desktop.kestrel`): one name,
//! one connection, every interface kestrel answers for apps at
//! `/org/freedesktop/portal/desktop`, as GNOME's and KDE's backends are built. xdg-desktop-portal
//! routes to it what portals.conf names it for (the rest stays with the other backends).
//!
//! - FileChooser: kestrel's file picker (crate::filechooser).
//! - AppChooser: "Open with…" (crate::appchooser).
//! - Settings: the appearance apps follow (`org.freedesktop.appearance`): dark, kestrel's own
//!   accent colour (from the wallpaper or chosen), contrast, reduced motion. The frontend asks
//!   every Settings backend and the first listed wins per key (xdg-desktop-portal 1.22,
//!   src/settings.c), so GNOME's font, cursor and icon keys still come from the GTK backend
//!   listed after kestrel.

use crate::{appchooser, filechooser};
use futures::Stream;
use std::collections::HashMap;
use std::sync::Mutex;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.kestrel";
const PATH: &str = "/org/freedesktop/portal/desktop";
const APPEARANCE: &str = "org.freedesktop.appearance";

/// How apps should look, as kestrel looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Appearance {
    /// kestrel's accent (sRGB, 0 to 1); None: no preference (theming apps is off).
    pub accent: Option<(f64, f64, f64)>,
    /// Higher contrast asked for.
    pub high_contrast: bool,
    /// Less motion asked for.
    pub reduced_motion: bool,
}

impl Appearance {
    /// The `org.freedesktop.appearance` keys and their values.
    fn keys(&self) -> Vec<(&'static str, OwnedValue)> {
        let mut v: Vec<(&'static str, OwnedValue)> = vec![
            // kestrel is dark: apps are asked to be too (1: prefer dark).
            ("color-scheme", OwnedValue::from(1u32)),
            ("contrast", OwnedValue::from(u32::from(self.high_contrast))),
            (
                "reduced-motion",
                OwnedValue::from(u32::from(self.reduced_motion)),
            ),
        ];
        if let Some((r, g, b)) = self.accent
            && let Ok(c) = OwnedValue::try_from(Value::from((r, g, b)))
        {
            v.push(("accent-color", c));
        }
        v
    }
}

static NOW: Mutex<Option<Appearance>> = Mutex::new(None);
static CONN: Mutex<Option<zbus::Connection>> = Mutex::new(None);

fn now() -> Appearance {
    NOW.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .unwrap_or_default()
}

/// The appearance changed (a new wallpaper, accent or setting): apps are told what changed.
pub fn set_appearance(a: Appearance) {
    let before = NOW
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(a);
    if before == Some(a) {
        return;
    }
    let conn = CONN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let Some(conn) = conn else {
        return;
    };
    let old = before.unwrap_or_default().keys();
    let new = a.keys();
    let changed: Vec<(&'static str, OwnedValue)> = new
        .into_iter()
        .filter(|(k, v)| !old.iter().any(|(ok, ov)| ok == k && ov == v))
        .collect();
    tokio::spawn(async move {
        let Ok(emitter) = SignalEmitter::new(&conn, PATH) else {
            return;
        };
        for (k, v) in changed {
            let _ = Settings::setting_changed(&emitter, APPEARANCE, k, Value::from(v)).await;
        }
    });
}

struct Settings;

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl Settings {
    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }

    /// Every key of the namespaces asked for (empty: all; a trailing `*` matches a prefix).
    fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, HashMap<String, OwnedValue>> {
        let wanted = namespaces.iter().all(String::is_empty)
            || namespaces.iter().any(|n| match n.strip_suffix('*') {
                Some(prefix) => APPEARANCE.starts_with(prefix),
                None => n == APPEARANCE,
            });
        let mut out = HashMap::new();
        if wanted {
            out.insert(
                APPEARANCE.to_string(),
                now()
                    .keys()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            );
        }
        out
    }

    fn read(&self, namespace: String, key: String) -> zbus::fdo::Result<OwnedValue> {
        if namespace == APPEARANCE
            && let Some((_, v)) = now().keys().into_iter().find(|(k, _)| *k == key)
        {
            return Ok(v);
        }
        // Not kestrel's: the frontend asks the next backend.
        Err(zbus::fdo::Error::Failed(format!(
            "unknown setting {namespace} {key}"
        )))
    }

    #[zbus(signal)]
    async fn setting_changed(
        emitter: &SignalEmitter<'_>,
        namespace: &str,
        key: &str,
        value: Value<'_>,
    ) -> zbus::Result<()>;
}

/// What apps ask the shell through the portal.
#[derive(Clone, Debug)]
pub enum Event {
    /// A file to open or save as.
    File(filechooser::Request),
    /// Which app should open something ("Open with…").
    OpenWith(appchooser::Event),
}

/// Apps' requests, as they come. Owns the backend's name while it runs.
pub fn stream() -> impl Stream<Item = Event> {
    use futures::SinkExt;
    crate::stream::channel(4, async |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (atx, mut arx) = tokio::sync::mpsc::unbounded_channel();
        let conn = async {
            zbus::connection::Builder::session()?
                .serve_at(PATH, filechooser::Chooser::new(tx))?
                .serve_at(PATH, appchooser::Chooser::new(atx))?
                .serve_at(PATH, Settings)?
                .name(BUS_NAME)?
                .build()
                .await
        }
        .await;
        let conn = match conn {
            Ok(c) => c,
            Err(e) => {
                log::warn!("portal: {e}");
                return;
            }
        };
        *CONN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(conn.clone());
        loop {
            let e = tokio::select! {
                Some(r) = rx.recv() => Event::File(r),
                Some(a) = arx.recv() => Event::OpenWith(a),
                else => break,
            };
            if out.send(e).await.is_err() {
                break;
            }
        }
        *CONN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys as the portal spec has them: color-scheme 1 (dark), the accent as (ddd), no
    /// accent at all when theming apps is off.
    #[test]
    fn appearance_keys_follow_the_spec() {
        let a = Appearance {
            accent: Some((0.2, 0.6, 1.0)),
            ..Default::default()
        };
        let keys = a.keys();
        let get = |k: &str| keys.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("color-scheme"), Some(OwnedValue::from(1u32)));
        assert_eq!(get("contrast"), Some(OwnedValue::from(0u32)));
        let accent: (f64, f64, f64) = get("accent-color").unwrap().try_into().unwrap();
        assert_eq!(accent, (0.2, 0.6, 1.0));
        assert!(
            Appearance::default()
                .keys()
                .iter()
                .all(|(k, _)| *k != "accent-color")
        );
    }

    /// Only kestrel's namespace is answered; others are left to the next backend.
    #[test]
    fn reads_only_its_namespace() {
        let s = Settings;
        assert!(
            s.read_all(vec!["org.gnome.desktop.interface".into()])
                .is_empty()
        );
        assert!(
            s.read_all(vec!["org.freedesktop.*".into()])
                .contains_key(APPEARANCE)
        );
        assert!(s.read_all(vec![]).contains_key(APPEARANCE));
        assert!(s.read(APPEARANCE.into(), "color-scheme".into()).is_ok());
        assert!(
            s.read("org.gnome.desktop.interface".into(), "font-name".into())
                .is_err()
        );
    }
}
