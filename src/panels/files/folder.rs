//! Reading folders: their items, sorted and searched, the app's file types applied.

use super::*;

pub(super) fn last_folder() -> Option<PathBuf> {
    crate::state::get(|s| s.files.last_folder.clone()).map(PathBuf::from)
}

pub(super) fn item(path: PathBuf, rel: Option<String>) -> Option<Item> {
    let meta = std::fs::metadata(&path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let mime = (!meta.is_dir())
        .then(|| crate::services::open::type_by_name(&name))
        .flatten();
    Some(Item {
        name,
        mime,
        dir: meta.is_dir(),
        size: meta.len(),
        modified: meta.modified().ok(),
        path,
        rel,
    })
}

/// Folders before files when sorted by name (as every file manager does); by date or size
/// they mix in, so the newest download is at the top even with folders around it. Names
/// compare like people count: "Shot 2" before "Shot 10".
pub(super) fn sort(items: &mut [Item], s: Sort) {
    use std::cmp::Ordering;
    items.sort_by(|a, b| {
        let by_name = || natural(&a.name, &b.name);
        let o = match s.by {
            By::Name => {
                let o = by_name();
                return b.dir.cmp(&a.dir).then(if s.desc { o.reverse() } else { o });
            }
            By::Modified => a.modified.cmp(&b.modified),
            // Folders have no size of their own: after the files.
            By::Size => match (a.dir, b.dir) {
                (false, true) => return Ordering::Less,
                (true, false) => return Ordering::Greater,
                (true, true) => return by_name(),
                _ => a.size.cmp(&b.size),
            },
        };
        (if s.desc { o.reverse() } else { o }).then_with(by_name)
    });
}

/// Case-insensitive, with runs of digits compared as numbers.
pub(super) fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.next_if(char::is_ascii_digit) {
                    n.push(c);
                }
                let mut m = String::new();
                while let Some(c) = y.next_if(char::is_ascii_digit) {
                    m.push(c);
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

/// Reads `dir` and shows it.
pub(super) fn load(st: &mut State, dir: PathBuf) {
    let mut listing: Vec<Item> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| st.hidden || !e.file_name().to_string_lossy().starts_with('.'))
                .filter_map(|e| item(e.path(), None))
                .collect()
        })
        .unwrap_or_default();
    st.sort = sort_for(&dir);
    sort(&mut listing, st.sort);
    st.dir = dir;
    st.items = listing.clone();
    st.listing = listing;
    st.mode = Mode::Folder;
    st.query.clear();
    new_list(st);
    st.scroll = 0.0;
    st.click = None;
    st.new_folder = None;
    st.searching = false;
}

/// A different list is shown (a folder, recent files, a search, another filter): nothing in it
/// is selected or picked yet, and it starts at its top. The one place a list starts, so no
/// choice made in one list is carried into another (a file picked in one folder was sent
/// with the next one's).
pub(super) fn new_list(st: &mut State) {
    st.selected = None;
    st.picked.clear();
    st.anchor = None;
    st.top = 0;
    st.pos = 0.0;
}

/// Goes to `dir`, remembering where it came from (Back).
pub(super) fn go(st: &mut State, dir: PathBuf) {
    if dir != st.dir && !st.dir.as_os_str().is_empty() {
        st.back.push(st.dir.clone());
        st.forward.clear();
    }
    load(st, dir);
}

/// `~/x` and `/x` as a path.
pub(super) fn expand(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) => home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(p),
    }
}

/// A typed path's folder and what its last part starts with.
pub(super) fn split_typed(q: &str) -> (PathBuf, String) {
    let full = expand(q.trim());
    if q.ends_with('/') {
        return (full, String::new());
    }
    let prefix = full
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = full
        .parent()
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
    (dir, prefix)
}

/// The entries of a typed path's folder that start like it.
pub(super) fn completions(st: &State, q: &str) -> Vec<Item> {
    let (dir, prefix) = split_typed(q);
    let low = prefix.to_lowercase();
    let mut v: Vec<Item> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    n.starts_with(&low)
                        && (st.hidden || low.starts_with('.') || !n.starts_with('.'))
                })
                .filter_map(|e| item(e.path(), None))
                .collect()
        })
        .unwrap_or_default();
    sort(&mut v, sort_for(&dir));
    v.truncate(200);
    v
}

/// Files opened lately: the desktop's recently-used list, newest first, those still there.
pub(super) fn recent() -> Vec<Item> {
    let p = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join("recently-used.xbel");
    let Ok(text) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Item> = text
        .split("<bookmark ")
        .skip(1)
        .filter_map(|b| {
            let href = b.split("href=\"").nth(1)?.split('"').next()?;
            let path = PathBuf::from(decode(href.strip_prefix("file://")?));
            if !seen.insert(path.clone()) || !path.is_file() {
                return None;
            }
            let rel = path
                .parent()
                .map(|d| d.strip_prefix(home()).unwrap_or(d).display().to_string());
            item(path, rel)
        })
        .collect();
    out.sort_by_key(|i| std::cmp::Reverse(i.modified));
    out.truncate(80);
    out
}

/// `%20` and friends back to bytes.
pub(super) fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Search below `dir` (services::search: fast, respects .gitignore, skips hidden unless asked). It stays on
/// `dir`'s drive: from `/` it searched /dev, /proc and /sys and listed device nodes.
pub(super) async fn search_below(dir: PathBuf, q: String, hidden: bool) -> Vec<PathBuf> {
    use crate::services::search::{Options, find};
    let o = Options {
        max_results: 120,
        max_depth: Some(8),
        files_only: false,
        hidden,
        one_file_system: true,
    };
    tokio::task::spawn_blocking(move || find(&dir, &q, o))
        .await
        .unwrap_or_default()
}

/// A shell pattern (`*`, `?`, and `[…]` classes with ranges and `!`/`^`), any case. Classes
/// matter: Chromium sends every extension as one (`*.[jJ][pP][gG]`), and without them its
/// "*.jpg" filter showed a folder of pictures as empty.
pub(super) fn glob(pattern: &str, name: &str) -> bool {
    /// The class at the start of `p` (just past its `[`): whether `c` is in it, and its
    /// length up to and including the `]`. `None`: no closing `]`, so `[` is a plain character.
    fn class(p: &[char], c: char) -> Option<(bool, usize)> {
        let not = matches!(p.first(), Some('!' | '^'));
        let mut i = usize::from(not);
        let mut hit = false;
        let mut first = true;
        loop {
            let a = *p.get(i)?;
            if a == ']' && !first {
                return Some((hit != not, i + 1));
            }
            first = false;
            if p.get(i + 1) == Some(&'-') && p.get(i + 2).is_some_and(|&b| b != ']') {
                let b = p[i + 2];
                let lc = c.to_ascii_lowercase();
                hit |= (a.to_ascii_lowercase()..=b.to_ascii_lowercase()).contains(&lc)
                    || (a.to_ascii_uppercase()..=b.to_ascii_uppercase())
                        .contains(&c.to_ascii_uppercase());
                i += 3;
            } else {
                hit |= a.eq_ignore_ascii_case(&c);
                i += 1;
            }
        }
    }
    fn m(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some('['), Some(&b)) => match class(&p[1..], b) {
                Some((hit, len)) => hit && m(&p[1 + len..], &n[1..]),
                None => b == '[' && m(&p[1..], &n[1..]),
            },
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    m(&p, &n)
}

/// Whether the app's current filter lets this item through (folders always).
pub(super) fn passes(st: &State, it: &Item) -> bool {
    if it.dir {
        return true;
    }
    // Choosing a folder: its files show (greyed, not choosable), so you see what it holds.
    if st.req.as_ref().is_some_and(|r| r.directory) {
        return true;
    }
    // Past the app's own filters: "All files".
    let Some(f) = st.req.as_ref().and_then(|r| r.filters.get(st.filter)) else {
        return true;
    };
    // As GTK's chooser: a name pattern (any case: "*.PNG" is a .png), or the file's type is
    // one of the filter's (its parents and families count: a .rs file is text/plain).
    let lower = it.name.to_lowercase();
    f.globs.iter().any(|g| glob(&g.to_lowercase(), &lower))
        || it
            .mime
            .as_deref()
            .is_some_and(|t| f.mimes.iter().any(|m| crate::services::open::is_a(t, m)))
}

/// Whether one of the app's filters already lets everything through (no "All files" added).
pub(super) fn has_catch_all(r: &Request) -> bool {
    r.filters.iter().any(|f| {
        f.globs.iter().any(|g| g == "*")
            || f.mimes
                .iter()
                .any(|m| m == "*/*" || m == "application/octet-stream")
    })
}

/// The file type choices: the app's, then "All files" unless one of them already is.
pub(super) fn filter_names(r: &Request) -> Vec<String> {
    let mut v: Vec<String> = r.filters.iter().map(|f| f.name.clone()).collect();
    if !v.is_empty() && !has_catch_all(r) {
        v.push("All files".into());
    }
    v
}

pub(super) fn shown(st: &State) -> Vec<usize> {
    st.items
        .iter()
        .enumerate()
        .filter(|(_, it)| passes(st, it))
        .map(|(i, _)| i)
        .collect()
}
