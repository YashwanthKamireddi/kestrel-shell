//! Which apps are using a camera (the bar's privacy indicator, beside the microphone's).
//!
//! Two ways an app reaches a camera, both watched, nothing polled:
//! - Through PipeWire (Snapshot, browsers, calls via the portal): PipeWire itself keeps the
//!   device open even while nothing streams, so the device being open says nothing. What counts
//!   is a video input stream, read from `pw-dump --monitor` (PipeWire's own tool; it reports each
//!   change as it happens and only runs while the indicator is in the bar).
//! - Directly (ffmpeg, some apps): every `/dev/video*` is watched with inotify for opens and
//!   closes, and the processes holding one are named. PipeWire and WirePlumber are not counted
//!   (their side is the first case), and neither is a program feeding a virtual camera (opened
//!   write-only: a phone camera tool), which is a camera source, not an app looking.
//!
//! A direct use counts once it has lasted `HOLD`: listing devices opens them for a moment.

use futures::{SinkExt, Stream, StreamExt};
use std::collections::{BTreeSet, HashMap};
use std::io::BufReader;
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

/// Applies one `pw-dump` update to the known video input streams (id → app name).
fn pipewire_streams(batch: &[serde_json::Value], known: &mut HashMap<u64, String>) {
    for o in batch {
        let Some(id) = o.get("id").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        if o.get("info").is_none_or(serde_json::Value::is_null) {
            known.remove(&id);
            continue;
        }
        let props = &o["info"]["props"];
        if props["media.class"].as_str() != Some("Stream/Input/Video") {
            continue;
        }
        let name = [
            "application.name",
            "application.process.binary",
            "node.name",
        ]
        .iter()
        .find_map(|k| props[*k].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("An app");
        known.insert(id, crate::apps::name_for_class(name));
    }
}

/// The apps using a camera, sorted; sent on every change (empty: none).
pub fn stream() -> impl Stream<Item = Vec<String>> {
    crate::stream::channel(4, async |mut out| {
        // PipeWire: pw-dump's updates, read on a blocking thread.
        let (ptx, mut prx) = futures::channel::mpsc::unbounded::<Vec<String>>();
        let _ = std::thread::Builder::new()
            .name("kestrel-camera".into())
            .spawn(move || {
                let Ok(mut child) = std::process::Command::new("pw-dump")
                    .args(["--monitor", "--no-colors"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                else {
                    return;
                };
                let Some(stdout) = child.stdout.take() else {
                    return;
                };
                let mut known = HashMap::new();
                let mut last: Option<Vec<String>> = None;
                let batches = serde_json::Deserializer::from_reader(BufReader::new(stdout))
                    .into_iter::<Vec<serde_json::Value>>();
                for batch in batches {
                    let Ok(batch) = batch else { break };
                    pipewire_streams(&batch, &mut known);
                    let names: Vec<String> = known
                        .values()
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    if last.as_ref() != Some(&names) {
                        last = Some(names.clone());
                        if ptx.unbounded_send(names).is_err() {
                            break;
                        }
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
            });

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
                p = prx.next() => piped = p.unwrap_or_default(),
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
