//! Clipboard history, kept by kestrel itself: every copy the clipboard watcher sees
//! (crate::clipboard) is recorded here as it happens (no `wl-paste --watch`, `cliphist` or
//! `wl-copy` processes). Kept until deleted by hand (the clipboard panel, Settings), as this
//! history was set up under cliphist (`max-items 1000000`); a copy of something already kept
//! moves it to the top; nothing a password manager marks secret is kept; text and images.
//!
//! Each entry's content is a file in `$XDG_DATA_HOME/kestrel/clipboard`, with `index.json`
//! listing them newest first; both are written whole and renamed in. A history kept by
//! cliphist before is brought over once, the first time.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;

/// A ceiling, not a policy: entries are kept until deleted by hand (the same ceiling the
/// history had under cliphist).
pub const MAX: usize = 1_000_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    /// The type it was copied as (`text/plain;charset=utf-8`, `image/png`…).
    pub mime: String,
    /// When it was (last) copied, seconds since 1970.
    pub time: u64,
    /// Text: its start, whitespace collapsed. Images: empty.
    pub preview: String,
    /// Images: width and height.
    #[serde(default)]
    pub size: Option<(u32, u32)>,
    pub bytes: u64,
    /// The content's digest (how a copy of something already kept is found).
    pub hash: u64,
}

impl Entry {
    pub fn is_image(&self) -> bool {
        self.mime.starts_with("image/")
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Index {
    next: u64,
    /// Newest first.
    entries: Vec<Entry>,
    /// Kept on top and never dropped or cleared, in the order they were pinned.
    #[serde(default)]
    pinned: Vec<u64>,
    /// A history kept by cliphist was brought over (or there was none).
    #[serde(default)]
    imported: bool,
}

static STORE: Mutex<Option<Index>> = Mutex::new(None);

fn dir() -> PathBuf {
    crate::xdg::data_home().join("kestrel").join("clipboard")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn write_whole(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("kestrel-new");
    std::fs::write(&tmp, data)?;
    std::fs::rename(tmp, path)
}

fn save(ix: &Index) {
    let r = std::fs::create_dir_all(dir()).and_then(|()| {
        let json = serde_json::to_vec(ix).map_err(std::io::Error::other)?;
        write_whole(&dir().join("index.json"), &json)
    });
    if let Err(e) = r {
        log::warn!("clipboard history: {e}");
    }
}

/// Runs `f` on the history, loaded (or brought over from cliphist) on first use.
fn with<T>(f: impl FnOnce(&mut Index) -> T) -> T {
    let mut g = STORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let ix = g.get_or_insert_with(|| match std::fs::read(dir().join("index.json")) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => Index::default(),
    });
    f(ix)
}

fn digest(mime: &str, data: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // Text copied as one text type or another is the same text.
    mime.starts_with("image/").hash(&mut h);
    data.hash(&mut h);
    h.finish()
}

fn preview_of(data: &[u8]) -> String {
    String::from_utf8_lossy(&data[..data.len().min(4096)])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

/// Records a copy (the watcher calls this as it reads one). Returns the entry.
pub fn add(mime: &str, data: &[u8], size: Option<(u32, u32)>) -> Option<Entry> {
    let is_image = mime.starts_with("image/");
    let preview = if is_image {
        String::new()
    } else {
        preview_of(data)
    };
    if data.is_empty() || (!is_image && preview.is_empty()) {
        return None;
    }
    let hash = digest(mime, data);
    with(|ix| {
        // Copied again: to the top, as it is.
        if let Some(i) = ix.entries.iter().position(|e| e.hash == hash) {
            let mut e = ix.entries.remove(i);
            e.time = now();
            ix.entries.insert(0, e.clone());
            save(ix);
            return Some(e);
        }
        let id = ix.next.max(1);
        ix.next = id + 1;
        let file = dir().join(id.to_string());
        if let Err(e) = std::fs::create_dir_all(dir()).and_then(|()| write_whole(&file, data)) {
            log::warn!("clipboard history: {e}");
            return None;
        }
        let e = Entry {
            id,
            mime: mime.to_string(),
            time: now(),
            preview,
            size,
            bytes: data.len() as u64,
            hash,
        };
        ix.entries.insert(0, e.clone());
        // Room made from the oldest that are not pinned.
        while ix.entries.len() > MAX {
            let pinned = &ix.pinned;
            let Some(i) = ix.entries.iter().rposition(|e| !pinned.contains(&e.id)) else {
                break;
            };
            let gone = ix.entries.remove(i);
            let _ = std::fs::remove_file(dir().join(gone.id.to_string()));
        }
        save(ix);
        Some(e)
    })
}

/// Every entry, newest first.
pub fn list() -> Vec<Entry> {
    with(|ix| ix.entries.clone())
}

/// An entry's content.
pub fn data(id: u64) -> Option<Vec<u8>> {
    std::fs::read(dir().join(id.to_string())).ok()
}

/// The file holding an entry's content (an image, for drawing it).
pub fn file(id: u64) -> PathBuf {
    dir().join(id.to_string())
}

/// The pinned entries, in the order they were pinned.
pub fn pinned() -> Vec<u64> {
    with(|ix| ix.pinned.clone())
}

pub fn set_pinned(id: u64, on: bool) {
    with(|ix| {
        ix.pinned.retain(|p| *p != id);
        if on && ix.entries.iter().any(|e| e.id == id) {
            ix.pinned.push(id);
        }
        save(ix);
    });
}

pub fn remove(id: u64) {
    with(|ix| {
        ix.entries.retain(|e| e.id != id);
        ix.pinned.retain(|p| *p != id);
        let _ = std::fs::remove_file(dir().join(id.to_string()));
        save(ix);
    });
}

/// Forgets everything but the pinned entries.
pub fn clear() {
    with(|ix| {
        let keep = ix.pinned.clone();
        for e in ix.entries.iter().filter(|e| !keep.contains(&e.id)) {
            let _ = std::fs::remove_file(dir().join(e.id.to_string()));
        }
        ix.entries.retain(|e| keep.contains(&e.id));
        save(ix);
    });
}

/// An entry read from cliphist: its cliphist id, type, content and picture size.
struct Brought {
    old: String,
    mime: String,
    data: Vec<u8>,
    size: Option<(u32, u32)>,
}

/// Brings a history kept by cliphist over, once, with the entries pinned by their cliphist ids
/// (`pins`, kept by the shell until now). Its entries go below anything copied since kestrel
/// started keeping the history (they are older). Run off the UI thread: it reads every entry.
pub fn import_cliphist(pins: &[String]) {
    if with(|ix| ix.imported) {
        return;
    }
    let lines: Vec<String> = match std::process::Command::new("cliphist").arg("list").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect(),
        // No cliphist (or no history in it): nothing to bring.
        _ => Vec::new(),
    };
    // Newest first, as cliphist lists them; read outside the lock (copies keep being kept).
    let mut brought: Vec<Brought> = Vec::new();
    for line in &lines {
        let Some((old, preview)) = line.split_once('\t') else {
            continue;
        };
        let Some(data) = cliphist_decode(line) else {
            continue;
        };
        // "[[ binary data 248 KiB png 562x850 ]]"
        let (mime, size) = if preview.starts_with("[[ binary data") {
            let words: Vec<&str> = preview.split_whitespace().collect();
            let kind = words.get(5).copied().unwrap_or("png");
            let size = words.get(6).and_then(|d| {
                let (w, h) = d.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            });
            (format!("image/{kind}"), size)
        } else {
            ("text/plain;charset=utf-8".to_string(), None)
        };
        brought.push(Brought {
            old: old.trim().to_string(),
            mime,
            data,
            size,
        });
    }
    let n = brought.len();
    with(|ix| {
        if ix.imported {
            return;
        }
        let mut map: HashMap<String, u64> = HashMap::new();
        let _ = std::fs::create_dir_all(dir());
        for Brought {
            old,
            mime,
            data,
            size,
        } in brought
        {
            let hash = digest(&mime, &data);
            // Already kept (copied again since): that entry stands for it.
            if let Some(e) = ix.entries.iter().find(|e| e.hash == hash) {
                map.insert(old, e.id);
                continue;
            }
            let id = ix.next.max(1);
            ix.next = id + 1;
            if write_whole(&dir().join(id.to_string()), &data).is_err() {
                continue;
            }
            let is_image = mime.starts_with("image/");
            ix.entries.push(Entry {
                id,
                preview: if is_image {
                    String::new()
                } else {
                    preview_of(&data)
                },
                mime,
                time: now(),
                size,
                bytes: data.len() as u64,
                hash,
            });
            map.insert(old, id);
        }
        for p in pins {
            if let Some(id) = map.get(p.trim())
                && !ix.pinned.contains(id)
            {
                ix.pinned.push(*id);
            }
        }
        ix.imported = true;
        save(ix);
    });
    if n > 0 {
        log::info!("clipboard history: {n} entries brought over from cliphist");
    }
}

fn cliphist_decode(line: &str) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut c = std::process::Command::new("cliphist")
        .arg("decode")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    c.stdin.take()?.write_all(line.as_bytes()).ok()?;
    let out = c.wait_with_output().ok()?;
    (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_collapse_whitespace() {
        assert_eq!(preview_of(b"  a\n\tb  c "), "a b c");
        assert_eq!(preview_of(b" \n "), "");
    }

    #[test]
    fn same_text_in_another_type_is_the_same_copy() {
        assert_eq!(
            digest("text/plain", b"x"),
            digest("text/plain;charset=utf-8", b"x")
        );
        assert_ne!(digest("text/plain", b"x"), digest("image/png", b"x"));
    }
}
