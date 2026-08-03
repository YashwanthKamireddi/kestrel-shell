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

/// PipeWire's current video input streams, kept whole: each `pw-dump` update replaces the
/// objects it names (or drops them), and who is using a camera is worked out again from this
/// picture every time, never from a chain of start/stop events (a missed one left the
/// indicator on, 2026-11-16).
#[derive(Default)]
struct Streams(HashMap<u64, serde_json::Value>);

impl Streams {
    fn apply(&mut self, batch: &[serde_json::Value]) {
        for o in batch {
            let Some(id) = o.get("id").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            match o.get("info") {
                Some(info)
                    if !info.is_null()
                        && info["props"]["media.class"].as_str() == Some("Stream/Input/Video") =>
                {
                    self.0.insert(id, info.clone());
                }
                _ => {
                    self.0.remove(&id);
                }
            }
        }
    }

    /// The apps whose camera stream is running now (an idle or paused stream is not a camera
    /// in use; PipeWire's own device nodes are not apps).
    fn apps(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .0
            .values()
            .filter(|info| info["state"].as_str() == Some("running"))
            .filter_map(|info| {
                let props = &info["props"];
                ["application.name", "application.process.binary"]
                    .iter()
                    .find_map(|k| props[*k].as_str().filter(|s| !s.is_empty()))
                    .filter(|n| !matches!(*n, "pipewire" | "wireplumber"))
                    .map(crate::apps::name_for_class)
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Reads `pw-dump --monitor` for as long as kestrel runs, sending the camera apps after every
/// update. If pw-dump ends or sends something unreadable it is started again: its first output
/// is the whole current picture, so nothing stale survives a hiccup.
fn watch_pipewire(tx: &futures::channel::mpsc::UnboundedSender<Vec<String>>) {
    use std::os::unix::process::CommandExt;
    loop {
        let mut cmd = std::process::Command::new("pw-dump");
        cmd.args(["--monitor", "--no-colors"]);
        // Safety: prctl only asks the kernel to end pw-dump when kestrel ends.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let Ok(mut child) = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        let mut streams = Streams::default();
        let mut last: Option<Vec<String>> = None;
        for batch in serde_json::Deserializer::from_reader(BufReader::new(stdout))
            .into_iter::<Vec<serde_json::Value>>()
        {
            let Ok(batch) = batch else { break };
            streams.apply(&batch);
            let apps = streams.apps();
            if last.as_ref() != Some(&apps) {
                last = Some(apps.clone());
                if tx.unbounded_send(apps).is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        // Nothing is known while it restarts: say so rather than keep the old answer.
        if tx.unbounded_send(Vec::new()).is_err() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

pub fn stream() -> impl Stream<Item = Vec<String>> {
    crate::stream::channel(4, async |mut out| {
        // PipeWire: pw-dump's updates, read on a blocking thread.
        let (ptx, mut prx) = futures::channel::mpsc::unbounded::<Vec<String>>();
        let _ = std::thread::Builder::new()
            .name("kestrel-camera".into())
            .spawn(move || watch_pipewire(&ptx));

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
                p = prx.next() => {
                    piped = p.unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::Streams;
    use serde_json::json;

    fn stream(id: u64, app: &str, state: &str) -> serde_json::Value {
        json!({ "id": id, "info": { "state": state, "props": {
            "media.class": "Stream/Input/Video", "application.name": app } } })
    }

    #[test]
    fn only_running_app_streams_count_and_removals_are_kept() {
        let mut s = Streams::default();
        s.apply(&[
            stream(1, "Chromium", "running"),
            stream(2, "Snapshot", "idle"),
        ]);
        assert_eq!(s.apps(), vec!["Chromium".to_string()]);
        // Paused: no longer a camera in use.
        s.apply(&[stream(1, "Chromium", "suspended")]);
        assert!(s.apps().is_empty());
        s.apply(&[stream(1, "Chromium", "running")]);
        s.apply(&[json!({ "id": 1, "info": null })]);
        assert!(s.apps().is_empty());
        // PipeWire's own device nodes are not apps.
        s.apply(&[json!({ "id": 9, "info": { "state": "running", "props": {
            "media.class": "Video/Source", "node.name": "Pipewire_devices-x" } } })]);
        assert!(s.apps().is_empty());
    }
}
