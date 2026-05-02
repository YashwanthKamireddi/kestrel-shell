//! Idle times: when the session locks, the screen turns off and the computer suspends, read
//! from and written into hypridle's own config (~/.config/hypr/hypridle.conf), changing only the
//! three `timeout = …` lines (and the summary comment at the top, so it stays true); then
//! hypridle is started again through Hyprland (so it is not a child of the shell).

use std::path::PathBuf;

/// What happens after a while without input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Lock,
    ScreenOff,
    Suspend,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::Lock, Step::ScreenOff, Step::Suspend];

    /// How its listener is told apart: by what it runs.
    fn matches(self, on_timeout: &str) -> bool {
        match self {
            Step::Lock => on_timeout.contains("lock-session") || on_timeout.contains("hyprlock"),
            Step::ScreenOff => on_timeout.contains("dpms"),
            Step::Suspend => on_timeout.contains("suspend") || on_timeout.contains("hibernate"),
        }
    }
}

fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/hypr/hypridle.conf"))
}

/// The listeners in `text`: (step, line index of its `timeout = …`, seconds).
fn listeners(text: &str) -> Vec<(Step, usize, u64)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("listener") {
            let mut timeout = None;
            let mut action = String::new();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with('}') {
                let l = lines[j].trim();
                if let Some(v) = l.strip_prefix("timeout") {
                    let v = v.trim_start().trim_start_matches('=').trim();
                    timeout = v.parse::<u64>().ok().map(|n| (j, n));
                } else if let Some(v) = l.strip_prefix("on-timeout") {
                    action = v.to_string();
                }
                j += 1;
            }
            if let Some((line, secs)) = timeout
                && let Some(step) = Step::ALL.into_iter().find(|s| s.matches(&action))
            {
                out.push((step, line, secs));
            }
            i = j;
        }
        i += 1;
    }
    out
}

/// Each step's time in seconds, as configured (None: no such listener).
pub fn read() -> Vec<(Step, Option<u64>)> {
    let text = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let found = listeners(&text);
    Step::ALL
        .into_iter()
        .map(|s| {
            (
                s,
                found.iter().find(|(f, _, _)| *f == s).map(|(_, _, n)| *n),
            )
        })
        .collect()
}

/// "30 min", "1 h 30 min".
fn minutes(secs: u64) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// `text` with `step` set to `secs` (and the summary comment brought up to date).
fn with(text: &str, step: Step, secs: u64) -> Option<String> {
    let found = listeners(text);
    let (_, line, _) = *found.iter().find(|(s, _, _)| *s == step)?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let indent: String = lines[line]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    lines[line] = format!("{indent}timeout = {secs}");
    // The summary at the top ("# Idle: lock at 30 min, screen off at 45, suspend at 90.")
    // says the new times.
    let text = lines.join("\n") + "\n";
    let now = listeners(&text);
    let at = |s: Step| {
        now.iter()
            .find(|(f, _, _)| *f == s)
            .map(|(_, _, n)| minutes(*n))
    };
    if let Some(first) = lines.iter().position(|l| l.starts_with("# Idle:")) {
        let mut parts = Vec::new();
        if let Some(t) = at(Step::Lock) {
            parts.push(format!("lock at {t}"));
        }
        if let Some(t) = at(Step::ScreenOff) {
            parts.push(format!("screen off at {t}"));
        }
        if let Some(t) = at(Step::Suspend) {
            parts.push(format!("suspend at {t}"));
        }
        let tail = lines[first]
            .split_once(". ")
            .map(|(_, rest)| format!(" {rest}"))
            .unwrap_or_default();
        lines[first] = format!("# Idle: {}.{tail}", parts.join(", "));
    }
    Some(lines.join("\n") + "\n")
}

/// Sets `step` to `secs` and restarts hypridle so it takes effect.
pub async fn set(step: Step, secs: u64) -> Result<(), String> {
    let p = path().ok_or("no home folder")?;
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    // Already that: the file is left exactly as it is.
    if listeners(&text)
        .iter()
        .any(|(s, _, n)| *s == step && *n == secs)
    {
        return Ok(());
    }
    let new = with(&text, step, secs).ok_or("that idle step is not set up in hypridle.conf")?;
    std::fs::write(&p, new).map_err(|e| e.to_string())?;
    // hypridle reads its config once: started again by Hyprland (as at login).
    restart_hypridle().await;
    Ok(())
}

/// hypridle reads its config once: this session's is stopped and started again by Hyprland
/// (as at login). Only the hypridle of this Hyprland session (another session's, a nested
/// test compositor's, are left alone: `pkill -x hypridle` stopped every one).
async fn restart_hypridle() {
    let ours = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() else {
                continue;
            };
            let comm = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
            if comm.trim() != "hypridle" {
                continue;
            }
            let env = std::fs::read(e.path().join("environ")).unwrap_or_default();
            let same = env
                .split(|b| *b == 0)
                .any(|kv| kv == format!("HYPRLAND_INSTANCE_SIGNATURE={ours}").as_bytes());
            if same {
                let _ = tokio::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status()
                    .await;
            }
        }
    }
    crate::hyprland::dispatch("hl.dsp.exec_cmd(\"hypridle\")".into()).await;
}

/// Whether the session locks before the laptop sleeps (hypridle's `before_sleep_cmd`).
pub fn lock_on_sleep() -> Option<bool> {
    let text = std::fs::read_to_string(path()?).ok()?;
    Some(locks_on_sleep(&text))
}

fn locks_on_sleep(text: &str) -> bool {
    text.lines()
        .any(|l| l.trim_start().starts_with("before_sleep_cmd") && l.contains("lock-session"))
}

const LOCK_LINE: &str = "    before_sleep_cmd = loginctl lock-session";
const OFF_MARK: &str = "# kestrel: lock on sleep off: ";

/// The config with locking before sleep on or off. Off comments the line out and sets
/// inhibit_sleep to 0 (3 holds sleep until a lock that would never come); on puts both back
/// as they were.
fn with_lock_on_sleep(text: &str, on: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut have_lock = false;
    for l in text.lines() {
        let t = l.trim_start();
        if on {
            if let Some(orig) = t.strip_prefix(OFF_MARK) {
                out.push(format!("    {orig}"));
                if orig.starts_with("before_sleep_cmd") {
                    have_lock = true;
                }
                continue;
            }
            if t.starts_with("inhibit_sleep = 0") && l.contains("kestrel") {
                continue;
            }
            if t.starts_with("before_sleep_cmd") {
                have_lock = true;
            }
            out.push(l.to_string());
        } else if (t.starts_with("before_sleep_cmd") && l.contains("lock-session"))
            || t.starts_with("inhibit_sleep")
        {
            out.push(format!("    {OFF_MARK}{t}"));
            if t.starts_with("inhibit_sleep") {
                out.push("    inhibit_sleep = 0 # kestrel: nothing to wait for".into());
            }
        } else {
            out.push(l.to_string());
        }
    }
    // On, and the config never had the line: added inside `general`.
    if on
        && !have_lock
        && let Some(i) = out
            .iter()
            .position(|l| l.trim_start().starts_with("general"))
    {
        out.insert(i + 1, LOCK_LINE.into());
    }
    out.join("\n") + "\n"
}

/// Locks before sleeping, or not; hypridle started again to read it.
pub async fn set_lock_on_sleep(on: bool) -> Result<(), String> {
    let p = path().ok_or("no home folder")?;
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    if locks_on_sleep(&text) == on {
        return Ok(());
    }
    std::fs::write(&p, with_lock_on_sleep(&text, on)).map_err(|e| e.to_string())?;
    restart_hypridle().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_on_sleep_off_and_back() {
        let conf = "general {\n    lock_cmd = pidof hyprlock || hyprlock\n    before_sleep_cmd = loginctl lock-session\n    # 3: hold sleep until locked\n    inhibit_sleep = 3\n}\n";
        assert!(locks_on_sleep(conf));
        let off = with_lock_on_sleep(conf, false);
        assert!(!locks_on_sleep(&off));
        assert!(off.contains("inhibit_sleep = 0"));
        let back = with_lock_on_sleep(&off, true);
        assert_eq!(back, conf);
    }

    const CONF: &str = "# Idle: lock at 30 min, screen off at 45, suspend at 90. hyprlock is the lock screen.\n\ngeneral {\n    lock_cmd = pidof hyprlock || hyprlock\n}\n\nlistener {\n    timeout = 1800\n    on-timeout = loginctl lock-session\n}\n\nlistener {\n    timeout = 2700\n    on-timeout = hyprctl dispatch 'hl.dsp.dpms({ action = \"disable\" })'\n    on-resume = hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'\n}\n\nlistener {\n    timeout = 5400\n    on-timeout = systemctl suspend\n}\n";

    #[test]
    fn reads_the_three_steps() {
        let l = listeners(CONF);
        assert_eq!(
            l.iter().map(|(s, _, n)| (*s, *n)).collect::<Vec<_>>(),
            vec![
                (Step::Lock, 1800),
                (Step::ScreenOff, 2700),
                (Step::Suspend, 5400)
            ]
        );
    }

    #[test]
    fn changes_one_line_and_the_summary() {
        let out = with(CONF, Step::Lock, 600).unwrap();
        assert!(out.contains("    timeout = 600\n"));
        assert!(out.contains("    timeout = 2700\n"));
        assert!(out.starts_with(
            "# Idle: lock at 10 min, screen off at 45 min, suspend at 1 h 30 min. hyprlock is the lock screen.\n"
        ));
        // Everything else is untouched.
        assert_eq!(out.lines().count(), CONF.lines().count(),);
        assert!(out.contains("lock_cmd = pidof hyprlock || hyprlock"));
    }
}
