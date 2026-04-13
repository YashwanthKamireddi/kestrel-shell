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
    let _ = tokio::process::Command::new("pkill")
        .args(["-x", "hypridle"])
        .status()
        .await;
    crate::hyprland::dispatch("hl.dsp.exec_cmd(\"hypridle\")".into()).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
