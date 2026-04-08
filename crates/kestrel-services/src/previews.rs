//! Live window previews: a picture of any window, on any workspace, for the overview. Uses
//! Hyprland's toplevel export protocol (the one its portal uses to share a window) on its own
//! Wayland connection. `capture` takes one picture of each window; `live` keeps following
//! them while the overview is open (Hyprland answers a copy on its next monitor redraw, so a
//! still screen sends nothing), reusing one shared buffer per window and scaling straight from
//! it into the tile's picture.
//!
//! Protocol definition: protocols/hyprland-toplevel-export-v1.xml, from hyprwm/hyprland-protocols
//! (BSD-3-Clause), unchanged.

#![allow(clippy::all, clippy::pedantic, missing_docs, unused_qualifications)]

pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-toplevel-export-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-toplevel-export-v1.xml");
}

use protocol::hyprland_toplevel_export_frame_v1::{self as frame, HyprlandToplevelExportFrameV1};
use protocol::hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1;
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

#[derive(Default)]
struct State {
    shm: Option<wl_shm::WlShm>,
    manager: Option<HyprlandToplevelExportManagerV1>,
    /// Offered shared-memory buffer: format, width, height, stride.
    offer: Option<(wl_shm::Format, u32, u32, u32)>,
    buffer_done: bool,
    ready: bool,
    failed: bool,
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
                "wl_shm" => st.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "hyprland_toplevel_export_manager_v1" => {
                    st.manager = Some(registry.bind(name, version.min(1), qh, ()));
                }
                _ => {}
            }
        }
    }
}

macro_rules! ignore {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(),
                _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore!(
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    HyprlandToplevelExportManagerV1
);

impl Dispatch<HyprlandToplevelExportFrameV1, ()> for State {
    fn event(
        st: &mut Self,
        _: &HyprlandToplevelExportFrameV1,
        event: frame::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            frame::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } => {
                if matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888) {
                    st.offer = Some((format, width, height, stride));
                }
            }
            frame::Event::BufferDone => st.buffer_done = true,
            frame::Event::Ready { .. } => st.ready = true,
            frame::Event::Failed => st.failed = true,
            _ => {}
        }
    }
}

/// One picture of each job's window, already cropped, scaled and rounded to its size (the same
/// one-pass scaling as the live pictures). Blocking: run it off the UI thread.
pub fn snapshot(jobs: &[Job]) -> Vec<Frame> {
    let Ok(conn) = Connection::connect_to_env() else {
        return Vec::new();
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut st = State::default();
    if queue.roundtrip(&mut st).is_err() {
        return Vec::new();
    }
    let (Some(shm), Some(manager)) = (st.shm.clone(), st.manager.clone()) else {
        log::warn!("previews: the compositor has no toplevel export");
        return Vec::new();
    };
    jobs.iter()
        .filter_map(|job| {
            let h = handle(&job.address)?;
            let rgba = one(&conn, &mut queue, &qh, &mut st, &shm, &manager, h, job)?;
            Some(Frame {
                address: job.address.clone(),
                width: job.width.max(1),
                height: job.height.max(1),
                rgba,
            })
        })
        .collect()
}

/// The window handle the protocol wants: the address's low 32 bits.
fn handle(address: &str) -> Option<u32> {
    u64::from_str_radix(address.trim_start_matches("0x"), 16)
        .ok()
        .map(|v| v as u32)
}

fn one(
    conn: &Connection,
    queue: &mut wayland_client::EventQueue<State>,
    qh: &QueueHandle<State>,
    st: &mut State,
    shm: &wl_shm::WlShm,
    manager: &HyprlandToplevelExportManagerV1,
    handle: u32,
    job: &Job,
) -> Option<Vec<u8>> {
    st.offer = None;
    st.buffer_done = false;
    st.ready = false;
    st.failed = false;
    let frame = manager.capture_toplevel(0, handle, qh, ());
    while !st.buffer_done && !st.failed {
        queue.blocking_dispatch(st).ok()?;
    }
    let result = (|| {
        let (format, w, h, stride) = st.offer.filter(|_| !st.failed)?;
        let size = (stride * h) as usize;
        // Safety: a fresh anonymous memory file, owned once below.
        let fd = unsafe { libc::memfd_create(c"kestrel-preview".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(std::os::fd::AsRawFd::as_raw_fd(&fd), size as libc::off_t) }
            != 0
        {
            return None;
        }
        let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, qh, ());
        frame.copy(&buffer, 1);
        while !st.ready && !st.failed {
            queue.blocking_dispatch(st).ok()?;
        }
        let out = if st.ready {
            read(&fd, size, (format, w, h, stride), job)
        } else {
            None
        };
        buffer.destroy();
        pool.destroy();
        out
    })();
    frame.destroy();
    let _ = conn.flush();
    result
}

/// The copied window (BGRA in shared memory) as the job's picture.
fn read(
    fd: &OwnedFd,
    size: usize,
    key: (wl_shm::Format, u32, u32, u32),
    job: &Job,
) -> Option<Vec<u8>> {
    use std::os::fd::AsRawFd;
    // Safety: maps the memory file just filled by the compositor, read-only, unmapped below.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        return None;
    }
    let src = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) };
    let mask = corner_mask(job.width.max(1), job.height.max(1), job.radius as f32);
    let out = compose(src, key, job, &mask);
    unsafe {
        libc::munmap(ptr, size);
    }
    Some(out)
}

/// A window to keep live: its address and the picture's size on screen (pixels), with the
/// corner radius it is rounded to.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Job {
    pub address: String,
    pub width: u32,
    pub height: u32,
    pub radius: u32,
}

/// A fresh picture of a window, already cropped, scaled and rounded to its job's size.
#[derive(Clone, Debug)]
pub struct Frame {
    pub address: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One window being followed: its buffer (kept and reused while the window keeps its size).
struct Slot {
    job: Job,
    handle: u32,
    frame: Option<HyprlandToplevelExportFrameV1>,
    offer: Option<(wl_shm::Format, u32, u32, u32)>,
    buffer: Option<Shared>,
    ready: bool,
    failed: bool,
    /// When the next picture may be asked for.
    next: std::time::Instant,
    /// Rounded-corner coverage for the job's size (0..=255 per pixel).
    mask: Vec<u8>,
}

struct Shared {
    key: (wl_shm::Format, u32, u32, u32),
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    ptr: *mut libc::c_void,
    size: usize,
    _fd: OwnedFd,
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
        // Safety: mapped in `Shared::new`, unmapped once here.
        unsafe {
            libc::munmap(self.ptr, self.size);
        }
    }
}

impl Shared {
    fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<Live>,
        key: (wl_shm::Format, u32, u32, u32),
    ) -> Option<Self> {
        use std::os::fd::AsRawFd;
        let (format, w, h, stride) = key;
        let size = (stride * h) as usize;
        // Safety: a fresh anonymous memory file, owned at once; mapped for reading only.
        let fd = unsafe { libc::memfd_create(c"kestrel-live".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(fd.as_raw_fd(), size as libc::off_t) } != 0 {
            return None;
        }
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, qh, ());
        Some(Self {
            key,
            pool,
            buffer,
            ptr,
            size,
            _fd: fd,
        })
    }
}

#[derive(Default)]
struct Live {
    shm: Option<wl_shm::WlShm>,
    manager: Option<HyprlandToplevelExportManagerV1>,
    slots: Vec<Slot>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Live {
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
                "wl_shm" => st.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "hyprland_toplevel_export_manager_v1" => {
                    st.manager = Some(registry.bind(name, version.min(1), qh, ()));
                }
                _ => {}
            }
        }
    }
}

macro_rules! ignore_live {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for Live {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(),
                _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore_live!(
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    HyprlandToplevelExportManagerV1
);

impl Dispatch<HyprlandToplevelExportFrameV1, usize> for Live {
    fn event(
        st: &mut Self,
        f: &HyprlandToplevelExportFrameV1,
        event: frame::Event,
        k: &usize,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(shm) = st.shm.clone() else { return };
        let Some(slot) = st.slots.get_mut(*k) else {
            return;
        };
        match event {
            frame::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } => {
                if matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888) {
                    slot.offer = Some((format, width, height, stride));
                }
            }
            frame::Event::BufferDone => {
                let Some(key) = slot.offer else {
                    slot.failed = true;
                    return;
                };
                if slot.buffer.as_ref().is_none_or(|b| b.key != key) {
                    slot.buffer = Shared::new(&shm, qh, key);
                }
                match &slot.buffer {
                    // Waits for the window to change: a still window costs nothing.
                    Some(b) => f.copy(&b.buffer, 0),
                    None => slot.failed = true,
                }
            }
            frame::Event::Ready { .. } => slot.ready = true,
            frame::Event::Failed => slot.failed = true,
            _ => {}
        }
    }
}

/// Coverage of a `w`×`h` rounded rectangle with corner radius `r`, per pixel.
fn corner_mask(w: u32, h: u32, r: f32) -> Vec<u8> {
    let r = r.min(w as f32 / 2.0).min(h as f32 / 2.0);
    let mut m = vec![255u8; (w * h) as usize];
    if r <= 0.5 {
        return m;
    }
    for y in 0..h {
        for x in 0..w {
            let cx = (x as f32 + 0.5).clamp(r, w as f32 - r);
            let cy = (y as f32 + 0.5).clamp(r, h as f32 - r);
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            m[(y * w + x) as usize] = ((r - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    m
}

/// The window's picture in shared memory (BGRA rows), cropped to the target's shape (centred),
/// scaled down by averaging a small grid of samples per pixel, and rounded: straight from the
/// compositor's buffer to the final picture in one pass.
fn compose(src: &[u8], key: (wl_shm::Format, u32, u32, u32), job: &Job, mask: &[u8]) -> Vec<u8> {
    let (format, sw, sh, stride) = key;
    let alpha = format == wl_shm::Format::Argb8888;
    let (tw, th) = (job.width.max(1), job.height.max(1));
    let (cw, ch) = if u64::from(sw) * u64::from(th) > u64::from(sh) * u64::from(tw) {
        ((u64::from(sh) * u64::from(tw) / u64::from(th)) as u32, sh)
    } else {
        (sw, (u64::from(sw) * u64::from(th) / u64::from(tw)) as u32)
    };
    let (cw, ch) = (cw.clamp(1, sw.max(1)), ch.clamp(1, sh.max(1)));
    let (ox, oy) = ((sw - cw) / 2, (sh - ch) / 2);
    let (fx, fy) = (cw as f32 / tw as f32, ch as f32 / th as f32);
    // Samples per output pixel: up to 4 across, 2 down. Every sampled source row is a full
    // trip through memory (the buffer is fresh from the compositor, never in cache), so rows
    // are the cost; columns in the same rows are nearly free.
    let nx = (fx.ceil() as usize).clamp(1, 4);
    let ny = (fy.ceil() as usize).clamp(1, 2);
    // Where every sample falls, worked out once: byte offsets of the sampled columns and rows.
    let place = |o: u32, f: f32, len: u32, k: u32, i: usize, n: usize| {
        o + (((k as f32 + (i as f32 + 0.5) / n as f32) * f) as u32).min(len - 1)
    };
    let cols: Vec<usize> = (0..tw)
        .flat_map(|x| (0..nx).map(move |i| place(ox, fx, cw, x, i, nx) as usize * 4))
        .collect();
    let rows: Vec<usize> = (0..th)
        .flat_map(|y| (0..ny).map(move |j| place(oy, fy, ch, y, j, ny) as usize * stride as usize))
        .collect();
    let need =
        rows.iter().max().copied().unwrap_or(0) + cols.iter().max().copied().unwrap_or(0) + 4;
    if src.len() < need {
        return vec![0; (tw * th * 4) as usize];
    }
    let c = (nx * ny) as u32;
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for (y, ry) in rows.chunks_exact(ny).enumerate() {
        let line = &mut out[y * tw as usize * 4..(y + 1) * tw as usize * 4];
        for (x, (px, cx)) in line
            .chunks_exact_mut(4)
            .zip(cols.chunks_exact(nx))
            .enumerate()
        {
            let (mut b, mut g, mut r, mut a) = (0u32, 0u32, 0u32, 0u32);
            for &row in ry {
                for &col in cx {
                    let p = &src[row + col..row + col + 4];
                    b += u32::from(p[0]);
                    g += u32::from(p[1]);
                    r += u32::from(p[2]);
                    a += u32::from(p[3]);
                }
            }
            let a = if alpha { a / c } else { 255 };
            let cover = u32::from(mask.get(y * tw as usize + x).copied().unwrap_or(255));
            px.copy_from_slice(&[
                (r / c) as u8,
                (g / c) as u8,
                (b / c) as u8,
                (a * cover / 255) as u8,
            ]);
        }
    }
    out
}

/// Fastest a window's picture is renewed.
const FRAME: std::time::Duration = std::time::Duration::from_millis(33);

/// Follows `jobs`' windows while the receiver lives: each time a window changes (at most
/// about 30 times a second) its new picture is sent. Blocking: runs on its own thread.
fn follow(jobs: Vec<Job>, tx: &futures::channel::mpsc::UnboundedSender<Vec<Frame>>) -> Option<()> {
    use std::os::fd::AsRawFd;
    use std::time::Instant;
    let conn = Connection::connect_to_env().ok()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut st = Live::default();
    queue.roundtrip(&mut st).ok()?;
    let manager = st.manager.clone()?;
    st.slots = jobs
        .into_iter()
        .filter_map(|job| {
            let handle = handle(&job.address)?;
            Some(Slot {
                mask: corner_mask(job.width, job.height, job.radius as f32),
                job,
                handle,
                frame: None,
                offer: None,
                buffer: None,
                ready: false,
                failed: false,
                next: Instant::now(),
            })
        })
        .collect();
    let mut batch: Vec<Frame> = Vec::new();
    loop {
        if tx.is_closed() {
            return Some(());
        }
        let now = Instant::now();
        for (k, slot) in st.slots.iter_mut().enumerate() {
            if slot.frame.is_none() && slot.next <= now {
                slot.offer = None;
                slot.ready = false;
                slot.failed = false;
                slot.frame = Some(manager.capture_toplevel(0, slot.handle, &qh, k));
            }
        }
        conn.flush().ok()?;
        // Sleep until the compositor speaks or the next picture is due (never long, so a
        // closed overview is noticed at once).
        let due = st
            .slots
            .iter()
            .filter(|s| s.frame.is_none())
            .map(|s| s.next)
            .min();
        let wait = due
            .map_or(100, |d| {
                d.saturating_duration_since(Instant::now()).as_millis() as i32
            })
            .clamp(0, 100);
        if let Some(guard) = queue.prepare_read() {
            let mut pfd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // Safety: one valid pollfd.
            let n = unsafe { libc::poll(&mut pfd, 1, wait) };
            if n > 0 {
                guard.read().ok()?;
            }
        }
        queue.dispatch_pending(&mut st).ok()?;
        let now = Instant::now();
        for slot in &mut st.slots {
            if slot.ready {
                if let (Some(b), Some(key)) = (&slot.buffer, slot.offer) {
                    // Safety: the mapping lives as long as `b`; the compositor finished writing.
                    let src = unsafe { std::slice::from_raw_parts(b.ptr as *const u8, b.size) };
                    let rgba = compose(src, key, &slot.job, &slot.mask);
                    batch.retain(|f| f.address != slot.job.address);
                    batch.push(Frame {
                        address: slot.job.address.clone(),
                        width: slot.job.width.max(1),
                        height: slot.job.height.max(1),
                        rgba,
                    });
                }
                slot.next = now + FRAME;
            } else if slot.failed {
                // Gone or not capturable right now: try again a little later.
                slot.next = now + std::time::Duration::from_millis(500);
            } else {
                continue;
            }
            if let Some(f) = slot.frame.take() {
                f.destroy();
            }
            slot.ready = false;
            slot.failed = false;
        }
        if !batch.is_empty() && tx.unbounded_send(std::mem::take(&mut batch)).is_err() {
            return Some(());
        }
    }
}

/// Live pictures of `jobs`' windows, as they change, while the stream is polled.
pub fn live(jobs: Vec<Job>) -> impl futures::Stream<Item = Vec<Frame>> {
    crate::stream::channel(2, async move |mut out| {
        use futures::{SinkExt, StreamExt};
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("kestrel-previews".into())
            .spawn(move || {
                let _ = follow(jobs, &tx);
            });
        if spawned.is_err() {
            return;
        }
        while let Some(mut v) = rx.next().await {
            // Behind: take everything waiting and keep each window's newest picture only.
            while let Ok(more) = rx.try_recv() {
                for f in more {
                    v.retain(|o| o.address != f.address);
                    v.push(f);
                }
            }
            if out.send(v).await.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn handles_are_the_low_32_bits() {
        assert_eq!(super::handle("0x55d1a2b3c4d5"), Some(0xa2b3_c4d5));
        assert_eq!(super::handle("nonsense"), None);
    }
}

#[cfg(test)]
mod bench {
    #[test]
    #[ignore]
    fn compose_speed() {
        let key = (super::wl_shm::Format::Xrgb8888, 1920, 1080, 1920 * 4);
        let src: Vec<u8> = (0..1920 * 1080 * 4).map(|i| (i % 251) as u8).collect();
        let job = super::Job {
            address: String::new(),
            width: 236,
            height: 126,
            radius: 7,
        };
        let mask = super::corner_mask(236, 126, 7.0);
        let t = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(super::compose(&src, key, &job, &mask));
        }
        eprintln!("compose: {} us per frame", t.elapsed().as_micros() / 100);
    }
}
