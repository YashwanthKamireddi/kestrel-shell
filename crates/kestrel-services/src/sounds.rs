//! Event sounds (a notification arriving), from the user's XDG sound theme, played through
//! PipeWire with the Notification role so the session's policy (ducking, routing) applies.
//!
//! Sounds are looked up as the freedesktop sound theme spec describes: the theme named in
//! `org.gnome.desktop.sound theme-name`, then its `Inherits`, then `freedesktop`; in
//! `$XDG_DATA_HOME/sounds` and each `$XDG_DATA_DIRS/sounds`. A `pw-play` process plays one sound
//! and exits; nothing stays running.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What a notification asks to sound like (the spec's `sound-file`, `sound-name` and
/// `suppress-sound` hints), or the default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Sound {
    #[default]
    Default,
    Named(String),
    File(PathBuf),
    Silent,
}

/// The default sound for a message.
pub const MESSAGE: &str = "message-new-instant";

fn data_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = std::env::var_os("XDG_DATA_HOME") {
        v.push(PathBuf::from(h));
    } else if let Some(h) = std::env::var_os("HOME") {
        v.push(Path::new(&h).join(".local/share"));
    }
    let sys = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    v.extend(sys.split(':').map(PathBuf::from));
    v
}

/// The theme chosen in GNOME's settings (what GTK apps and canberra use too).
fn theme_name() -> String {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.sound", "theme-name"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .trim_matches('\'')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "freedesktop".into())
}

/// A theme's parents, from its index.theme.
fn inherits(theme: &str, dirs: &[PathBuf]) -> Vec<String> {
    dirs.iter()
        .find_map(|d| {
            std::fs::read_to_string(d.join("sounds").join(theme).join("index.theme")).ok()
        })
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("Inherits=").map(str::to_string))
        })
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
        .unwrap_or_default()
}

/// The file for sound `name`, or None if no theme has it.
pub fn lookup(name: &str) -> Option<PathBuf> {
    let dirs = data_dirs();
    let mut themes = vec![theme_name()];
    let mut i = 0;
    while i < themes.len() && themes.len() < 8 {
        for p in inherits(&themes[i].clone(), &dirs) {
            if !themes.contains(&p) {
                themes.push(p);
            }
        }
        i += 1;
    }
    if !themes.iter().any(|t| t == "freedesktop") {
        themes.push("freedesktop".into());
    }
    for theme in &themes {
        for d in &dirs {
            for sub in ["stereo", ""] {
                for ext in ["oga", "ogg", "wav"] {
                    let p = d
                        .join("sounds")
                        .join(theme)
                        .join(sub)
                        .join(format!("{name}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// Plays `sound` (off the caller's thread). Several at once (a burst of notifications) sound
/// once: a sound within 400 ms of the last is skipped.
pub fn play(sound: &Sound, volume: f32) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) {
            return;
        }
        *last = Some(Instant::now());
    }
    let sound = sound.clone();
    std::thread::spawn(move || {
        let file = match sound {
            Sound::Silent => return,
            Sound::File(p) => Some(p),
            Sound::Named(n) => lookup(&n).or_else(|| lookup(MESSAGE)),
            Sound::Default => lookup(MESSAGE),
        };
        let Some(file) = file else { return };
        let _ = std::process::Command::new("pw-play")
            .args(["--media-role", "Notification", "--volume"])
            .arg(format!("{:.2}", volume.clamp(0.0, 1.0)))
            .arg(&file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Plays sound `name` from the theme now and waits until it has played (tests: the speakers,
/// the alert sound). Not held back like notification bursts.
pub async fn play_now(name: &str, volume: f32) {
    let name = name.to_string();
    let Some(file) = tokio::task::spawn_blocking(move || lookup(&name))
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let _ = tokio::process::Command::new("pw-play")
        .args(["--media-role", "Notification", "--volume"])
        .arg(format!("{:.2}", volume.clamp(0.0, 1.0)))
        .arg(&file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
}

/// The speakers, left then right (checks both play, and the balance).
pub async fn speaker_test() {
    play_now("audio-channel-front-left", 1.0).await;
    play_now("audio-channel-front-right", 1.0).await;
}

/// Where the microphone test is kept between recording and playing it back.
fn mic_test_file() -> std::path::PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("kestrel-mic-test.wav")
}

/// Records `secs` seconds from the default microphone (stopped with SIGINT, so the file is
/// finished properly).
pub async fn mic_record(secs: u32) -> Result<(), String> {
    let ok = tokio::process::Command::new("timeout")
        .args([
            "-s",
            "INT",
            &secs.to_string(),
            "pw-record",
            "--channels",
            "1",
        ])
        .arg(mic_test_file())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok();
    if ok && mic_test_file().is_file() {
        Ok(())
    } else {
        Err("The microphone could not be recorded".into())
    }
}

/// Plays the recording back, then deletes it.
pub async fn mic_playback() {
    let f = mic_test_file();
    let _ = tokio::process::Command::new("pw-play")
        .arg(&f)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    let _ = std::fs::remove_file(f);
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_message_sound() {
        // The freedesktop theme is a dependency of the desktop; skip where it is absent.
        if std::path::Path::new("/usr/share/sounds/freedesktop").is_dir() {
            assert!(super::lookup(super::MESSAGE).is_some());
        }
    }
}
