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

/// The command line that opens `arg` (a path or link; empty: just start the app) with the
/// default app for `mime`, following its desktop file: field codes filled in, and a terminal
/// app (Yazi, ranger) run in the default terminal. None when no default is set.
pub fn command_for(mime: &str, arg: &str) -> Option<String> {
    let id = current(mime)?;
    let path = crate::apps::desktop_file(&id)?;
    let text = std::fs::read_to_string(path).ok()?;
    let mut on = false;
    let (mut exec, mut terminal) = (None, false);
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            on = l == "[Desktop Entry]";
            continue;
        }
        if !on {
            continue;
        }
        if let Some(v) = l.strip_prefix("Exec=") {
            exec.get_or_insert_with(|| v.to_string());
        } else if l == "Terminal=true" {
            terminal = true;
        }
    }
    let quoted = if arg.is_empty() {
        String::new()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    };
    let mut cmd = String::new();
    let exec = exec?;
    let mut chars = exec.chars().peekable();
    let mut used = false;
    while let Some(c) = chars.next() {
        if c != '%' {
            cmd.push(c);
            continue;
        }
        match chars.next() {
            Some('f' | 'F' | 'u' | 'U') => {
                cmd += &quoted;
                used = true;
            }
            Some('%') => cmd.push('%'),
            _ => {}
        }
    }
    if !used && !quoted.is_empty() {
        cmd = format!("{cmd} {quoted}");
    }
    let cmd = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if terminal {
        in_terminal(&cmd, None)
    } else {
        cmd
    })
}

// --- Terminal and editor -------------------------------------------------------------------
//
// The default terminal is the freedesktop way: ~/.config/xdg-terminals.list (desktop file
// ids, first first), which GLib reads for Terminal=true apps and xdg-terminal-exec too.
// kestrel opens its own terminal commands (updates, password) in it as well.

fn terminals_list() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))
        .unwrap_or_default();
    base.join("xdg-terminals.list")
}

/// Installed terminal apps (desktop entries in the TerminalEmulator category, shown ones):
/// (desktop file id, name).
pub fn terminals(apps: &[App]) -> Vec<(String, String)> {
    apps.iter()
        .filter(|a| {
            crate::apps::desktop_file(&a.id)
                .and_then(|p| std::fs::read_to_string(p).ok())
                .is_some_and(|t| {
                    t.lines()
                        .any(|l| l.starts_with("Categories=") && l.contains("TerminalEmulator"))
                        && !t.lines().any(|l| l.trim() == "NoDisplay=true")
                })
        })
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect()
}

/// The terminal in use: the first installed one in xdg-terminals.list, else kitty.
pub fn terminal() -> String {
    std::fs::read_to_string(terminals_list())
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .find(|id| crate::apps::desktop_file(id).is_some())
        .map_or_else(|| "kitty.desktop".into(), str::to_string)
}

/// Makes `id` the first terminal in xdg-terminals.list, keeping the rest of the list.
pub fn set_terminal(id: &str) -> Result<(), String> {
    let path = terminals_list();
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| l.trim() != id)
        .map(str::to_string)
        .collect();
    // After any leading comments, so a comment header stays on top.
    let at = lines
        .iter()
        .take_while(|l| l.trim_start().starts_with('#'))
        .count();
    lines.insert(at, id.to_string());
    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// `cmd` run in the default terminal (with a window title where the terminal takes one).
pub fn in_terminal(cmd: &str, title: Option<&str>) -> String {
    let id = terminal();
    let bin = id
        .trim_end_matches(".desktop")
        .rsplit('.')
        .next()
        .unwrap_or("kitty")
        .to_lowercase();
    let t = |flag: &str| {
        title.map_or_else(String::new, |t| {
            format!("{flag} '{}' ", t.replace('\'', ""))
        })
    };
    match bin.as_str() {
        "kitty" => format!("kitty {}-e {cmd}", t("--title")),
        "alacritty" => format!("alacritty {}-e {cmd}", t("--title")),
        "foot" => format!("foot {}{cmd}", t("--title")),
        "wezterm" => format!("wezterm start -- {cmd}"),
        "ghostty" => format!("ghostty {}-e {cmd}", t("--title")),
        "konsole" => format!("konsole -e {cmd}"),
        "ptyxis" | "console" => format!("ptyxis -- {cmd}"),
        "terminal" | "gnome-terminal" => format!("gnome-terminal -- {cmd}"),
        other => format!("{other} -e {cmd}"),
    }
}

/// Text editors that run in a terminal (what $EDITOR names), those installed.
pub fn editors() -> Vec<&'static str> {
    ["nvim", "vim", "hx", "helix", "micro", "nano", "emacs"]
        .into_iter()
        .filter(|e| {
            std::env::var_os("PATH")
                .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(e).is_file()))
        })
        .collect()
}
