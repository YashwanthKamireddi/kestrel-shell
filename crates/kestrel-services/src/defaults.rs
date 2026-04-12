//! Default apps: which app opens links, mail, folders, text, pictures, video, music and PDFs.
//! Read and set with xdg-mime (the freedesktop standard; it writes ~/.config/mimeapps.list),
//! so every app and toolkit agrees with what Settings shows.

use crate::apps::App;
use std::process::Command;

/// A kind of thing to open, and the types that make it up (the first decides what is shown).
pub struct Kind {
    pub name: &'static str,
    pub types: &'static [&'static str],
    /// Types a candidate must also open to be offered (a browser opens HTML files too: a web
    /// app's shortcut that only claims web links is not a browser).
    pub also: &'static [&'static str],
}

pub const KINDS: &[Kind] = &[
    Kind {
        name: "Web browser",
        types: &[
            "x-scheme-handler/https",
            "x-scheme-handler/http",
            "text/html",
        ],
        also: &["text/html"],
    },
    Kind {
        name: "Mail",
        types: &["x-scheme-handler/mailto"],
        also: &[],
    },
    Kind {
        name: "Files",
        types: &["inode/directory"],
        also: &[],
    },
    Kind {
        name: "Text",
        types: &["text/plain"],
        also: &[],
    },
    Kind {
        name: "Pictures",
        types: &["image/png", "image/jpeg", "image/webp", "image/gif"],
        also: &[],
    },
    Kind {
        name: "Video",
        types: &["video/mp4", "video/x-matroska", "video/webm"],
        also: &[],
    },
    Kind {
        name: "Music",
        types: &["audio/mpeg", "audio/flac", "audio/ogg", "audio/x-wav"],
        also: &[],
    },
    Kind {
        name: "PDF",
        types: &["application/pdf"],
        also: &[],
    },
];

/// The app (desktop file id) that opens `mime` now, if any.
pub fn current(mime: &str) -> Option<String> {
    let out = Command::new("xdg-mime")
        .args(["query", "default", mime])
        .output()
        .ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// The apps that can open the kind's first type, by name.
pub fn candidates<'a>(apps: &'a [App], kind: &Kind) -> Vec<&'a App> {
    let mut v: Vec<&App> = apps
        .iter()
        .filter(|a| {
            let opens = |t: &str| a.mime.split(';').any(|m| m == t);
            opens(kind.types[0]) && kind.also.iter().all(|t| opens(t))
        })
        .collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    v.dedup_by(|a, b| a.id == b.id);
    v
}

/// Makes `app` (a desktop file id) the default for every type of the kind.
pub fn set(kind: &Kind, app: &str) -> Result<(), String> {
    let ok = Command::new("xdg-mime")
        .arg("default")
        .arg(app)
        .args(kind.types)
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err(format!("Could not set the default for {}", kind.name))
    }
}
