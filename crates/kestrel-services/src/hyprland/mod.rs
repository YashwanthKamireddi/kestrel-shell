//! Hyprland, through its two IPC sockets: events arrive on `.socket2.sock`; state is read and
//! commands are sent on `.socket.sock`. No `hyprctl` processes.

use futures::{SinkExt, Stream};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

mod binds;
mod requests;
pub use binds::*;
pub use requests::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    pub id: i32,
    pub monitor: String,
    pub windows: u32,
    /// Window class of the workspace's most recently focused window (empty when none).
    pub app: String,
}

/// A window, as the overview needs it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    pub address: String,
    pub class: String,
    pub title: String,
    pub workspace: i32,
    /// 0 = focused now; higher = longer ago.
    pub focus: i32,
    /// Position and size on its monitor, in logical pixels.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Its monitor's logical size.
    pub screen: (f32, f32),
    pub floating: bool,
    /// 0 normal, 1 maximized, 2 fullscreen (it covers the others on its workspace).
    pub fullscreen: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    /// Active workspace id per monitor name.
    pub active: Vec<(String, i32)>,
    pub focused_monitor: String,
    pub window_title: String,
    pub window_class: String,
    /// The focused window's address ("0x…"), to send it something directly.
    pub window_address: String,
    /// The focused monitor shows a fullscreen window.
    pub fullscreen: Vec<String>,
    /// The screen is being recorded or shared (a capture that has lasted over a second, so a
    /// screenshot does not count). From Hyprland's `screencast` events.
    pub screencast: bool,
    /// The file a running wf-recorder is writing (shown when it stops).
    pub recording: Option<String>,
    /// When that recorder started (its process's start), for the bar's timer.
    pub recording_since: Option<std::time::SystemTime>,
    /// That recorder's process (stopping it stops only this one).
    pub recording_pid: Option<u32>,
    /// When the screen was last captured outside a recording (a screenshot): an image copied
    /// right after it is a screenshot.
    pub captured_at: Option<std::time::SystemTime>,
    /// The keyboard layout in use ("English (US)"); empty until known.
    pub layout: String,
}

impl State {
    pub fn active_on(&self, monitor: &str) -> Option<i32> {
        self.active
            .iter()
            .find(|(m, _)| m == monitor)
            .map(|(_, id)| *id)
    }
}

// Every field has a default: while a monitor is being switched on or off Hyprland briefly
// reports entries without some of them (an `activeWorkspace` of `{}`), and one such entry
// made the whole state unreadable, freezing the workspaces in the bar.
#[derive(Deserialize)]
struct JWorkspace {
    #[serde(default)]
    id: i32,
    #[serde(default)]
    monitor: String,
    #[serde(default)]
    windows: u32,
}

#[derive(Deserialize, Default)]
struct JRef {
    #[serde(default)]
    id: i32,
}

#[derive(Deserialize)]
struct JMonitor {
    #[serde(default)]
    name: String,
    #[serde(default)]
    focused: bool,
    #[serde(rename = "activeWorkspace", default)]
    active: JRef,
}

#[derive(Deserialize, Default)]
struct JWindow {
    #[serde(default)]
    address: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    class: String,
    #[serde(default)]
    fullscreen: u8,
    #[serde(default)]
    monitor: i32,
}

#[derive(Deserialize)]
struct JMonitorId {
    #[serde(default)]
    id: i32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    transform: i32,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize)]
struct JClient {
    #[serde(default)]
    address: String,
    workspace: JRef,
    #[serde(default)]
    class: String,
    #[serde(default)]
    title: String,
    #[serde(rename = "focusHistoryID", default)]
    focus: i32,
    #[serde(default)]
    at: (f32, f32),
    #[serde(default)]
    size: (f32, f32),
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    fullscreen: u8,
    #[serde(default)]
    monitor: i32,
}

/// A fresh snapshot (window positions change without an event, e.g. on resize).
pub async fn snapshot() -> Option<State> {
    read_state().await.ok()
}

async fn read_state() -> anyhow::Result<State> {
    // The four asked at once (each its own connection), not one after another.
    let (ws_raw, clients_raw, mons_raw, win_raw) = tokio::join!(
        request("j/workspaces"),
        request("j/clients"),
        request("j/monitors"),
        request("j/activewindow"),
    );
    let ws: Vec<JWorkspace> = serde_json::from_str(&ws_raw?)?;
    let clients: Vec<JClient> = serde_json::from_str(&clients_raw?).unwrap_or_default();
    // Per workspace: the class of the window focused most recently (lowest history id).
    let app_on = |id: i32| {
        clients
            .iter()
            .filter(|c| c.workspace.id == id && !c.class.is_empty())
            .min_by_key(|c| c.focus)
            .map(|c| c.class.clone())
            .unwrap_or_default()
    };
    let mons_raw = mons_raw?;
    let mons: Vec<JMonitor> = serde_json::from_str(&mons_raw)?;
    let mon_ids: Vec<JMonitorId> = serde_json::from_str(&mons_raw)?;
    let win: JWindow = serde_json::from_str(&win_raw?).unwrap_or_default();
    let mut workspaces: Vec<Workspace> = ws
        .into_iter()
        .filter(|w| w.id > 0)
        .map(|w| Workspace {
            id: w.id,
            monitor: w.monitor,
            windows: w.windows,
            app: app_on(w.id),
        })
        .collect();
    workspaces.sort_by_key(|w| w.id);
    let fullscreen = if win.fullscreen > 0 {
        mon_ids
            .iter()
            .filter(|m| m.id == win.monitor)
            .map(|m| m.name.clone())
            .collect()
    } else {
        Vec::new()
    };
    let mut windows: Vec<Window> = clients
        .iter()
        .filter(|c| c.workspace.id > 0)
        .map(|c| {
            // Logical monitor box (rotated monitors swap width and height).
            let m = mon_ids.iter().find(|m| m.id == c.monitor);
            let (mx, my, mw, mh) = m.map_or((0.0, 0.0, 1920.0, 1080.0), |m| {
                let s = if m.scale > 0.0 { m.scale } else { 1.0 };
                let (w, h) = if m.transform % 2 == 1 {
                    (m.height, m.width)
                } else {
                    (m.width, m.height)
                };
                (m.x, m.y, w / s, h / s)
            });
            Window {
                address: c.address.clone(),
                class: c.class.clone(),
                title: c.title.clone(),
                workspace: c.workspace.id,
                focus: c.focus,
                x: c.at.0 - mx,
                y: c.at.1 - my,
                w: c.size.0,
                h: c.size.1,
                screen: (mw, mh),
                floating: c.floating,
                fullscreen: c.fullscreen,
            }
        })
        .collect();
    windows.sort_by_key(|w| (w.workspace, w.focus));
    Ok(State {
        workspaces,
        windows,
        active: mons.iter().map(|m| (m.name.clone(), m.active.id)).collect(),
        focused_monitor: mons
            .iter()
            .find(|m| m.focused)
            .map(|m| m.name.clone())
            .unwrap_or_default(),
        window_title: win.title,
        window_class: win.class,
        window_address: win.address,
        fullscreen,
        screencast: false,
        recording: None,
        recording_since: None,
        recording_pid: None,
        captured_at: None,
        layout: String::new(),
    })
}

/// This session's running wf-recorder: its process id and the file it writes (`-f`).
fn recorder_running() -> Option<(u32, String, Option<std::time::SystemTime>)> {
    // This session's recorder only (another session's is not this screen being recorded).
    let (pid, dir) = crate::session::processes("wf-recorder")
        .into_iter()
        .next()?;
    let cmd = std::fs::read(dir.join("cmdline")).unwrap_or_default();
    let args: Vec<String> = cmd
        .split(|b| *b == 0)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let file = args
        .iter()
        .position(|a| a == "-f" || a == "--file")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| {
            args.iter()
                .find_map(|a| a.strip_prefix("--file=").map(String::from))
        })
        .unwrap_or_default();
    // The process directory appears when the process starts.
    let since = std::fs::metadata(&dir).and_then(|m| m.modified()).ok();
    Some((pid, file, since))
}

/// How long a capture lasts before the bar says the screen is being recorded.
const CAST_AFTER: std::time::Duration = std::time::Duration::from_millis(600);
/// How long a capture must stay off before the bar stops saying so.
const CAST_OFF: std::time::Duration = std::time::Duration::from_millis(1500);

/// Events that change what the bar shows.
fn relevant(event: &str) -> bool {
    matches!(
        event,
        "workspacev2"
            | "createworkspacev2"
            | "destroyworkspacev2"
            | "moveworkspacev2"
            | "focusedmonv2"
            | "activewindowv2"
            | "windowtitlev2"
            | "openwindow"
            | "closewindow"
            | "movewindowv2"
            | "fullscreen"
            | "monitoraddedv2"
            | "monitorremovedv2"
            | "renameworkspace"
    )
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(16, async |mut out| {
        loop {
            let Some(dir) = socket_dir() else {
                log::warn!("HYPRLAND_INSTANCE_SIGNATURE not set; workspaces disabled");
                std::future::pending::<()>().await;
                return;
            };
            match UnixStream::connect(dir.join(".socket2.sock")).await {
                Ok(stream) => {
                    // A recorder (wf-recorder) shows the moment it starts and goes the moment its
                    // process ends. Anything else that captures (a screen share) shows once it
                    // has lasted CAST_AFTER, so a screenshot never does, and goes once it has
                    // been off for CAST_OFF: captures flip on and off as the screen changes.
                    let mut cast_since: Option<tokio::time::Instant> = None;
                    let mut cast_off: Option<tokio::time::Instant> = None;
                    let mut casting = false;
                    let mut captured: Option<std::time::SystemTime> = None;
                    // The layout in use: the main keyboard's now, then each `activelayout`.
                    let mut layout = main_layout().await.unwrap_or_default();
                    let mut recorder = recorder_running();
                    if let Ok(mut s) = read_state().await {
                        casting = recorder.is_some();
                        s.screencast = casting;
                        s.recording = recorder.as_ref().map(|r| r.1.clone());
                        s.recording_pid = recorder.as_ref().map(|r| r.0);
                        s.recording_since = recorder.as_ref().and_then(|r| r.2);
                        s.captured_at = captured;
                        s.layout.clone_from(&layout);
                        let _ = out.send(s).await;
                    }
                    // The last full state, for the quick workspace update.
                    let mut quick: Option<State> = None;
                    let mut lines = BufReader::new(stream).lines();
                    // The recorder's end, as the kernel reports it (its pidfd turns readable).
                    let mut exit_watch: Option<(u32, Exit)> = None;
                    // A burst of events (a window moved: several at once) is read once, a moment
                    // after its first event, while the rest of the burst keeps being read.
                    let mut dirty: Option<tokio::time::Instant> = None;
                    loop {
                        match &recorder {
                            Some((pid, _, _)) if exit_watch.as_ref().map(|w| w.0) != Some(*pid) => {
                                exit_watch = Some((*pid, Box::pin(process_exit(*pid))));
                            }
                            None => exit_watch = None,
                            _ => {}
                        }
                        let deadline = if recorder.is_some() {
                            None
                        } else if casting {
                            cast_off.map(|t| t + CAST_OFF)
                        } else {
                            cast_since.map(|t| t + CAST_AFTER)
                        };
                        let line = tokio::select! {
                            l = lines.next_line() => match l {
                                Ok(Some(l)) => l,
                                _ => break,
                            },
                            () = async {
                                match dirty {
                                    Some(d) => tokio::time::sleep_until(d).await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                dirty = None;
                                match read_state().await {
                                    Ok(mut s) => {
                                        quick = Some(s.clone());
                                        s.screencast = casting;
                                        s.recording = recorder.as_ref().map(|r| r.1.clone());
                                        s.recording_pid = recorder.as_ref().map(|r| r.0);
                                        s.recording_since = recorder.as_ref().and_then(|r| r.2);
                                        s.captured_at = captured;
                                        s.layout.clone_from(&layout);
                                        let _ = out.send(s).await;
                                    }
                                    Err(e) => log::warn!("hyprland state: {e}"),
                                }
                                continue;
                            }
                            // The recorder ended.
                            () = async {
                                match exit_watch.as_mut() {
                                    Some((_, f)) => f.await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                    recorder = None;
                                    casting = false;
                                    cast_since = None;
                                    cast_off = None;
                                    if let Ok(mut s) = read_state().await {
                                        s.screencast = false;
                                        s.layout.clone_from(&layout);
                                        let _ = out.send(s).await;
                                    }
                                continue;
                            }
                            () = async {
                                match deadline {
                                    Some(d) => tokio::time::sleep_until(d).await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                casting = !casting;
                                cast_off = None;
                                if !casting {
                                    cast_since = None;
                                }
                                if let Ok(mut s) = read_state().await {
                                    s.screencast = casting;
                                    s.layout.clone_from(&layout);
                                    let _ = out.send(s).await;
                                }
                                continue;
                            }
                        };
                        // "NAME>>DATA" (a line without ">>" is a name with no data).
                        let (name, data) = line.split_once(">>").unwrap_or((line.as_str(), ""));
                        // "activelayout>>KEYBOARD,LAYOUT": every keyboard says it; only a new
                        // name is news.
                        if name == "activelayout" {
                            let (kb, now) = data.split_once(',').unwrap_or_default();
                            // Virtual keyboards (wtype typing an emoji, remote input) come and
                            // go with a keymap of no name, announced as "error" and then the
                            // real one: two false layout changes on every emoji typed.
                            let now =
                                if kb.starts_with("hl-virtual-keyboard") || now.trim() == "error" {
                                    ""
                                } else {
                                    now.trim()
                                }
                                .to_string();
                            log::debug!("activelayout: {now:?} (was {layout:?})");
                            if !now.is_empty() && now != layout {
                                layout = now;
                                if let Ok(mut st) = read_state().await {
                                    st.screencast = casting;
                                    st.recording = recorder.as_ref().map(|r| r.1.clone());
                                    st.recording_pid = recorder.as_ref().map(|r| r.0);
                                    st.recording_since = recorder.as_ref().and_then(|r| r.2);
                                    st.captured_at = captured;
                                    st.layout = layout.clone();
                                    let _ = out.send(st).await;
                                }
                            }
                            continue;
                        }
                        if name == "screencast" {
                            // kestrel's own window pictures (the overview) are not a share.
                            if crate::previews::own_capture() {
                                continue;
                            }
                            let on = data.starts_with('1');
                            if on
                                && recorder.is_none()
                                && let Some(r) = recorder_running()
                            {
                                recorder = Some(r);
                                casting = true;
                                cast_since = None;
                                cast_off = None;
                                if let Ok(mut s) = read_state().await {
                                    s.screencast = true;
                                    s.recording = recorder.as_ref().map(|r| r.1.clone());
                                    s.recording_pid = recorder.as_ref().map(|r| r.0);
                                    s.recording_since = recorder.as_ref().and_then(|r| r.2);
                                    s.captured_at = captured;
                                    s.layout.clone_from(&layout);
                                    let _ = out.send(s).await;
                                }
                                continue;
                            }
                            if recorder.is_some() {
                                continue;
                            }
                            if on {
                                cast_since.get_or_insert_with(tokio::time::Instant::now);
                                cast_off = None;
                                if !casting {
                                    captured = Some(std::time::SystemTime::now());
                                    if let Ok(mut s) = read_state().await {
                                        s.captured_at = captured;
                                        s.layout.clone_from(&layout);
                                        let _ = out.send(s).await;
                                    }
                                }
                            } else if casting {
                                cast_off.get_or_insert_with(tokio::time::Instant::now);
                            } else {
                                cast_since = None;
                            }
                            continue;
                        }
                        // A workspace switch, said at once from the event itself (the full
                        // read below takes tens of milliseconds): the overview closes and the
                        // bar's highlight moves while the switch begins, not after it.
                        if name == "workspacev2"
                            && let Some(st) = quick.as_mut()
                            && let Some(id) =
                                data.split(',').next().and_then(|i| i.parse::<i32>().ok())
                        {
                            let mon = st.focused_monitor.clone();
                            if let Some(a) = st.active.iter_mut().find(|(m, _)| *m == mon) {
                                a.1 = id;
                            }
                            let _ = out.send(st.clone()).await;
                        }
                        if relevant(name) {
                            dirty.get_or_insert(
                                tokio::time::Instant::now() + std::time::Duration::from_millis(8),
                            );
                        }
                    }
                }
                Err(e) => log::warn!("hyprland events: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    })
}

#[cfg(test)]
mod tolerant_tests {
    #[test]
    fn a_monitor_without_its_workspace_does_not_spoil_the_state() {
        let m: Vec<super::JMonitor> =
            serde_json::from_str(r#"[{"name":"eDP-1","focused":false,"activeWorkspace":{}}]"#)
                .unwrap();
        assert_eq!(m[0].active.id, 0);
    }
}

/// The pointing devices people use (mice, touchpads), by Hyprland's device names: a
/// keyboard's media keys, virtual pointers (keyd) and the PS/2 stand-in are left out.
pub async fn pointer_devices() -> Vec<String> {
    let Ok(text) = request("j/devices").await else {
        return Vec::new();
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let names: Vec<String> = v["mice"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect();
    pointers_of(&names)
}

/// The devices worth a speed of their own among Hyprland's pointer names.
pub fn pointers_of(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|n| {
            !n.contains("consumer-control")
                && !n.contains("system-control")
                && !n.contains("virtual")
                && !n.contains("keyboard")
                && !n.contains("unknown")
                && !n.starts_with("ps/2")
                // A touchpad's own "-mouse" twin is the same hardware.
                && !n.strip_suffix("-mouse").is_some_and(|base| {
                    names.iter().any(|o| o == &format!("{base}-touchpad"))
                })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod pointer_tests {
    #[test]
    fn real_devices_only() {
        let names: Vec<String> = [
            "dell-kb216-wired-keyboard-consumer-control-1",
            "pixart-dell-ms116-usb-optical-mouse",
            "ven_04f3:00-04f3:32b3-mouse",
            "ven_04f3:00-04f3:32b3-touchpad",
            "ps/2-generic-mouse",
            "keyd-virtual-pointer",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            super::pointers_of(&names),
            [
                "pixart-dell-ms116-usb-optical-mouse",
                "ven_04f3:00-04f3:32b3-touchpad"
            ]
        );
    }
}

/// The main keyboard's layout now ("English (US)").
async fn main_layout() -> Option<String> {
    let text = request("j/devices").await.ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let kbs = v["keyboards"].as_array()?;
    kbs.iter()
        .find(|k| k["main"].as_bool() == Some(true))
        .or(kbs.first())
        .and_then(|k| k["active_keymap"].as_str())
        .map(str::to_string)
}

/// A recorder's end, awaited.
type Exit = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

/// Resolves when process `pid` ends: its pidfd turns readable then, no polling. Already gone:
/// at once. Without pidfd (old kernels): a look at /proc once a second.
async fn process_exit(pid: u32) {
    use std::os::fd::FromRawFd;
    // Safety: pidfd_open takes a pid and flags and returns a new fd or -1.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd >= 0 {
        // Safety: a fresh fd that nothing else owns.
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as i32) };
        // Safety: the fd is owned here and stays open as long as the AsyncFd holds it.
        if let Ok(a) = unsafe {
            tokio::io::unix::AsyncFd::register_with_interest(fd, tokio::io::Interest::READABLE)
        } {
            let _ = a.readable().await;
            return;
        }
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        return;
    }
    while std::path::Path::new(&format!("/proc/{pid}")).exists() {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod exit_tests {
    #[tokio::test]
    async fn process_exit_resolves_when_it_ends() {
        let mut c = std::process::Command::new("sleep")
            .arg("0.3")
            .spawn()
            .unwrap();
        let t = std::time::Instant::now();
        let pid = c.id();
        let waiter = tokio::spawn(super::process_exit(pid));
        // Reap it once it ends (the pidfd turns readable at exit, before the reap).
        let _ = tokio::task::spawn_blocking(move || c.wait()).await;
        waiter.await.unwrap();
        let d = t.elapsed();
        assert!(
            d >= std::time::Duration::from_millis(250) && d < std::time::Duration::from_secs(2)
        );
        // Already gone: at once.
        let t = std::time::Instant::now();
        super::process_exit(pid).await;
        assert!(t.elapsed() < std::time::Duration::from_millis(1100));
    }
}
