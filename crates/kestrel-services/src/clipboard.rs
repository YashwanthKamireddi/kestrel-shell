//! The clipboard, watched and set: what was just copied, as it happens, and kestrel's own
//! copies (a history entry, a file). Uses the wlr data-control protocol on its own Wayland
//! connection (no `wl-paste` or `wl-copy` processes, no focus needed), so the shell can say
//! "Copied" the moment anything copies, a screenshot included, and every copy goes into the
//! clipboard history (crate::clips) as it is read.
//!
//! The same content offered again (wl-clip-persist re-offers it when the source app quits) is
//! not reported twice. Password managers mark secrets with `x-kde-passwordManagerHint`; those
//! are reported without their content.

use futures::Stream;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

/// Something for kestrel to put on the clipboard: the types it is offered as, and its content.
struct Put {
    mimes: Vec<String>,
    data: Arc<Vec<u8>>,
    /// Told once the compositor has it (an app pasting after this gets the new content).
    done: tokio::sync::oneshot::Sender<()>,
}

/// A `wl_display.sync` answered: everything asked before it is done.
type Ack = Mutex<Option<tokio::sync::oneshot::Sender<()>>>;

/// The watcher's inbox for `Put`s, and the socket that wakes its loop.
static PUT: Mutex<Option<(std::sync::mpsc::Sender<Put>, std::os::unix::net::UnixStream)>> =
    Mutex::new(None);

/// The types text is offered as (what apps ask for, newest names first).
pub const TEXT_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// Puts `data` on the clipboard, offered as `mimes`. kestrel serves it while it is the
/// clipboard (and wl-clip-persist keeps it after). The answer comes once the compositor has it
/// (dropped unanswered if the clipboard is not connected).
pub fn copy(mimes: &[&str], data: Vec<u8>) -> tokio::sync::oneshot::Receiver<()> {
    let (done, rx) = tokio::sync::oneshot::channel();
    let g = PUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((tx, wake)) = g.as_ref() else {
        log::warn!("clipboard: not connected");
        return rx;
    };
    let put = Put {
        mimes: mimes.iter().map(|m| (*m).to_string()).collect(),
        data: Arc::new(data),
        done,
    };
    if tx.send(put).is_ok() {
        let _ = std::io::Write::write(&mut &*wake, &[1]);
    }
    rx
}

/// Largest copy that is read to describe it (bigger images are still reported, unread).
const READ_LIMIT: usize = 64 << 20;
const THUMB: (u32, u32) = (112, 64);

#[derive(Clone, Debug, PartialEq)]
pub enum Copied {
    /// Text: its start, whitespace collapsed.
    Text(String),
    /// An image: its size, a small RGBA thumbnail, and a copy of it kept in the runtime
    /// directory (memory-backed, gone at logout) so it can be opened after the clipboard moves on.
    Image {
        width: u32,
        height: u32,
        thumb: (u32, u32, Vec<u8>),
        file: Option<String>,
    },
    /// Something marked secret, or a type with nothing to show.
    Other,
}

struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<ZwlrDataControlManagerV1>,
    /// MIME types announced per offer.
    offers: HashMap<ObjectId, Vec<String>>,
    current: Option<ZwlrDataControlOfferV1>,
    device: Option<ZwlrDataControlDeviceV1>,
    /// kestrel's own copy while it is the clipboard.
    source: Option<ZwlrDataControlSourceV1>,
    /// The selection already present at start is not news.
    seen_first: bool,
    out: futures::channel::mpsc::UnboundedSender<Copied>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        st: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" if st.seat.is_none() => {
                    st.seat = Some(registry.bind(name, version.min(7), qh, ()));
                }
                "zwlr_data_control_manager_v1" => {
                    st.manager = Some(registry.bind(name, version.min(2), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlManagerV1,
        _: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        st: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            st.offers.entry(offer.id()).or_default().push(mime_type);
        }
    }
}

impl Dispatch<wayland_client::protocol::wl_callback::WlCallback, Ack> for State {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_callback::WlCallback,
        event: wayland_client::protocol::wl_callback::Event,
        ack: &Ack,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_callback::Event::Done { .. } = event
            && let Some(tx) = ack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
        {
            let _ = tx.send(());
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, Arc<Vec<u8>>> for State {
    fn event(
        st: &mut Self,
        source: &ZwlrDataControlSourceV1,
        event: zwlr_data_control_source_v1::Event,
        data: &Arc<Vec<u8>>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // An app pastes: the content is written at the app's pace, off the event loop.
            zwlr_data_control_source_v1::Event::Send { fd, .. } => {
                let data = data.clone();
                std::thread::spawn(move || {
                    let _ = std::io::Write::write_all(&mut std::fs::File::from(fd), &data);
                });
            }
            // Something else is the clipboard now.
            zwlr_data_control_source_v1::Event::Cancelled => {
                source.destroy();
                if st.source.as_ref() == Some(source) {
                    st.source = None;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        st: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _: &(),
        conn: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::Selection { id } => {
                if let Some(old) = st.current.take() {
                    st.offers.remove(&old.id());
                    old.destroy();
                }
                let Some(offer) = id else {
                    return;
                };
                let mimes = st.offers.get(&offer.id()).cloned().unwrap_or_default();
                if st.seen_first {
                    read_offer(&offer, &mimes, conn, st.out.clone());
                } else {
                    st.seen_first = true;
                }
                st.current = Some(offer);
            }
            zwlr_data_control_device_v1::Event::PrimarySelection { id: Some(offer) } => {
                // Primary selection (middle click) is not "copying"; drop it.
                st.offers.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

/// Ask the source for the best type and describe it on another thread (the source writes to
/// the pipe at its own pace; the event loop must keep running meanwhile).
fn read_offer(
    offer: &ZwlrDataControlOfferV1,
    mimes: &[String],
    conn: &Connection,
    out: futures::channel::mpsc::UnboundedSender<Copied>,
) {
    if mimes.iter().any(|m| m == "x-kde-passwordManagerHint") {
        let _ = out.unbounded_send(Copied::Other);
        return;
    }
    let image = mimes
        .iter()
        .find(|m| *m == "image/png")
        .or_else(|| mimes.iter().find(|m| m.starts_with("image/")));
    let text = [
        "text/plain;charset=utf-8",
        "UTF8_STRING",
        "text/plain",
        "STRING",
    ]
    .iter()
    .find_map(|t| mimes.iter().find(|m| m == t));
    let Some(mime) = image.or(text).cloned() else {
        let _ = out.unbounded_send(Copied::Other);
        return;
    };
    let mut fds = [0; 2];
    // Safety: pipe2 fills both descriptors on success; each is owned exactly once below.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return;
    }
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    offer.receive(mime.clone(), write.as_fd());
    let _ = conn.flush();
    drop(write);
    let is_image = mime.starts_with("image/");
    let mime = mime.clone();
    std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = std::fs::File::from(read)
            .take(READ_LIMIT as u64)
            .read_to_end(&mut data);
        let copied = describe(&data, is_image, &mime);
        // Into the history first, so the list is current when "Copied" arrives.
        match &copied {
            Copied::Image { width, height, .. } => {
                crate::clips::add(&mime, &data, Some((*width, *height)));
            }
            Copied::Text(_) => {
                crate::clips::add(&mime, &data, None);
            }
            Copied::Other => {}
        }
        let _ = out.unbounded_send(copied);
    });
}

fn describe(data: &[u8], is_image: bool, mime: &str) -> Copied {
    if is_image {
        return match image::load_from_memory(data) {
            Ok(img) => {
                let (width, height) = (img.width(), img.height());
                let t = img.thumbnail(THUMB.0, THUMB.1).into_rgba8();
                Copied::Image {
                    width,
                    height,
                    thumb: (t.width(), t.height(), t.into_raw()),
                    file: keep(data, mime),
                }
            }
            Err(_) => Copied::Other,
        };
    }
    let text = String::from_utf8_lossy(data);
    let flat: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect();
    if flat.is_empty() {
        Copied::Other
    } else {
        Copied::Text(flat)
    }
}

/// Keeps a copied image in `$XDG_RUNTIME_DIR/kestrel/copied` (the newest ten), for opening it
/// from the notification list.
fn keep(data: &[u8], mime: &str) -> Option<String> {
    let dir = std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?)
        .join("kestrel")
        .join("copied");
    std::fs::create_dir_all(&dir).ok()?;
    let ext = mime.strip_prefix("image/").unwrap_or("png");
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    let path = dir.join(format!("{ms}.{ext}"));
    std::fs::write(&path, data).ok()?;
    let mut old: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    old.sort();
    for p in old.iter().rev().skip(10) {
        let _ = std::fs::remove_file(p);
    }
    Some(path.to_string_lossy().into_owned())
}

fn digest(c: &Copied) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match c {
        Copied::Text(t) => t.hash(&mut h),
        Copied::Image {
            width,
            height,
            thumb,
            ..
        } => (width, height, &thumb.2).hash(&mut h),
        Copied::Other => 0u8.hash(&mut h),
    }
    h.finish()
}

/// Everything copied from now on.
pub fn stream() -> impl Stream<Item = Copied> {
    crate::stream::channel(8, async |mut out| {
        use futures::{SinkExt, StreamExt};
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("kestrel-clipboard".into())
            .spawn(move || {
                if let Err(e) = watch(tx) {
                    log::warn!("clipboard: {e}");
                }
            });
        if spawned.is_err() {
            return;
        }
        let mut last = None;
        while let Some(c) = rx.next().await {
            // wl-clip-persist offers the same content again when its source quits.
            let d = digest(&c);
            if last == Some(d) {
                continue;
            }
            last = Some(d);
            if out.send(c).await.is_err() {
                break;
            }
        }
    })
}

fn watch(out: futures::channel::mpsc::UnboundedSender<Copied>) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut st = State {
        seat: None,
        manager: None,
        offers: HashMap::new(),
        current: None,
        device: None,
        source: None,
        seen_first: false,
        out,
    };
    queue.roundtrip(&mut st)?;
    let (Some(manager), Some(seat)) = (st.manager.clone(), st.seat.clone()) else {
        anyhow::bail!("the compositor has no wlr data-control");
    };
    st.device = Some(manager.get_data_device(&seat, &qh, ()));
    // If nothing is on the clipboard at start, the first copy is news.
    queue.roundtrip(&mut st)?;
    st.seen_first = true;
    let (puts, inbox) = std::sync::mpsc::channel::<Put>();
    let (wake_rx, wake_tx) = std::os::unix::net::UnixStream::pair()?;
    wake_rx.set_nonblocking(true)?;
    *PUT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((puts, wake_tx));
    // Waits on both: the compositor's events and kestrel's own copies.
    let result = (|| -> anyhow::Result<()> {
        loop {
            queue.dispatch_pending(&mut st)?;
            conn.flush()?;
            let Some(guard) = queue.prepare_read() else {
                continue;
            };
            let mut fds = [
                libc::pollfd {
                    fd: guard.connection_fd().as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake_rx.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // Safety: poll on two descriptors that live through the call.
            if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e.into());
            }
            if fds[0].revents != 0 {
                match guard.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(e))
                        if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            } else {
                drop(guard);
            }
            if fds[1].revents != 0 {
                let mut buf = [0u8; 64];
                while (&wake_rx).read(&mut buf).is_ok_and(|n| n > 0) {}
                while let Ok(put) = inbox.try_recv() {
                    let source = manager.create_data_source(&qh, put.data);
                    for m in &put.mimes {
                        source.offer(m.clone());
                    }
                    if let Some(device) = &st.device {
                        device.set_selection(Some(&source));
                    }
                    if let Some(old) = st.source.replace(source) {
                        old.destroy();
                    }
                    // Answered once the compositor has done all of the above.
                    conn.display().sync(&qh, Mutex::new(Some(put.done)));
                }
            }
        }
    })();
    *PUT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_collapsed_and_cut() {
        let long = "a  b\n\tc ".repeat(100);
        match describe(long.as_bytes(), false, "text/plain") {
            Copied::Text(t) => {
                assert!(t.starts_with("a b c a b c"));
                assert_eq!(t.chars().count(), 120);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(describe(b"  \n", false, "text/plain"), Copied::Other);
    }
}
