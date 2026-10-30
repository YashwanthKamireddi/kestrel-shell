//! The clipboard, watched and set: what was just copied, as it happens, and kestrel's own
//! copies (a history entry, a file). Uses the wlr data-control protocol on its own Wayland
//! connection (no `wl-paste` or `wl-copy` processes, no focus needed), so the shell can say
//! "Copied" the moment anything copies, a screenshot included, and every copy goes into the
//! clipboard history (crate::clips) as it is read.
//!
//! The clipboard is kept when the app it came from quits (as KDE's Klipper does, "prevent empty
//! clipboard"): what was copied (its text, its picture, or its list of files) is held, and when
//! the clipboard goes empty kestrel offers it again, so it can still be pasted. That replaced
//! wl-clip-persist, which took over every copy the moment it was made. Content offered again
//! is not reported twice. Password managers mark secrets with `x-kde-passwordManagerHint`;
//! those are reported without their content and never kept.

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

/// What kestrel asks the clipboard's thread: to put something on it, or to open what is on it
/// as text (the pipe it arrives through; None when there is no text).
enum Ask {
    Put(Put),
    Paste(std::sync::mpsc::Sender<Option<OwnedFd>>),
}

/// A `wl_display.sync` answered: everything asked before it is done.
type Ack = Mutex<Option<tokio::sync::oneshot::Sender<()>>>;

/// What is on offer from kestrel: each type it can be pasted as, and its content.
type Offered = Arc<Vec<(String, Arc<Vec<u8>>)>>;

/// The last copy's content, kept to offer again when its app quits and the clipboard empties.
type Kept = Arc<Mutex<KeptCopy>>;

/// The kept content and the number of the copy it belongs to. Each copy is read on a thread of
/// its own, so a slow one (a large picture) could finish after a later copy's and put the older
/// content back: the clipboard, emptied, then came back with what was copied before.
#[derive(Default)]
struct KeptCopy {
    copy: u64,
    offered: Option<Offered>,
}

impl KeptCopy {
    /// A new copy has arrived: its number. Anything still being read for an older one is not
    /// kept when it is done.
    fn next(&mut self) -> u64 {
        self.copy += 1;
        self.copy
    }

    /// Keeps `offered` as copy `n`'s content, unless a newer copy has arrived since.
    fn set(&mut self, n: u64, offered: Option<Offered>) {
        if n == self.copy {
            self.offered = offered;
        }
    }
}

fn lock(kept: &Kept) -> std::sync::MutexGuard<'_, KeptCopy> {
    kept.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The watcher's inbox, and the socket that wakes its loop.
static PUT: Mutex<Option<(std::sync::mpsc::Sender<Ask>, std::os::unix::net::UnixStream)>> =
    Mutex::new(None);

/// Hands the clipboard's thread a request and wakes it; false when it is not running.
fn ask(a: Ask) -> bool {
    let g = PUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((tx, wake)) = g.as_ref() else {
        log::warn!("clipboard: not connected");
        return false;
    };
    if tx.send(a).is_err() {
        return false;
    }
    let _ = std::io::Write::write(&mut &*wake, &[1]);
    true
}

/// The types text is offered as (what apps ask for, newest names first).
pub const TEXT_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// Puts `data` on the clipboard, offered as `mimes`. kestrel serves it while it is the
/// clipboard, and keeps it after. The answer comes once the compositor has it
/// (dropped unanswered if the clipboard is not connected).
pub fn copy(mimes: &[&str], data: Vec<u8>) -> tokio::sync::oneshot::Receiver<()> {
    let (done, rx) = tokio::sync::oneshot::channel();
    ask(Ask::Put(Put {
        mimes: mimes.iter().map(|m| (*m).to_string()).collect(),
        data: Arc::new(data),
        done,
    }));
    rx
}

/// The longest a paste waits for the app that copied to hand its text over (a frozen app must
/// not freeze the shell). smithay-clipboard, which iced used before, waits the same way.
const PASTE_WAIT: std::time::Duration = std::time::Duration::from_millis(500);
/// The most text a paste takes.
const PASTE_LIMIT: usize = 16 << 20;

/// What is on the clipboard, as text, for pasting into kestrel's own text fields (iced's
/// clipboard, served by this one connection: there is no second clipboard). Blocks until the
/// copying app has written it, at most `PASTE_WAIT`.
pub fn paste_text() -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    if !ask(Ask::Paste(tx)) {
        return None;
    }
    let deadline = std::time::Instant::now() + PASTE_WAIT;
    let fd = rx.recv_timeout(PASTE_WAIT).ok()??;
    let mut file = std::fs::File::from(fd);
    let mut data = Vec::new();
    let mut buf = [0u8; 64 << 10];
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let mut p = libc::pollfd {
            fd: file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = i32::try_from(left.as_millis()).unwrap_or(i32::MAX);
        // Safety: one descriptor that lives through the call.
        if ms == 0 || unsafe { libc::poll(&raw mut p, 1, ms) } <= 0 {
            log::warn!("clipboard: the copying app did not hand over its text in time");
            return None;
        }
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) if data.len() + n <= PASTE_LIMIT => data.extend_from_slice(&buf[..n]),
            Ok(_) => return None,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    Some(String::from_utf8_lossy(&data).into_owned()).filter(|t| !t.is_empty())
}

/// Largest copy kestrel holds (kept after its app quits, put in the history). A bigger one is
/// reported as copied but not held: apps still paste it from the app it came from.
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
    /// The last copy, to offer again when the clipboard empties (its app quit).
    kept: Kept,
    manager_for_keep: Option<ZwlrDataControlManagerV1>,
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

impl Dispatch<ZwlrDataControlSourceV1, Offered> for State {
    fn event(
        st: &mut Self,
        source: &ZwlrDataControlSourceV1,
        event: zwlr_data_control_source_v1::Event,
        offered: &Offered,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // An app pastes: the content is written at the app's pace, off the event loop.
            zwlr_data_control_source_v1::Event::Send { fd, mime_type } => {
                let Some(data) = offered
                    .iter()
                    .find(|(m, _)| *m == mime_type)
                    .or_else(|| offered.first())
                    .map(|(_, d)| d.clone())
                else {
                    return;
                };
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
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::Selection { id } => {
                if let Some(old) = st.current.take() {
                    st.offers.remove(&old.id());
                    old.destroy();
                }
                let Some(offer) = id else {
                    // Empty: the app it came from quit. What it held is offered again.
                    if st.source.is_none() {
                        keep_offering(st, qh);
                    }
                    return;
                };
                let mimes = st.offers.get(&offer.id()).cloned().unwrap_or_default();
                // The copy already there at start is kept (its app may quit later) but is not
                // news: not announced, not added to the history again.
                read_offer(
                    &offer,
                    &mimes,
                    conn,
                    st.seen_first.then(|| st.out.clone()),
                    st.kept.clone(),
                );
                st.seen_first = true;
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

/// The kept copy offered again (the clipboard emptied: its app quit).
fn keep_offering(st: &mut State, qh: &QueueHandle<State>) {
    let kept = lock(&st.kept).offered.clone();
    let (Some(offered), Some(manager), Some(device)) =
        (kept, st.manager_for_keep.as_ref(), st.device.as_ref())
    else {
        return;
    };
    let source = manager.create_data_source(qh, offered.clone());
    for (m, _) in offered.iter() {
        source.offer(m.clone());
    }
    device.set_selection(Some(&source));
    if let Some(old) = st.source.replace(source) {
        old.destroy();
    }
}

/// Starts reading `mime` from the offer: the pipe to read it from.
fn receive(offer: &ZwlrDataControlOfferV1, mime: &str, conn: &Connection) -> Option<OwnedFd> {
    let mut fds = [0; 2];
    // Safety: pipe2 fills both descriptors on success; each is owned exactly once below.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return None;
    }
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    offer.receive(mime.to_string(), write.as_fd());
    let _ = conn.flush();
    drop(write);
    Some(read)
}

/// Ask the source for the best type and describe it on another thread (the source writes to
/// the pipe at its own pace; the event loop must keep running meanwhile). What was read is
/// kept, to offer again if its app quits (secrets never).
fn read_offer(
    offer: &ZwlrDataControlOfferV1,
    mimes: &[String],
    conn: &Connection,
    out: Option<futures::channel::mpsc::UnboundedSender<Copied>>,
    kept: Kept,
) {
    let n = lock(&kept).next();
    if mimes.iter().any(|m| m == "x-kde-passwordManagerHint") {
        lock(&kept).set(n, None);
        if let Some(out) = out {
            let _ = out.unbounded_send(Copied::Other);
        }
        return;
    }
    // Copied files (a file manager's copy): their list, kept as it was offered.
    let files = mimes
        .iter()
        .any(|m| m == "text/uri-list")
        .then(|| receive(offer, "text/uri-list", conn))
        .flatten();
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
    let chosen = image.or(text).cloned();
    let read = chosen.as_ref().and_then(|m| receive(offer, m, conn));
    let is_image = chosen.as_ref().is_some_and(|m| m.starts_with("image/"));
    let mime = chosen.unwrap_or_default();
    std::thread::spawn(move || {
        // The whole copy, or None when it is over READ_LIMIT: never its first part (a cut
        // copy offered again after its app quit would paste cut, with no sign of it).
        let slurp = |fd: OwnedFd| {
            let mut data = Vec::new();
            let _ = std::fs::File::from(fd)
                .take(READ_LIMIT as u64 + 1)
                .read_to_end(&mut data);
            (data.len() <= READ_LIMIT).then_some(data)
        };
        let (files, data) = match (files.map(slurp), read.map(slurp)) {
            (Some(None), _) | (_, Some(None)) => {
                // Too big to hold: not kept (the previous copy is not the clipboard any more
                // either), not in the history; announced as copied, nothing to show.
                lock(&kept).set(n, None);
                log::info!("clipboard: a copy over {} MB is not kept", READ_LIMIT >> 20);
                if let Some(out) = out {
                    let _ = out.unbounded_send(Copied::Other);
                }
                return;
            }
            (f, d) => (
                f.flatten().filter(|d| !d.is_empty()),
                d.flatten().unwrap_or_default(),
            ),
        };
        // Kept to offer again: the files' list, the picture, or the text under every name
        // text is asked for.
        let mut offered: Vec<(String, Arc<Vec<u8>>)> = Vec::new();
        if let Some(f) = files {
            offered.push(("text/uri-list".into(), Arc::new(f)));
        }
        if !data.is_empty() {
            let data = Arc::new(data.clone());
            if is_image {
                offered.push((mime.clone(), data));
            } else {
                offered.extend(TEXT_TYPES.iter().map(|t| ((*t).to_string(), data.clone())));
            }
        }
        lock(&kept).set(n, (!offered.is_empty()).then(|| Arc::new(offered)));
        // Only kept (the copy there at start): nothing to announce.
        let Some(out) = out else {
            return;
        };
        if mime.is_empty() {
            let _ = out.unbounded_send(Copied::Other);
            return;
        }
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
            // The same content offered again (kestrel keeping it after its app quit) is not
            // news.
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
        kept: Arc::default(),
        manager_for_keep: None,
        seen_first: false,
        out,
    };
    queue.roundtrip(&mut st)?;
    let (Some(manager), Some(seat)) = (st.manager.clone(), st.seat.clone()) else {
        anyhow::bail!("the compositor has no wlr data-control");
    };
    st.device = Some(manager.get_data_device(&seat, &qh, ()));
    st.manager_for_keep = Some(manager.clone());
    // If nothing is on the clipboard at start, the first copy is news.
    queue.roundtrip(&mut st)?;
    st.seen_first = true;
    let (puts, inbox) = std::sync::mpsc::channel::<Ask>();
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
                while let Ok(a) = inbox.try_recv() {
                    let put = match a {
                        Ask::Put(put) => put,
                        Ask::Paste(reply) => {
                            // The current copy's best text type, opened for reading.
                            let fd = st.current.as_ref().and_then(|offer| {
                                let mimes = st.offers.get(&offer.id())?;
                                let mime =
                                    TEXT_TYPES.iter().find(|t| mimes.iter().any(|m| m == *t))?;
                                receive(offer, mime, &conn)
                            });
                            let _ = reply.send(fd);
                            continue;
                        }
                    };
                    let offered: Offered = Arc::new(
                        put.mimes
                            .iter()
                            .map(|m| (m.clone(), put.data.clone()))
                            .collect(),
                    );
                    // kestrel's own copy is kept too (offered again if the clipboard empties).
                    {
                        let mut k = lock(&st.kept);
                        let n = k.next();
                        k.set(n, Some(offered.clone()));
                    }
                    let source = manager.create_data_source(&qh, offered);
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
    fn a_slow_read_of_an_older_copy_does_not_replace_a_newer_one() {
        let text = |t: &[u8]| -> Option<Offered> {
            Some(Arc::new(vec![("text/plain".into(), Arc::new(t.to_vec()))]))
        };
        let mut k = KeptCopy::default();
        let picture = k.next();
        let newer = k.next();
        k.set(newer, text(b"newer"));
        // The picture's thread finishes last.
        k.set(picture, text(b"older"));
        assert_eq!(*k.offered.unwrap()[0].1, b"newer");
        // A newer copy that is not kept (a secret) is not undone by an older one either.
        let mut k = KeptCopy::default();
        let older = k.next();
        let secret = k.next();
        k.set(secret, None);
        k.set(older, text(b"older"));
        assert!(k.offered.is_none());
    }

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
