//! Which apps are using a camera (the bar's privacy indicator, beside the microphone's).
//!
//! Two ways an app reaches a camera, both watched, nothing polled:
//! - Through PipeWire (Snapshot, browsers, calls via the portal): PipeWire itself keeps the
//!   device open even while nothing streams, so the device being open says nothing. What counts
//!   is a running video input stream, from PipeWire's own picture (crate::pipewire).
//! - Directly (ffmpeg, some apps): every `/dev/video*` is watched with inotify for opens and
//!   closes, and the processes holding one are named. PipeWire and WirePlumber are not counted
//!   (their side is the first case), and neither is a program feeding a virtual camera (opened
//!   write-only: a phone camera tool), which is a camera source, not an app looking.
//!
//! A direct use counts once it has lasted `HOLD`: listing devices opens them for a moment.

use futures::{SinkExt, Stream, StreamExt};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a direct use must last to count (listing a camera opens it for milliseconds).
const HOLD: Duration = Duration::from_millis(1200);

fn cameras() -> Vec<PathBuf> {
    std::fs::read_dir("/dev")
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("video"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The apps holding a camera open directly (not through PipeWire, not feeding one).
pub fn direct_users() -> Vec<String> {
    let cams = cameras();
    if cams.is_empty() {
        return Vec::new();
    }
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut out = BTreeSet::new();
    for p in procs.flatten() {
        if !p
            .file_name()
            .to_str()
            .is_some_and(|n| n.parse::<u32>().is_ok())
        {
            continue;
        }
        let comm = std::fs::read_to_string(p.path().join("comm")).unwrap_or_default();
        let comm = comm.trim();
        if matches!(comm, "pipewire" | "wireplumber") {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else {
            continue;
        };
        let reading = fds.flatten().any(|fd| {
            std::fs::read_link(fd.path()).is_ok_and(|t| cams.contains(&t)) && {
                // O_WRONLY (1): it writes frames into a virtual camera.
                let info = p.path().join("fdinfo").join(fd.file_name());
                let flags = std::fs::read_to_string(info)
                    .ok()
                    .and_then(|t| {
                        t.lines()
                            .find_map(|l| l.strip_prefix("flags:"))
                            .and_then(|f| u32::from_str_radix(f.trim(), 8).ok())
                    })
                    .unwrap_or(0);
                flags & 3 != 1
            }
        });
        if reading {
            out.insert(crate::apps::name_for_class(comm));
        }
    }
    out.into_iter().collect()
}

pub fn stream() -> impl Stream<Item = Vec<String>> {
    crate::stream::channel(4, async |mut out| {
        // PipeWire: its picture of the camera streams (crate::pipewire).
        let mut pw = crate::pipewire::watch();

        // Direct users: inotify on the devices.
        let Ok(ino) = inotify::Inotify::init() else {
            return;
        };
        let mut w = ino.watches();
        let watch_all = |w: &mut inotify::Watches| {
            let mask = inotify::WatchMask::OPEN
                | inotify::WatchMask::CLOSE_WRITE
                | inotify::WatchMask::CLOSE_NOWRITE;
            for c in cameras() {
                let _ = w.add(&c, mask);
            }
        };
        watch_all(&mut w);
        let _ = w.add(Path::new("/dev"), inotify::WatchMask::CREATE);
        let Ok(mut events) = ino.into_event_stream([0u8; 1024]) else {
            return;
        };
        let look = || async {
            tokio::task::spawn_blocking(direct_users)
                .await
                .unwrap_or_default()
        };
        let mut direct = look().await;
        let mut piped: Vec<String> = Vec::new();
        let mut shown: Option<Vec<String>> = None;
        loop {
            let mut now: Vec<String> = direct.iter().chain(&piped).cloned().collect();
            now.sort();
            now.dedup();
            if shown.as_ref() != Some(&now) {
                log::info!(
                    "camera: {}",
                    if now.is_empty() {
                        "off".to_string()
                    } else {
                        now.join(", ")
                    }
                );
                shown = Some(now.clone());
                if out.send(now).await.is_err() {
                    return;
                }
            }
            tokio::select! {
                // Any PipeWire change is also a reason to look at the devices again (a close the
                // inotify watch missed must not keep an app shown).
                changed = pw.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    piped = pw.borrow_and_update().camera.clone();
                    direct = look().await;
                }
                e = events.next() => {
                    let Some(Ok(e)) = e else { return };
                    if e.mask.contains(inotify::EventMask::CREATE) {
                        if e.name.as_deref().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("video")) {
                            watch_all(&mut w);
                        }
                        continue;
                    }
                    // A burst of opens and closes is one change: let it settle, then look.
                    // Starting waits out a listing probe; stopping is reported at once.
                    let wait = if e.mask.contains(inotify::EventMask::OPEN) && direct.is_empty() {
                        HOLD
                    } else {
                        Duration::from_millis(150)
                    };
                    let settle = tokio::time::sleep(wait);
                    tokio::pin!(settle);
                    loop {
                        tokio::select! {
                            () = &mut settle => break,
                            more = events.next() => if more.is_none() { return },
                        }
                    }
                    direct = look().await;
                }
            }
        }
    })
}
