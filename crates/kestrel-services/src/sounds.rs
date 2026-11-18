//! Event sounds (a notification arriving), from the user's XDG sound theme, played through
//! PipeWire with the Notification role so the session's policy (ducking, routing) applies.
//!
//! Sounds are looked up as the freedesktop sound theme spec describes: the theme named in
//! `org.gnome.desktop.sound theme-name`, then its `Inherits`, then `freedesktop`; in
//! `$XDG_DATA_HOME/sounds` and each `$XDG_DATA_DIRS/sounds`.
//!
//! Played as libcanberra plays them: each file is decoded once (Ogg Vorbis or WAV), uploaded
//! into the sound server's sample cache by the audio service's connection, and from then on
//! played by name (no process, no file read, nothing decoded again).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// The theme chosen in GNOME's settings (what GTK apps and canberra use too).
fn theme_name() -> String {
    crate::gsettings::get_string("org.gnome.desktop.sound", "theme-name")
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
    let dirs = crate::xdg::data_all();
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
        if let Some(pcm) = decoded(&file) {
            crate::audio::command(crate::audio::Command::Play(pcm, volume));
        }
    });
}

/// Decoded sound: interleaved 16-bit samples.
pub struct Pcm {
    /// The file it came from (also its name in the server's sample cache).
    pub file: PathBuf,
    pub rate: u32,
    pub channels: u8,
    pub samples: Vec<i16>,
    /// Kept in the server's cache for the session (a theme sound); a recording is not.
    pub keep: bool,
}

impl Pcm {
    pub fn duration(&self) -> Duration {
        let frames = self.samples.len() / usize::from(self.channels.max(1));
        Duration::from_secs_f64(frames as f64 / f64::from(self.rate.max(1)))
    }

    /// Little-endian bytes, as the server takes them.
    pub fn bytes(&self) -> Vec<u8> {
        self.samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }
}

impl std::fmt::Debug for Pcm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pcm({}, {:?})", self.file.display(), self.duration())
    }
}

/// `file` decoded, kept for the session (a sound theme is a few small files).
fn decoded(file: &Path) -> Option<Arc<Pcm>> {
    static CACHE: Mutex<Option<HashMap<PathBuf, Arc<Pcm>>>> = Mutex::new(None);
    if let Some(p) = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_default()
        .get(file)
    {
        return Some(p.clone());
    }
    let pcm = match decode(file) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            log::warn!("sound {}: {e}", file.display());
            return None;
        }
    };
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_default()
        .insert(file.to_path_buf(), pcm.clone());
    Some(pcm)
}

fn decode(file: &Path) -> Result<Pcm, String> {
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let (rate, channels, samples) = if bytes.starts_with(b"OggS") {
        ogg(&bytes)?
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        wav(&bytes)?
    } else {
        return Err("not Ogg Vorbis or WAV".into());
    };
    if samples.is_empty() || channels == 0 || rate == 0 {
        return Err("no sound in it".into());
    }
    Ok(Pcm {
        file: file.to_path_buf(),
        rate,
        channels,
        samples,
        keep: true,
    })
}

fn ogg(bytes: &[u8]) -> Result<(u32, u8, Vec<i16>), String> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| e.to_string())?;
    let (rate, channels) = (r.ident_hdr.audio_sample_rate, r.ident_hdr.audio_channels);
    let mut samples = Vec::new();
    while let Some(p) = r.read_dec_packet_itl().map_err(|e| e.to_string())? {
        samples.extend(p);
    }
    Ok((rate, channels, samples))
}

/// A RIFF WAVE file's PCM (8, 16, 24 or 32-bit integers, or 32-bit floats) as 16-bit.
fn wav(b: &[u8]) -> Result<(u32, u8, Vec<i16>), String> {
    let u16_at = |i: usize| b.get(i..i + 2).map(|x| u16::from_le_bytes([x[0], x[1]]));
    let u32_at = |i: usize| {
        b.get(i..i + 4)
            .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
    };
    let (mut fmt, mut data) = (None, None);
    let mut i = 12;
    while let (Some(id), Some(len)) = (b.get(i..i + 4), u32_at(i + 4)) {
        let body = i + 8;
        let end = body.saturating_add(len as usize).min(b.len());
        match id {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body..end),
            _ => {}
        }
        // Chunks are padded to an even length.
        i = end + (len as usize & 1);
    }
    let f = fmt.ok_or("no format")?;
    let data = &b[data.ok_or("no data")?];
    let (kind, channels, rate, bits) = (
        u16_at(f).ok_or("bad format")?,
        u16_at(f + 2).ok_or("bad format")?,
        u32_at(f + 4).ok_or("bad format")?,
        u16_at(f + 14).ok_or("bad format")?,
    );
    // 0xFFFE (extensible) carries the real kind in its sub-format; 1 = integer, 3 = float.
    let kind = if kind == 0xfffe {
        u16_at(f + 24).ok_or("bad format")?
    } else {
        kind
    };
    let samples: Vec<i16> = match (kind, bits) {
        (1, 8) => data.iter().map(|v| (i16::from(*v) - 128) << 8).collect(),
        (1, 16) => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| i16::from_le_bytes(*x))
            .collect(),
        (1, 24) => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|x| i16::from_le_bytes([x[1], x[2]]))
            .collect(),
        (1, 32) => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| i16::from_le_bytes([x[2], x[3]]))
            .collect(),
        (3, 32) => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| (f32::from_le_bytes(*x).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
            .collect(),
        _ => return Err(format!("unsupported WAV ({kind}, {bits} bits)")),
    };
    let channels = u8::try_from(channels).map_err(|_| "too many channels")?;
    Ok((rate, channels, samples))
}

/// Plays sound `name` from the theme now and waits until it has played (tests: the speakers,
/// the alert sound). Not held back like notification bursts.
pub async fn play_now(name: &str, volume: f32) {
    let name = name.to_string();
    let Some(pcm) = tokio::task::spawn_blocking(move || decoded(&lookup(&name)?))
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let length = pcm.duration();
    crate::audio::command(crate::audio::Command::Play(pcm, volume));
    // Played by the server from here; waited out so the next (the right speaker after the
    // left) follows it.
    tokio::time::sleep(length).await;
}

/// The speakers, left then right (checks both play, and the balance).
pub async fn speaker_test() {
    play_now("audio-channel-front-left", 1.0).await;
    play_now("audio-channel-front-right", 1.0).await;
}

/// Records `secs` seconds from the default microphone, in memory (the audio service's
/// connection records it; nothing is written to disk).
pub async fn mic_record(secs: u32) -> Result<Arc<Pcm>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::audio::command(crate::audio::Command::Record(
        secs,
        crate::audio::Reply::new(tx),
    ));
    rx.await
        .unwrap_or_else(|_| Err("The sound service is not running".into()))
}

/// Plays a recording back and waits until it has played.
pub async fn mic_playback(pcm: Arc<Pcm>) {
    let length = pcm.duration();
    crate::audio::command(crate::audio::Command::Play(pcm, 1.0));
    tokio::time::sleep(length).await;
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_the_theme_and_wav() {
        if let Some(f) = super::lookup(super::MESSAGE) {
            let p = super::decode(&f).unwrap();
            assert!(p.rate > 0 && p.channels > 0);
            assert!(p.duration() > std::time::Duration::from_millis(100));
        }
        // A 16-bit stereo WAV of two frames.
        let mut w = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x02\0\x44\xac\0\0\x10\xb1\x02\0\x04\0\x10\0data\x08\0\0\0".to_vec();
        w.extend([1, 0, 2, 0, 3, 0, 4, 0]);
        assert_eq!(super::wav(&w).unwrap(), (44100, 2, vec![1, 2, 3, 4]));
    }

    #[test]
    fn finds_the_message_sound() {
        // The freedesktop theme is a dependency of the desktop; skip where it is absent.
        if std::path::Path::new("/usr/share/sounds/freedesktop").is_dir() {
            assert!(super::lookup(super::MESSAGE).is_some());
        }
    }
}

#[cfg(test)]
mod live {
    /// Uploads and plays the message sound at level 0 through the real server (by hand:
    /// `--ignored`, then `pactl list samples short`).
    #[tokio::test]
    #[ignore]
    async fn plays_silently_through_the_sample_cache() {
        use futures::StreamExt;
        let mut s = Box::pin(crate::audio::stream());
        s.next().await;
        super::play_now(super::MESSAGE, 0.0).await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod live_mic {
    /// Records one second (by hand: `--ignored`); not played.
    #[tokio::test]
    #[ignore]
    async fn records_from_the_microphone() {
        use futures::StreamExt;
        let mut s = Box::pin(crate::audio::stream());
        s.next().await;
        let p = super::mic_record(1).await.unwrap();
        println!("recorded {:?} ({} samples)", p.duration(), p.samples.len());
        assert!(p.duration() > std::time::Duration::from_millis(500));
    }
}
