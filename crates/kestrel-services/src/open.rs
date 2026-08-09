//! Opening a file, folder or link with its default app, done directly (no `xdg-open`): the
//! type from the shared MIME database (`mime/globs2`, `subclasses`, `aliases` in each data
//! directory, by the shared-mime-info spec), its default app from mimeapps.list (crate::xdg),
//! that app's command line from its desktop file (crate::defaults::command_for). The caller
//! starts the command through the compositor, so the app is never kestrel's child.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

struct Glob {
    weight: u32,
    mime: String,
    pattern: String,
    case_sensitive: bool,
}

struct Db {
    globs: Vec<Glob>,
    /// type → its parent types (`text/rust` → `text/plain`).
    parents: HashMap<String, Vec<String>>,
    /// alias → its real type.
    aliases: HashMap<String, String>,
}

/// The database, read once (it changes only when packages install new types).
fn db() -> &'static Db {
    static DB: OnceLock<Db> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = Db {
            globs: Vec::new(),
            parents: HashMap::new(),
            aliases: HashMap::new(),
        };
        // The user's directory first: its entries win (read first, kept first below).
        for dir in crate::xdg::data_all() {
            let mime = dir.join("mime");
            if let Ok(t) = std::fs::read_to_string(mime.join("globs2")) {
                for l in t.lines().filter(|l| !l.starts_with('#')) {
                    let mut f = l.split(':');
                    let (Some(w), Some(m), Some(p)) = (f.next(), f.next(), f.next()) else {
                        continue;
                    };
                    if p == "__NOGLOBS__" {
                        continue;
                    }
                    db.globs.push(Glob {
                        weight: w.parse().unwrap_or(50),
                        mime: m.into(),
                        pattern: p.into(),
                        case_sensitive: f.next().is_some_and(|flags| flags.contains("cs")),
                    });
                }
            }
            if let Ok(t) = std::fs::read_to_string(mime.join("subclasses")) {
                for (child, parent) in t.lines().filter_map(|l| l.split_once(' ')) {
                    db.parents
                        .entry(child.into())
                        .or_default()
                        .push(parent.into());
                }
            }
            if let Ok(t) = std::fs::read_to_string(mime.join("aliases")) {
                for (alias, real) in t.lines().filter_map(|l| l.split_once(' ')) {
                    db.aliases
                        .entry(alias.into())
                        .or_insert_with(|| real.into());
                }
            }
        }
        db
    })
}

/// `*`, `?` and `[…]` glob matching, as fnmatch does for file names.
fn glob_match(pattern: &[char], name: &[char]) -> bool {
    match pattern.first() {
        None => name.is_empty(),
        Some('*') => (0..=name.len()).any(|i| glob_match(&pattern[1..], &name[i..])),
        Some('?') => !name.is_empty() && glob_match(&pattern[1..], &name[1..]),
        Some('[') => {
            let Some(end) = pattern
                .iter()
                .skip(1)
                .position(|c| *c == ']')
                .map(|i| i + 1)
            else {
                return name.first() == Some(&'[') && glob_match(&pattern[1..], &name[1..]);
            };
            let Some(c) = name.first() else {
                return false;
            };
            let set = &pattern[1..end];
            let (negate, set) = match set.first() {
                Some('!' | '^') => (true, &set[1..]),
                _ => (false, set),
            };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    hit |= (set[i]..=set[i + 2]).contains(c);
                    i += 3;
                } else {
                    hit |= set[i] == *c;
                    i += 1;
                }
            }
            hit != negate && glob_match(&pattern[end + 1..], &name[1..])
        }
        Some(p) => name.first() == Some(p) && glob_match(&pattern[1..], &name[1..]),
    }
}

/// `name` matches the shell-style `pattern` (`*`, `?`, `[…]`).
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    glob_match(
        &pattern.chars().collect::<Vec<_>>(),
        &name.chars().collect::<Vec<_>>(),
    )
}

/// The type a file name says it is: of the patterns that match, the highest weight, then the
/// longest pattern (shared-mime-info, "Recommended checking order").
pub fn type_by_name(name: &str) -> Option<String> {
    let lower: Vec<char> = name.to_lowercase().chars().collect();
    let exact: Vec<char> = name.chars().collect();
    let mut best: Option<(&Glob, usize)> = None;
    for g in &db().globs {
        let matched = if g.case_sensitive {
            glob_match(&g.pattern.chars().collect::<Vec<_>>(), &exact)
        } else {
            glob_match(
                &g.pattern.to_lowercase().chars().collect::<Vec<_>>(),
                &lower,
            )
        };
        if !matched {
            continue;
        }
        let len = g.pattern.chars().count();
        if best.is_none_or(|(b, blen)| (g.weight, len) > (b.weight, blen)) {
            best = Some((g, len));
        }
    }
    best.map(|(g, _)| g.mime.clone())
}

/// The type of what is at `path`: a folder, the name's type, else plain text when it reads
/// as text (the database's own fallback rule), else unknown bytes.
pub fn type_of(path: &Path) -> String {
    if path.is_dir() {
        return "inode/directory".into();
    }
    if let Some(m) = path
        .file_name()
        .and_then(|n| type_by_name(&n.to_string_lossy()))
    {
        return m;
    }
    let mut head = vec![0u8; 4096];
    let n = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .unwrap_or(0);
    let head = &head[..n];
    // UTF-16 with its byte-order mark is text too (Windows' desktop.ini).
    if head.starts_with(&[0xff, 0xfe]) || head.starts_with(&[0xfe, 0xff]) {
        return "text/plain".into();
    }
    let text = !head.contains(&0)
        && match std::str::from_utf8(head) {
            Ok(_) => true,
            // A character cut at the 4 KiB edge is still text.
            Err(e) => e.error_len().is_none(),
        };
    if text {
        "text/plain".into()
    } else {
        "application/octet-stream".into()
    }
}

/// `mime`, then its parents, breadth first (an app for `text/plain` opens `text/rust`).
fn with_parents(mime: &str) -> Vec<String> {
    let db = db();
    let first = db.aliases.get(mime).cloned().unwrap_or_else(|| mime.into());
    let mut out = vec![first];
    let mut i = 0;
    while i < out.len() {
        for p in db.parents.get(&out[i]).into_iter().flatten() {
            if !out.contains(p) {
                out.push(p.clone());
            }
        }
        i += 1;
    }
    // Every text file is plain text, every file is bytes (the spec's implicit parents).
    for implicit in ["text/plain", "application/octet-stream"] {
        let applies = implicit == "application/octet-stream" || out[0].starts_with("text/");
        if applies && !out.iter().any(|m| m == implicit) {
            out.push(implicit.into());
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A link's scheme (`https`, `mailto`…), if `target` is a link and not a path.
fn scheme(target: &str) -> Option<&str> {
    let (s, _) = target.split_once(':')?;
    let mut c = s.chars();
    (c.next()?.is_ascii_alphabetic()
        && c.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        && s.len() > 1)
        .then_some(s)
}

/// The command line that opens `target` (a path, `~/…`, `file://…` or a link) with its default
/// app, or why there is none.
pub fn command(target: &str) -> Result<String, String> {
    let target = target.trim();
    let path: Option<PathBuf> = if let Some(p) = target.strip_prefix("file://") {
        Some(PathBuf::from(percent_decode(p)))
    } else if let Some(rest) = target.strip_prefix('~') {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        Some(home.join(rest.trim_start_matches('/')))
    } else if target.starts_with('/') || scheme(target).is_none() {
        Some(PathBuf::from(target))
    } else {
        None
    };
    let (types, arg) = match &path {
        Some(p) => (with_parents(&type_of(p)), p.to_string_lossy().into_owned()),
        None => {
            let s = scheme(target).unwrap_or_default().to_lowercase();
            (vec![format!("x-scheme-handler/{s}")], target.to_string())
        }
    };
    // As GIO (what GTK file managers use) picks: a default someone chose for the type or any
    // parent type first (Markdown opens in the text editor), only then an app that merely
    // lists the type (one claiming dozens of types once caught every .md file).
    types
        .iter()
        .find_map(|m| crate::xdg::chosen_app(m))
        .or_else(|| types.iter().find_map(|m| crate::xdg::listing_app(m)))
        .and_then(|id| crate::defaults::command_of(&id, &arg))
        .ok_or_else(|| format!("No app opens {}", types[0]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, n: &str) -> bool {
        glob_match(
            &p.chars().collect::<Vec<_>>(),
            &n.chars().collect::<Vec<_>>(),
        )
    }

    #[test]
    fn globs() {
        assert!(m("*.rs", "main.rs"));
        assert!(!m("*.rs", "main.rsx"));
        assert!(m("Makefile", "Makefile"));
        assert!(m("*.[ch]", "x.h"));
        assert!(!m("*.[!ch]", "x.c"));
        assert!(m("README*", "README.md"));
    }

    #[test]
    fn links_and_paths() {
        assert_eq!(scheme("https://x.org"), Some("https"));
        assert_eq!(scheme("mailto:a@b"), Some("mailto"));
        assert_eq!(scheme("/home/x:y"), None);
        assert_eq!(scheme("C:"), None);
        assert_eq!(percent_decode("/a%20b/%C3%A9"), "/a b/é");
    }
}

#[cfg(test)]
mod live {
    /// This machine's files: the app chosen here and by xdg-mime (run by hand: `--ignored`).
    #[test]
    #[ignore]
    fn same_apps_as_xdg_open() {
        let home = std::env::var("HOME").unwrap();
        let mut files: Vec<String> = Vec::new();
        for dir in [
            "Documents",
            "Downloads",
            "Pictures",
            "Videos",
            "Projects/kestrel",
            "Projects/kestrel/src",
        ] {
            if let Ok(rd) = std::fs::read_dir(format!("{home}/{dir}")) {
                files.extend(
                    rd.flatten()
                        .take(25)
                        .map(|e| e.path().to_string_lossy().into_owned()),
                );
            }
        }
        let (mut same, mut differ) = (0, 0);
        for f in &files {
            let ours = super::type_of(std::path::Path::new(f));
            let theirs = std::process::Command::new("xdg-mime")
                .args(["query", "filetype", f])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            let app = |m: &str| {
                let types = super::with_parents(m);
                types
                    .iter()
                    .find_map(|t| crate::xdg::chosen_app(t))
                    .or_else(|| types.iter().find_map(|t| crate::xdg::listing_app(t)))
            };
            let theirs_app = std::process::Command::new("xdg-mime")
                .args(["query", "default", &theirs])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .ok()
                .filter(|s| !s.is_empty());
            let (a, b) = (app(&ours), theirs_app);
            if a == b || b.is_none() {
                same += 1;
            } else {
                differ += 1;
                println!("{f}: ours {ours} -> {a:?}; xdg {theirs} -> {b:?}");
            }
        }
        println!("{same} same app, {differ} differ, of {}", files.len());
    }
}
