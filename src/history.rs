//! The notification history, kept across restarts (ii saves it to a file, Noctalia has its
//! `notification_history_store`): what came in, in `~/.local/state/kestrel/notifications.json`.
//! Kept: the app, its icon, the title and message, a picture that is a file, the time, and
//! kestrel's own actions (open a recording, a screenshot). Another app's buttons are not:
//! after a restart nothing answers them.

use crate::services::notifications::{Image, Notification};
use serde::{Deserialize, Serialize};
use std::time::{Duration, UNIX_EPOCH};

/// Restored entries take ids from here up, apart from the apps' (from 1) and kestrel's own
/// (down from the top), so nothing collides after a restart.
const RESTORED: u32 = 0x4000_0000;

#[derive(Serialize, Deserialize)]
struct Saved {
    app: String,
    app_icon: String,
    summary: String,
    body: String,
    picture: Option<String>,
    actions: Vec<(String, String)>,
    urgency: u8,
    /// Seconds since 1970.
    time: u64,
}

fn file() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state"))
        })?;
    Some(base.join("kestrel").join("notifications.json"))
}

/// The history as last saved, oldest first.
pub fn load() -> Vec<Notification> {
    let Some(saved) = file()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice::<Vec<Saved>>(&b).ok())
    else {
        return Vec::new();
    };
    saved
        .into_iter()
        .enumerate()
        .map(|(k, s)| Notification {
            id: RESTORED + k as u32,
            app: s.app,
            app_icon: s.app_icon,
            summary: s.summary,
            body: s.body,
            actions: s.actions,
            image: s.picture.map(Image::Path),
            urgency: s.urgency,
            timeout: 0,
            time: UNIX_EPOCH + Duration::from_secs(s.time),
            sound: Default::default(),
            replaced: false,
        })
        .collect()
}

/// Saves `list` (written off the caller's thread, replaced whole so a crash never leaves half
/// a file).
pub fn save(list: &[Notification]) {
    let saved: Vec<Saved> = list
        .iter()
        .map(|n| Saved {
            app: n.app.clone(),
            app_icon: n.app_icon.clone(),
            summary: n.summary.clone(),
            body: n.body.clone(),
            picture: match &n.image {
                Some(Image::Path(p)) => Some(p.clone()),
                _ => None,
            },
            actions: n
                .actions
                .iter()
                .filter(|(k, _)| k.starts_with("kestrel-"))
                .cloned()
                .collect(),
            urgency: n.urgency,
            time: n.time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        })
        .collect();
    let (Some(f), Ok(bytes)) = (file(), serde_json::to_vec(&saved)) else {
        return;
    };
    std::thread::spawn(move || {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    });
}

/// A cheap fingerprint of the list, to save only when it changed.
pub fn fingerprint(list: &[Notification]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for n in list {
        n.id.hash(&mut h);
        n.summary.hash(&mut h);
        n.body.hash(&mut h);
    }
    h.finish()
}
