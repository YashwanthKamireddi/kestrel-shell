//! kestrel is the desktop's notification server (`org.freedesktop.Notifications`).

use futures::{SinkExt, Stream};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Structure};
use zbus::{Connection, interface};

/// Decoded RGBA pixels. `id` is a hash of the content, so the UI can keep one renderer
/// handle per distinct image however often the service re-reads it.
#[derive(Clone, Debug)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
    pub id: u64,
}

impl Pixels {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (width, height).hash(&mut h);
        data.hash(&mut h);
        Self {
            width,
            height,
            data: Arc::new(data),
            id: h.finish(),
        }
    }
}

impl PartialEq for Pixels {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Image {
    Path(String),
    Rgba(Pixels),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub app: String,
    /// Icon name or path for the app.
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub image: Option<Image>,
    pub urgency: u8,
    /// Milliseconds; -1 = server default, 0 = never.
    pub timeout: i32,
    pub time: SystemTime,
    /// The sound it asks for (spec hints `sound-file`, `sound-name`, `suppress-sound`).
    pub sound: crate::sounds::Sound,
    /// It updates one already shown (`replaces_id`): a progress or a changed message, which
    /// does not sound again.
    pub replaced: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    Added(Box<Notification>),
    Closed(u32),
}

#[derive(Clone, Copy, Debug)]
pub enum Reason {
    Expired = 1,
    Dismissed = 2,
    Closed = 3,
}

static SERVER: OnceLock<(Connection, Arc<Mutex<Ids>>)> = OnceLock::new();

struct Server {
    ids: Arc<Mutex<Ids>>,
    tx: mpsc::UnboundedSender<Event>,
}

/// The ids given out: the last one, and the ones the shell shows now (its list, sent after
/// every change by `shown`, plus the ones given since).
#[derive(Default)]
struct Ids {
    last: u32,
    shown: HashSet<u32>,
}

impl Ids {
    /// A new id: the one after the last, past `u32::MAX` back to 1 (0 means "none" in the
    /// spec), and never one still shown (an app's or one of kestrel's own, which count down
    /// from the top).
    fn next(&mut self) -> u32 {
        loop {
            self.last = self.last.checked_add(1).unwrap_or(1);
            if self.shown.insert(self.last) {
                return self.last;
            }
        }
    }
}

/// The ids the shell shows now, after every change to its list: a new notification never gets
/// one of them, however far the counter has gone round.
pub fn shown(ids: impl IntoIterator<Item = u32>) {
    if let Some((_, lock)) = SERVER.get() {
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shown = ids.into_iter().collect();
    }
}

fn sound_from_hints(hints: &HashMap<String, OwnedValue>) -> crate::sounds::Sound {
    use crate::sounds::Sound;
    let text = |k: &str| {
        hints
            .get(k)
            .and_then(|v| String::try_from(v.clone()).ok())
            .filter(|s| !s.is_empty())
    };
    if hints
        .get("suppress-sound")
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false)
    {
        Sound::Silent
    } else if let Some(f) = text("sound-file") {
        let f = f.strip_prefix("file://").unwrap_or(&f).to_string();
        Sound::File(f.into())
    } else if let Some(n) = text("sound-name") {
        Sound::Named(n)
    } else {
        Sound::Default
    }
}

/// The spec's `image-data` pixels (rows `stride` bytes apart, 3 or 4 bytes a pixel) as RGBA.
/// Every number comes from whichever program sent the notification: a negative or huge size,
/// a stride shorter than a row, or 1 or 2 channels made the old loop allocate exabytes or
/// slice past the data and panic. Now each is checked and such an image is simply not shown.
fn decode_rgba(w: i32, h: i32, stride: i32, channels: i32, data: &[u8]) -> Option<Pixels> {
    let (w, h, stride) = (
        usize::try_from(w).ok()?,
        usize::try_from(h).ok()?,
        usize::try_from(stride).ok()?,
    );
    if w == 0 || h == 0 || !matches!(channels, 3 | 4) {
        return None;
    }
    let channels = channels as usize;
    // Every row must fit its stride, and every row the data (the last may end short of it).
    let row = w.checked_mul(channels)?;
    if stride < row || data.len() < stride.checked_mul(h - 1)?.checked_add(row)? {
        return None;
    }
    let mut rgba = Vec::with_capacity(w.checked_mul(h)?.checked_mul(4)?);
    for line in (0..h).map(|y| &data[y * stride..y * stride + row]) {
        for px in line.chunks_exact(channels) {
            rgba.extend_from_slice(&px[..3]);
            rgba.push(if channels == 4 { px[3] } else { 255 });
        }
    }
    Some(Pixels::new(
        u32::try_from(w).ok()?,
        u32::try_from(h).ok()?,
        rgba,
    ))
}

fn image_from_hints(hints: &HashMap<String, OwnedValue>) -> Option<Image> {
    for key in ["image-data", "image_data", "icon_data"] {
        if let Some(v) = hints.get(key)
            && let Ok(s) = Structure::try_from(v.clone())
        {
            let f = s.fields();
            if f.len() == 7 {
                let num = |i: usize| i32::try_from(&f[i]).ok();
                let (Some(w), Some(h), Some(stride)) = (num(0), num(1), num(2)) else {
                    continue;
                };
                let alpha = bool::try_from(&f[3]).unwrap_or(true);
                let channels = num(5).unwrap_or(if alpha { 4 } else { 3 });
                let Ok(data) = Vec::<u8>::try_from(f[6].try_clone().ok()?) else {
                    continue;
                };
                return decode_rgba(w, h, stride, channels, &data).map(Image::Rgba);
            }
        }
    }
    for key in ["image-path", "image_path"] {
        if let Some(v) = hints.get(key)
            && let Ok(p) = String::try_from(v.clone())
            && !p.is_empty()
        {
            return Some(Image::Path(p.trim_start_matches("file://").to_string()));
        }
    }
    None
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = {
            let mut ids = self
                .ids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if replaces_id > 0 {
                ids.shown.insert(replaces_id);
                replaces_id
            } else {
                ids.next()
            }
        };
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .unwrap_or(1);
        let desktop = hints
            .get("desktop-entry")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();
        let sound = sound_from_hints(&hints);
        let n = Notification {
            id,
            app: if app_name.is_empty() {
                desktop.clone()
            } else {
                app_name
            },
            app_icon: if app_icon.is_empty() {
                desktop
            } else {
                app_icon
            },
            summary,
            body: clean_body(&body),
            actions: actions
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| (c[0].clone(), c[1].clone()))
                .collect(),
            image: image_from_hints(&hints),
            urgency,
            timeout: expire_timeout,
            time: SystemTime::now(),
            sound,
            replaced: replaces_id > 0,
        };
        let _ = self.tx.send(Event::Added(Box::new(n)));
        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let _ = self.tx.send(Event::Closed(id));
        let _ = Self::notification_closed(&emitter, id, Reason::Closed as u32).await;
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        vec![
            "body",
            "body-markup",
            "actions",
            "icon-static",
            "persistence",
            "sound",
        ]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("kestrel", "kestrel", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(emitter: &SignalEmitter<'_>, id: u32, action: &str)
    -> zbus::Result<()>;
}

/// Bodies may carry a little markup; kestrel shows plain text.
fn clean_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        // Chat apps send Markdown (Discord's **bold**): the marks are noise as plain text.
        .replace("**", "")
        .replace("~~", "")
        .replace('`', "")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn emitter() -> Option<SignalEmitter<'static>> {
    let (c, _) = SERVER.get()?;
    SignalEmitter::new(c, "/org/freedesktop/Notifications").ok()
}

/// Tell the sender a notification went away.
pub async fn closed(id: u32, reason: Reason) {
    if let Some(e) = emitter().await {
        let _ = Server::notification_closed(&e, id, reason as u32).await;
    }
}

pub async fn invoke(id: u32, action: String) {
    if let Some(e) = emitter().await {
        let _ = Server::action_invoked(&e, id, &action).await;
        let _ = Server::notification_closed(&e, id, Reason::Dismissed as u32).await;
    }
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = Event> {
    crate::stream::channel(16, async |mut out| {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let ids = Arc::new(Mutex::new(Ids::default()));
        let server = Server {
            ids: ids.clone(),
            tx,
        };
        let conn = async {
            zbus::connection::Builder::session()?
                .serve_at("/org/freedesktop/Notifications", server)?
                .name("org.freedesktop.Notifications")?
                .build()
                .await
        }
        .await;
        match conn {
            Ok(c) => {
                let _ = SERVER.set((c, ids));
            }
            Err(e) => {
                log::warn!("notifications: another server owns the name ({e})");
                return;
            }
        }
        while let Some(e) = rx.recv().await {
            if out.send(e).await.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn image_data_from_any_sender_cannot_panic() {
        use super::decode_rgba;
        // 2x2 RGB, rows padded to 8 bytes; the last row may end without its padding.
        let data = [1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12];
        let p = decode_rgba(2, 2, 8, 3, &data).unwrap();
        assert_eq!((p.width, p.height), (2, 2));
        assert_eq!(
            *p.data,
            [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        // Each of these panicked or tried to allocate more memory than exists.
        assert!(decode_rgba(-1, 1, 4, 4, &[0; 4]).is_none());
        assert!(decode_rgba(i32::MAX, i32::MAX, i32::MAX, 4, &[0; 16]).is_none());
        assert!(decode_rgba(1, 1, 4, 1, &[0; 4]).is_none());
        assert!(decode_rgba(2, 2, -8, 4, &[0; 16]).is_none());
        assert!(
            decode_rgba(2, 2, 4, 4, &[0; 16]).is_none(),
            "stride under a row"
        );
        assert!(
            decode_rgba(2, 2, 8, 4, &[0; 15]).is_none(),
            "data too short"
        );
    }

    #[test]
    fn ids_go_round_and_skip_the_ones_shown() {
        let mut ids = super::Ids::default();
        assert_eq!((ids.next(), ids.next()), (1, 2));
        // Past the top it starts again at 1 (0 means "none"), stepping over every id still
        // shown: an app's (1) and kestrel's own, which count down from the top.
        ids.last = u32::MAX - 2;
        ids.shown = [u32::MAX - 1, 1].into();
        assert_eq!(ids.next(), u32::MAX);
        assert_eq!(ids.next(), 2);
        // Once gone from the shell's list, an id may come round again.
        ids.last = 0;
        ids.shown = [2].into();
        assert_eq!(ids.next(), 1);
        assert_eq!(ids.next(), 3);
    }

    #[test]
    fn bodies_are_plain_text() {
        assert_eq!(
            super::clean_body("<b>Hi</b> &amp; **Daily Digest**\n\n  `x` my__file  "),
            "Hi & Daily Digest\nx my__file"
        );
    }
}
