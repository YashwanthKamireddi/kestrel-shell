//! The browser's media, published as a standard MPRIS player. Chromium starts this (the kestrel
//! binary, as a native messaging host for the "kestrel media" extension in `browser/`); the
//! extension reads what a page plays from the page itself (Apple Music's player, a page's media
//! session and its playing <audio>/<video>) and sends it here. This makes it a player like any
//! other: kestrel shows it, `playerctl` controls it, and commands go back to the tab.
//!
//! Messages are native messaging frames: a 32-bit little-endian length, then JSON.

use futures::StreamExt;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";

/// What the extension reports for the tab that matters.
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
struct Page {
    tab: Option<i64>,
    title: String,
    artist: String,
    album: String,
    art: String,
    playing: bool,
    /// Seconds.
    position: f64,
    duration: f64,
    can_next: bool,
    can_prev: bool,
    site: String,
}

struct Shared {
    page: Page,
    /// When `page.position` was true.
    at: Instant,
    /// The cover, downloaded: (its URL, the local file).
    art: Option<(String, String)>,
    /// The browser's process id, so the shell can tell this player and the browser's own
    /// (`…chromium.instance<PID>`) apart from other browsers.
    pid: u32,
}

type State = Arc<Mutex<Shared>>;

fn send(cmd: &str, arg: Option<f64>) {
    let body = serde_json::json!({ "cmd": cmd, "arg": arg }).to_string();
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&(body.len() as u32).to_le_bytes());
    let _ = out.write_all(body.as_bytes());
    let _ = out.flush();
}

struct Root {
    state: State,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    async fn raise(&self) {}
    async fn quit(&self) {}
    #[zbus(property)]
    fn identity(&self) -> String {
        let s = self
            .state
            .lock()
            .map(|s| s.page.site.clone())
            .unwrap_or_default();
        if s.is_empty() { "Browser".into() } else { s }
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "chromium".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    state: State,
}

impl Player {
    fn page(&self) -> Page {
        self.state
            .lock()
            .map(|s| s.page.clone())
            .unwrap_or_default()
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    async fn play_pause(&self) {
        send("playpause", None);
    }
    async fn play(&self) {
        send("play", None);
    }
    async fn pause(&self) {
        send("pause", None);
    }
    async fn stop(&self) {
        send("pause", None);
    }
    async fn next(&self) {
        send("next", None);
    }
    async fn previous(&self) {
        send("previous", None);
    }
    /// Relative, in microseconds.
    async fn seek(&self, offset: i64) {
        let at = position(&self.state) + offset;
        send("seek", Some(at.max(0) as f64 / 1e6));
    }
    async fn set_position(&self, _track: ObjectPath<'_>, at: i64) {
        send("seek", Some(at.max(0) as f64 / 1e6));
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        let p = self.page();
        if p.title.is_empty() {
            "Stopped".into()
        } else if p.playing {
            "Playing".into()
        } else {
            "Paused".into()
        }
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let (p, art, pid) = match self.state.lock() {
            Ok(s) => (s.page.clone(), s.art.clone(), s.pid),
            Err(_) => return HashMap::new(),
        };
        let mut m = HashMap::new();
        let mut put = |k: &str, v: Value<'_>| {
            if let Ok(v) = OwnedValue::try_from(v) {
                m.insert(k.to_string(), v);
            }
        };
        if p.title.is_empty() {
            return m;
        }
        put("xesam:title", Value::from(p.title.clone()));
        put("xesam:artist", Value::from(vec![p.artist.clone()]));
        put("xesam:album", Value::from(p.album.clone()));
        if p.duration > 0.0 {
            put("mpris:length", Value::from((p.duration * 1e6) as i64));
        }
        if let Some((url, file)) = art.filter(|(u, _)| *u == p.art) {
            let _ = url;
            put("mpris:artUrl", Value::from(format!("file://{file}")));
        }
        if let Ok(id) = ObjectPath::try_from(format!("/dev/kestrel/track/t{:x}", hash(&p.title))) {
            put("mpris:trackid", Value::from(id));
        }
        put("kestrel:pid", Value::from(i64::from(pid)));
        m
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        position(&self.state)
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.page().can_next
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.page().can_prev
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.page().duration > 0.0
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
}

/// Microseconds, moved forward from the last report while playing.
fn position(state: &State) -> i64 {
    let Ok(s) = state.lock() else {
        return 0;
    };
    let mut secs = s.page.position;
    if s.page.playing {
        secs += s.at.elapsed().as_secs_f64();
    }
    if s.page.duration > 0.0 {
        secs = secs.min(s.page.duration);
    }
    (secs * 1e6) as i64
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Frames from the extension, read on a plain thread (stdin blocks).
fn frames() -> futures::channel::mpsc::UnboundedReceiver<Page> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut len = [0u8; 4];
            if stdin.read_exact(&mut len).is_err() {
                return; // The browser closed: end.
            }
            let n = u32::from_le_bytes(len) as usize;
            if n > 1 << 20 {
                return;
            }
            let mut buf = vec![0u8; n];
            if stdin.read_exact(&mut buf).is_err() {
                return;
            }
            let page: Page = serde_json::from_slice(&buf).unwrap_or_default();
            if tx.unbounded_send(page).is_err() {
                return;
            }
        }
    });
    rx
}

/// The cover into kestrel's runtime folder (players hand the shell local files).
async fn fetch_art(url: &str) -> Option<String> {
    if url.starts_with("data:") || url.is_empty() {
        return None;
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)?
        .join("kestrel")
        .join("art");
    std::fs::create_dir_all(&dir).ok()?;
    let stem = format!("{:x}", hash(url));
    for ext in ["jpg", "png", "webp"] {
        let f = dir.join(format!("{stem}.{ext}"));
        if f.is_file() {
            return Some(f.to_string_lossy().into_owned());
        }
    }
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    // Named by what it is (readers go by the extension); anything else is not kept.
    let ext = match image::guess_format(&bytes).ok()? {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        _ => return None,
    };
    let file = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&file, &bytes).ok()?;
    Some(file.to_string_lossy().into_owned())
}

/// Run as the native messaging host until the browser goes away.
pub fn run() {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    rt.block_on(async {
        let pid = std::os::unix::process::parent_id();
        let state: State = Arc::new(Mutex::new(Shared {
            page: Page::default(),
            at: Instant::now(),
            art: None,
            pid,
        }));
        let conn = async {
            zbus::connection::Builder::session()?
                .serve_at(
                    PATH,
                    Root {
                        state: state.clone(),
                    },
                )?
                .serve_at(
                    PATH,
                    Player {
                        state: state.clone(),
                    },
                )?
                // Its own process's id, as MPRIS names go: one name per host, so the hosts of
                // several browser profiles never take one another's name (they did, each taking
                // the name the others held, a new host every few seconds: 2026-11-17). The
                // browser's own id is in the metadata (`kestrel:pid`).
                .name(format!(
                    "org.mpris.MediaPlayer2.kestrel_browser.instance{}",
                    std::process::id()
                ))?
                .build()
                .await
        }
        .await;
        let Ok(conn) = conn else {
            return;
        };
        let Ok(player) = conn.object_server().interface::<_, Player>(PATH).await else {
            return;
        };
        let Ok(root) = conn.object_server().interface::<_, Root>(PATH).await else {
            return;
        };
        // It ends when the browser closes its end (the extension's port went away).
        let mut pages = frames();
        while let Some(page) = pages.next().await {
            let before = state.lock().map(|s| s.page.clone()).ok();
            let art_url = page.art.clone();
            let need_art = state
                .lock()
                .map(|s| s.art.as_ref().map(|(u, _)| u.as_str()) != Some(art_url.as_str()))
                .unwrap_or(false);
            if let Ok(mut s) = state.lock() {
                s.page = page;
                s.at = Instant::now();
            }
            if need_art {
                let file = fetch_art(&art_url).await;
                if let Ok(mut s) = state.lock() {
                    s.art = file.map(|f| (art_url.clone(), f));
                }
            }
            // Only what changed is announced: the extension's reports come every few seconds
            // whether or not anything did, and each announcement makes every player reader
            // (the shell, playerctl) read the player again.
            let Ok(now) = state.lock().map(|s| s.page.clone()) else {
                continue;
            };
            let old = before.unwrap_or_default();
            let e: &SignalEmitter<'_> = player.signal_emitter();
            let p = player.get().await;
            if need_art
                || old.title != now.title
                || old.artist != now.artist
                || old.album != now.album
                || old.duration != now.duration
                || old.tab != now.tab
            {
                let _ = p.metadata_changed(e).await;
            }
            if old.playing != now.playing {
                let _ = p.playback_status_changed(e).await;
            }
            if old.can_next != now.can_next {
                let _ = p.can_go_next_changed(e).await;
            }
            if old.can_prev != now.can_prev {
                let _ = p.can_go_previous_changed(e).await;
            }
            if (old.duration > 0.0) != (now.duration > 0.0) {
                let _ = p.can_seek_changed(e).await;
            }
            drop(p);
            if old.site != now.site {
                let r = root.get().await;
                let _ = r.identity_changed(root.signal_emitter()).await;
            }
        }
    });
}
