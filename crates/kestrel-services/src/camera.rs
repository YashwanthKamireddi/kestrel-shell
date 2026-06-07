//! Whether a camera is in use (the bar's privacy indicator, beside the microphone's).
//!
//! Nothing announces a camera turning on, so every `/dev/video*` is watched with inotify for
//! opens and closes (no polling; nothing runs while no camera is touched), and on each event the
//! processes holding a camera are counted from /proc. PipeWire and WirePlumber open cameras for a
//! moment to list them: a camera counts as in use only once it has stayed open for `HOLD`. New
//! cameras (a USB webcam plugged in) are picked up from /dev.

use futures::{SinkExt, Stream, StreamExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a camera must stay open to count (listing it takes a few milliseconds).
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

/// Whether any of this user's processes holds a camera open (their fd links name the device).
pub fn in_use() -> bool {
    let cams = cameras();
    if cams.is_empty() {
        return false;
    }
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return false;
    };
    procs.flatten().any(|p| {
        p.file_name()
            .to_str()
            .is_some_and(|n| n.parse::<u32>().is_ok())
            && std::fs::read_dir(p.path().join("fd")).is_ok_and(|fds| {
                fds.flatten()
                    .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|t| cams.contains(&t)))
            })
    })
}

fn watch_all(w: &mut inotify::Watches) {
    let mask = inotify::WatchMask::OPEN
        | inotify::WatchMask::CLOSE_WRITE
        | inotify::WatchMask::CLOSE_NOWRITE;
    for c in cameras() {
        let _ = w.add(&c, mask);
    }
}

/// `true` while a camera is in use, `false` when it stops; sent only on changes.
pub fn stream() -> impl Stream<Item = bool> {
    crate::stream::channel(4, async |mut out| {
        let Ok(ino) = inotify::Inotify::init() else {
            return;
        };
        let mut w = ino.watches();
        watch_all(&mut w);
        // A camera plugged in later appears in /dev.
        let _ = w.add(Path::new("/dev"), inotify::WatchMask::CREATE);
        let Ok(mut events) = ino.into_event_stream([0u8; 1024]) else {
            return;
        };
        let check = || async { tokio::task::spawn_blocking(in_use).await.unwrap_or(false) };
        let mut shown = check().await;
        let _ = out.send(shown).await;
        while let Some(Ok(e)) = events.next().await {
            if e.mask.contains(inotify::EventMask::CREATE) {
                if e.name
                    .as_deref()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("video"))
                {
                    watch_all(&mut w);
                }
                continue;
            }
            // A burst of opens and closes is one change; let it settle, then look. Turning on
            // waits out the listing probes; turning off is reported as soon as it is quiet.
            let opening = e.mask.contains(inotify::EventMask::OPEN);
            let wait = if opening && !shown {
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
            let now = check().await;
            if now != shown {
                shown = now;
                log::info!("camera: {}", if now { "in use" } else { "off" });
                if out.send(now).await.is_err() {
                    return;
                }
            }
        }
    })
}
