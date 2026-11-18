//! What a query finds: apps, open windows, files, paths, links, emoji, clipboard entries,
//! processes and the web, in the order they are shown.

use super::*;

/// A pasted path that exists: "/home/…/a.pdf", "~/Videos", "file:///…" (a file manager's copy,
/// %20 for spaces), in quotes or not. Enter opens it in its default app, a folder in the file
/// manager (xdg-open). One stat, nothing searched.
pub(super) fn pasted_path(q: &str) -> Option<PathBuf> {
    let q = q.trim().trim_matches(|c| c == '\'' || c == '"');
    let q = q.strip_prefix("file://").unwrap_or(q);
    if !(q.starts_with('/') || q.starts_with("~/") || q == "~") {
        return None;
    }
    let q = if q.contains('%') {
        crate::services::open::percent_decode(q)
    } else {
        q.to_string()
    };
    let p = match q.strip_prefix('~') {
        Some(rest) => PathBuf::from(std::env::var("HOME").ok()?).join(rest.trim_start_matches('/')),
        None => PathBuf::from(q),
    };
    p.exists().then_some(p)
}

pub fn entries(st: &State) -> Vec<Entry<'_>> {
    let q = st.query.trim();
    // A pasted path: just that, to open (before "/" means a search by name).
    if let Some(p) = pasted_path(q) {
        return vec![Entry::File(p)];
    }
    if let Some(expr) = q.strip_prefix('=') {
        return calc(expr).map(|v| vec![Entry::Calc(v)]).unwrap_or_default();
    }
    if let Some(filter) = q.strip_prefix(':') {
        let filter = filter.trim().to_lowercase();
        return st
            .clips
            .iter()
            .flatten()
            .filter(|c| filter.is_empty() || c.preview.to_lowercase().contains(&filter))
            .take(MAX_RESULTS)
            .map(Entry::Clip)
            .collect();
    }
    if let Some(f) = q.strip_prefix('.') {
        let f = f.trim().to_lowercase();
        return EMOJI
            .lines()
            .filter_map(|l| l.split_once(' '))
            .filter(|(_, name)| {
                // "smile" finds "smiling" too: a word also matches without its last letter.
                f.is_empty()
                    || f.split_whitespace().all(|w| {
                        name.contains(w) || (w.len() > 3 && name.contains(&w[..w.len() - 1]))
                    })
            })
            .take(MAX_RESULTS)
            .map(|(e, n)| Entry::Emoji(e, n))
            .collect();
    }
    if let Some(term) = q.strip_prefix('?') {
        let term = term.trim();
        return if term.is_empty() {
            vec![]
        } else {
            vec![Entry::Web(term.to_string())]
        };
    }
    if q.starts_with('/') {
        return if st.files.0 == q {
            st.files.1.iter().cloned().map(Entry::File).collect()
        } else {
            vec![]
        };
    }
    if let Some(name) = q.strip_prefix('!') {
        // The last list read; while a newer read is on its way, narrowed to what is typed now.
        let name = name.trim().to_lowercase();
        return st
            .procs
            .1
            .iter()
            .filter(|(_, n, _)| n.to_lowercase().contains(&name))
            .map(|(pid, n, mem)| Entry::Process(*pid, n.clone(), *mem))
            .collect();
    }
    if let Some(cmd) = q.strip_prefix('>') {
        let cmd = cmd.trim();
        return if cmd.is_empty() {
            vec![]
        } else {
            vec![Entry::Run(cmd.to_string())]
        };
    }
    let scored: Vec<(i32, &apps::App)> = st
        .apps
        .iter()
        .filter_map(|a| {
            let count = *st.counts.get(&a.id).unwrap_or(&0) as i32;
            // Nothing typed: the apps you use most first, then the rest by name, so the list
            // is never bare (a new install showed only the search field).
            if q.is_empty() {
                return Some((count, a));
            }
            let best = [
                // Letters picked out in order only in the name ("vsc" finds Visual Studio
                // Code); the rest must contain the text as typed.
                apps::score(q, &a.name).map(|v| v * 3),
                apps::score_contains(q, &a.generic),
                apps::score_contains(q, &a.keywords),
                apps::score_contains(q, &a.id).map(|v| v / 2),
            ]
            .into_iter()
            .flatten()
            .max()?;
            Some((best + count.min(50) * 4, a))
        })
        .collect();
    // Settings pages, scored like apps: the page's name like an app's, its everyday words like
    // an app's keywords ("blu" finds Bluetooth, "wifi" Network). Two letters at least: one
    // matched too much.
    let mut found: Vec<(i32, Entry<'_>)> = scored
        .into_iter()
        .map(|(v, a)| (v, Entry::App(a)))
        .collect();
    if q.chars().count() >= 2 {
        let mut pages: Vec<(i32, Entry<'_>)> = crate::panels::settings::Page::all()
            .filter_map(|p| {
                let best = [
                    apps::score_contains(q, p.name()).map(|v| v * 3),
                    apps::score_contains(q, p.words()),
                ]
                .into_iter()
                .flatten()
                .max()?;
                Some((best, Entry::Setting(p)))
            })
            .collect();
        pages.sort_by_key(|p| std::cmp::Reverse(p.0));
        found.extend(pages.into_iter().take(3));
        // The system's switches, the same way: "dnd", "night", "wifi" flip them from here.
        let mut switches: Vec<(i32, Entry<'_>)> = Switch::ALL
            .into_iter()
            .filter_map(|w| {
                let best = [
                    apps::score_contains(q, w.name()).map(|v| v * 3),
                    apps::score_contains(q, w.words()),
                ]
                .into_iter()
                .flatten()
                .max()?;
                Some((best, Entry::Switch(w)))
            })
            .collect();
        switches.sort_by_key(|p| std::cmp::Reverse(p.0));
        found.extend(switches.into_iter().take(2));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| title(&a.1).cmp(title(&b.1))));
    let mut out: Vec<Entry<'_>> = found
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, e)| e)
        .collect();
    // Never a dead end: what was typed can always be run (when it names a program) or looked
    // up on the web, at the bottom, so Enter on a query nothing matched still does something.
    if q.chars().count() >= 2 {
        // Not when a result is that program already (btop++ starts btop).
        let word = q.split_whitespace().next().unwrap_or("");
        let exe = st.exe
            && !out.iter().any(|e| {
                matches!(e, Entry::App(a)
                    if a.exec.split_whitespace().next().and_then(|c| c.rsplit('/').next()) == Some(word))
            });
        let room = MAX_RESULTS - usize::from(exe) - 1;
        out.truncate(room);
        if exe {
            out.push(Entry::Run(q.to_string()));
        }
        out.push(Entry::Web(q.to_string()));
    }
    // A bare expression like "2+2" also gets an answer.
    if q.chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '(')
        && let Some(v) = calc(q)
    {
        out.insert(0, Entry::Calc(v));
    }
    out
}

/// An entry's name, for ordering equal scores.
pub(super) fn title<'a>(e: &'a Entry<'_>) -> &'a str {
    match e {
        Entry::App(a) => &a.name,
        Entry::Setting(p) => p.name(),
        Entry::Switch(w) => w.name(),
        _ => "",
    }
}

/// Whether `word` is a program on PATH.
pub(super) fn on_path(word: &str) -> bool {
    !word.is_empty()
        && !word.contains('/')
        && std::env::var_os("PATH").is_some_and(|p| {
            std::env::split_paths(&p).any(|d| {
                std::fs::metadata(d.join(word)).is_ok_and(|m| {
                    m.is_file()
                        && std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0
                })
            })
        })
}

pub(super) fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub(super) async fn find_files(term: String) -> Vec<PathBuf> {
    use crate::services::search::{Options, find};
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let o = Options {
        max_results: 8,
        max_depth: None,
        files_only: true,
        hidden: false,
        one_file_system: false,
    };
    tokio::task::spawn_blocking(move || find(&home, &term, o))
        .await
        .unwrap_or_default()
}

/// This user's processes whose name contains `name`, biggest memory first: (pid, name, KiB),
/// read off the UI thread (every process's status file).
pub(super) async fn find_processes(name: String) -> Vec<(u32, String, u64)> {
    tokio::task::spawn_blocking(move || processes(&name))
        .await
        .unwrap_or_default()
}

/// This user's processes whose name contains `name`, biggest memory first: (pid, name, KiB).
fn processes(name: &str) -> Vec<(u32, String, u64)> {
    let uid = unsafe { libc::getuid() };
    let me = std::process::id();
    let name = name.to_lowercase();
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return out;
    };
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(e.path().join("status")) else {
            continue;
        };
        let field = |k: &str| {
            status
                .lines()
                .find(|l| l.starts_with(k))
                .map(|l| l[k.len()..].trim().to_string())
        };
        let owner = field("Uid:")
            .and_then(|u| u.split_whitespace().next().map(str::to_string))
            .and_then(|u| u.parse::<u32>().ok());
        if owner != Some(uid) {
            continue;
        }
        let comm = field("Name:").unwrap_or_default();
        if !name.is_empty() && !comm.to_lowercase().contains(&name) {
            continue;
        }
        let rss = field("VmRSS:")
            .and_then(|v| v.split_whitespace().next().and_then(|n| n.parse().ok()))
            .unwrap_or(0);
        if rss == 0 {
            continue;
        }
        out.push((pid, comm, rss));
    }
    out.sort_by_key(|p| std::cmp::Reverse(p.2));
    out.truncate(MAX_RESULTS);
    out
}

/// A switch's state in words: what it is on with only while it is on.
pub fn status_text(on: bool, with: Option<&str>) -> String {
    match (on, with.filter(|w| !w.is_empty())) {
        (false, _) => "Off".into(),
        (true, None) => "On".into(),
        (true, Some(w)) => format!("On \u{b7} {w}"),
    }
}

/// A system switch the launcher flips: one of the shell's changes (app::Change), so it acts
/// exactly as the control centre's tile does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Wifi,
    Bluetooth,
    Airplane,
    Dnd,
    NightLight,
    Caffeine,
    GameMode,
}

impl Switch {
    pub const ALL: [Switch; 7] = [
        Switch::Wifi,
        Switch::Bluetooth,
        Switch::Airplane,
        Switch::Dnd,
        Switch::NightLight,
        Switch::Caffeine,
        Switch::GameMode,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Switch::Wifi => "Wi-Fi",
            Switch::Bluetooth => "Bluetooth",
            Switch::Airplane => "Airplane mode",
            Switch::Dnd => "Do not disturb",
            Switch::NightLight => "Night light",
            Switch::Caffeine => "Caffeine",
            Switch::GameMode => "Game mode",
        }
    }

    /// Everyday words it is found by.
    pub fn words(self) -> &'static str {
        match self {
            Switch::Wifi => "wifi wireless internet network",
            Switch::Bluetooth => "bt headphones airpods",
            Switch::Airplane => "flight plane offline radios",
            Switch::Dnd => "dnd quiet silence focus notifications",
            Switch::NightLight => "warm blue light eyes evening",
            Switch::Caffeine => "awake sleep coffee stay on",
            Switch::GameMode => "games gaming performance",
        }
    }

    pub fn icon(self) -> crate::ui::Icon {
        use crate::ui::Icon;
        match self {
            Switch::Wifi => Icon::Wifi,
            Switch::Bluetooth => Icon::Bluetooth,
            Switch::Airplane => Icon::Plane,
            Switch::Dnd => Icon::BellOff,
            Switch::NightLight => Icon::Sunset,
            Switch::Caffeine => Icon::Coffee,
            Switch::GameMode => Icon::Gamepad2,
        }
    }

    /// Whether it is on now, and what it is on with (the network, the device, until when),
    /// read from the shell's live state as the control centre's tile reads it.
    pub fn state(self, cx: &Cx<'_>) -> (bool, Option<String>) {
        match self {
            Switch::Wifi => (cx.net.wifi_enabled, cx.net.ssid.clone()),
            Switch::Bluetooth => {
                let names: Vec<&str> = cx.bt.connected().map(|d| d.name.as_str()).collect();
                let with = match names.len() {
                    0 => None,
                    1 => Some(names[0].to_string()),
                    k => Some(format!("{k} devices")),
                };
                (cx.bt.powered, with)
            }
            Switch::Airplane => (crate::state::get(|s| s.toggles.airplane.is_some()), None),
            Switch::Dnd => (
                cx.dnd,
                crate::state::get(|s| s.toggles.dnd_until)
                    .map(|t| format!("until {}", crate::panels::control::until_text(t))),
            ),
            Switch::NightLight => (
                cx.night_light,
                Some(format!("{} K", cx.config.night_light_temperature)),
            ),
            Switch::Caffeine => (cx.caffeine, None),
            Switch::GameMode => (cx.game_mode, None),
        }
    }

    /// Its row's second line: "On", "Off", or "On · Home" (what it is on with).
    pub fn status(self, cx: &Cx<'_>) -> String {
        let (on, with) = self.state(cx);
        status_text(on, with.as_deref())
    }

    pub fn change(self) -> crate::app::Change {
        use crate::app::Change;
        match self {
            Switch::Wifi => Change::FlipWifi,
            Switch::Bluetooth => Change::FlipBluetooth,
            Switch::Airplane => Change::Airplane,
            Switch::Dnd => Change::Dnd,
            Switch::NightLight => Change::NightLight,
            Switch::Caffeine => Change::Caffeine,
            Switch::GameMode => Change::GameMode,
        }
    }
}
