//! The clipboard history: reading entries, their text and thumbnails, putting one back,
//! pasting into the window that had focus, removing one.

use super::*;

/// The history's id of an entry named by its key.
pub(super) fn entry_id(key: &str) -> Option<u64> {
    key.trim().parse().ok()
}

/// A text entry's whole text (up to 16 KiB).
pub(super) async fn clip_text(key: String) -> String {
    tokio::task::spawn_blocking(move || {
        let data = entry_id(&key).and_then(crate::services::clips::data)?;
        let n = data.len().min(16 * 1024);
        Some(String::from_utf8_lossy(&data[..n]).into_owned())
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub(super) fn thumbs_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("kestrel")
        .join("clips3")
}

/// Where an image entry's thumbnail is kept: named by what was copied (its content hash), not
/// by its place in the history. Numbered by id, a thumbnail outlived its entry: after the
/// history was cleared (or another kestrel with the same /run used the numbers), a new entry
/// showed an old picture, even beside text.
fn thumb_path(entry: &crate::services::clips::Entry) -> PathBuf {
    thumbs_dir().join(format!("{:016x}.png", entry.hash))
}

/// A small thumbnail of an image entry (kept in memory-backed /run).
pub(super) async fn thumbnail(key: &str) -> Option<PathBuf> {
    let id = entry_id(key)?;
    let entry = crate::services::clips::list()
        .into_iter()
        .find(|e| e.id == id && e.is_image())?;
    let path = thumb_path(&entry);
    if path.is_file() {
        return Some(path);
    }
    tokio::task::spawn_blocking(move || {
        // The stored file has no extension: the format is read from its first bytes.
        let img = ::image::ImageReader::open(crate::services::clips::file(id))
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?;
        std::fs::create_dir_all(thumbs_dir()).ok()?;
        // Big enough for the preview pane; the rows scale it down.
        img.thumbnail(720, 440).save(&path).ok()?;
        Some(path)
    })
    .await
    .ok()
    .flatten()
}

/// Every entry, newest first. The first time, a history kept by cliphist is brought over (with
/// the entries pinned by their cliphist ids, which the shell's state held until then).
pub async fn clip_list() -> Vec<Clip> {
    tokio::task::spawn_blocking(|| {
        let old_pins = crate::state::get(|s| s.clipboard.pinned.clone());
        crate::services::clips::import_cliphist(&old_pins);
        if !old_pins.is_empty() {
            crate::state::update(|s| s.clipboard.pinned.clear());
        }
        crate::services::clips::list()
            .into_iter()
            .map(|entry| {
                // Only a picture has a thumbnail.
                let cached = entry.is_image().then(|| thumb_path(&entry));
                Clip {
                    key: entry.id.to_string(),
                    preview: entry.preview.clone(),
                    thumb: cached.filter(|p| p.is_file()),
                    entry,
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Puts an entry back on the clipboard (as text, or as the picture it was), and returns once
/// the compositor has it.
pub async fn clip_copy(key: String) {
    let put = tokio::task::spawn_blocking(move || {
        let id = entry_id(&key)?;
        let entry = crate::services::clips::list()
            .into_iter()
            .find(|e| e.id == id)?;
        let data = crate::services::clips::data(id)?;
        Some(if entry.is_image() {
            crate::services::clipboard::copy(&[entry.mime.as_str()], data)
        } else {
            crate::services::clipboard::copy(crate::services::clipboard::TEXT_TYPES, data)
        })
    })
    .await
    .ok()
    .flatten();
    if let Some(done) = put {
        let _ = done.await;
    }
}

/// Presses paste in the window that had focus before the panel (`address`, `class`):
/// Ctrl+Shift+V in a terminal (where Ctrl+V means something else), Ctrl+V anywhere else. Sent
/// to that window by Hyprland, which hands it the keyboard for the shortcut, so the panel
/// needs not be gone first (no waiting).
pub async fn paste_into(address: &str, class: &str) {
    let c = class.to_lowercase();
    let terminal = [
        "kitty",
        "alacritty",
        "foot",
        "wezterm",
        "ghostty",
        "konsole",
        "terminal",
    ]
    .iter()
    .any(|t| c.contains(t));
    let mods = if terminal { "CTRL SHIFT" } else { "CTRL" };
    let window = (!address.is_empty()).then(|| address.to_string());
    crate::services::compositor::run(crate::services::compositor::Action::Shortcut(
        mods.into(),
        "V".into(),
        window,
    ))
    .await;
}

/// Puts text on the clipboard; returns once the compositor has it.
pub async fn copy_text(text: String) {
    let done =
        crate::services::clipboard::copy(crate::services::clipboard::TEXT_TYPES, text.into());
    let _ = done.await;
}

/// Puts text on the clipboard and pastes it into the window that had focus (an emoji picked).
pub async fn type_into(text: String, address: String, class: String) {
    copy_text(text).await;
    if !class.is_empty() {
        paste_into(&address, &class).await;
    }
}

pub(super) async fn clip_delete(key: String) {
    if let Some(id) = entry_id(&key) {
        let _ = tokio::task::spawn_blocking(move || crate::services::clips::remove(id)).await;
    }
}

/// Made-up entries for screen checks: text of several kinds and pictures.
#[cfg(test)]
pub fn sample_clips(n: u64) -> Vec<Clip> {
    use crate::services::clips::Entry;
    (1..=n)
        .map(|id| {
            let image = id % 5 == 0;
            let preview = match id % 3 {
                0 => format!("https://example.com/page/{id}"),
                1 => format!("Copied text number {id}, long enough to be cut at the row's end."),
                _ => "fn main() {\n    println!(\"hi\");\n}".to_string(),
            };
            Clip {
                key: id.to_string(),
                entry: Entry {
                    id,
                    mime: if image {
                        "image/png"
                    } else {
                        "text/plain;charset=utf-8"
                    }
                    .into(),
                    size: image.then_some((640, 480)),
                    bytes: 4096,
                    preview: if image {
                        String::new()
                    } else {
                        preview.clone()
                    },
                    ..Entry::default()
                },
                preview: if image { String::new() } else { preview },
                thumb: None,
            }
        })
        .collect()
}
