//! kestrel is the desktop's notification server (`org.freedesktop.Notifications`).

use futures::{SinkExt, Stream};
use std::collections::HashMap;
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

static SERVER: OnceLock<(Connection, Arc<Mutex<u32>>)> = OnceLock::new();

struct Server {
    next: Arc<Mutex<u32>>,
    tx: mpsc::UnboundedSender<Event>,
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
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let i = (y * stride + x * channels) as usize;
                        if i + channels as usize > data.len() {
                            return None;
                        }
                        rgba.extend_from_slice(&data[i..i + 3]);
                        rgba.push(if channels == 4 { data[i + 3] } else { 255 });
                    }
                }
                return Some(Image::Rgba(Pixels::new(w as u32, h as u32, rgba)));
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
        let id = if replaces_id > 0 {
            replaces_id
        } else {
            let mut n = self
                .next
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *n += 1;
            *n
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
        let next = Arc::new(Mutex::new(0));
        let server = Server {
            next: next.clone(),
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
                let _ = SERVER.set((c, next));
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
    fn bodies_are_plain_text() {
        assert_eq!(
            super::clean_body("<b>Hi</b> &amp; **Daily Digest**\n\n  `x` my__file  "),
            "Hi & Daily Digest\nx my__file"
        );
    }
}
