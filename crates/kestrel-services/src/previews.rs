//! Live window previews: a picture of any window, on any workspace, for the overview. Uses
//! Hyprland's toplevel export protocol (the one its portal uses to share a window) on its own
//! Wayland connection, copying each window into shared memory once and scaling it down.
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

/// A window's picture, RGBA, at most the requested width.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

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

/// Pictures of the windows at `addresses` (Hyprland's "0x…" window addresses), each at most
/// `max_width` wide. Blocking: run it off the UI thread.
pub fn capture(addresses: &[String], max_width: u32) -> Vec<(String, Option<Preview>)> {
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
    addresses
        .iter()
        .map(|a| {
            let p = handle(a).and_then(|h| one(&conn, &mut queue, &qh, &mut st, &shm, &manager, h));
            (a.clone(), p.map(|p| shrink(p, max_width)))
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
) -> Option<Preview> {
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
            read(&fd, size, w, h, stride, format == wl_shm::Format::Argb8888)
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

/// The copied pixels (BGRA in memory) as RGBA.
fn read(fd: &OwnedFd, size: usize, w: u32, h: u32, stride: u32, alpha: bool) -> Option<Preview> {
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
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let row = &src[y * stride as usize..y * stride as usize + w as usize * 4];
        for px in row.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], if alpha { px[3] } else { 255 }]);
        }
    }
    unsafe {
        libc::munmap(ptr, size);
    }
    Some(Preview {
        width: w,
        height: h,
        rgba,
    })
}

fn shrink(p: Preview, max_width: u32) -> Preview {
    if p.width <= max_width {
        return p;
    }
    let Some(img) = image::RgbaImage::from_raw(p.width, p.height, p.rgba) else {
        return Preview {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        };
    };
    let h = (u64::from(p.height) * u64::from(max_width) / u64::from(p.width)).max(1) as u32;
    let small = image::imageops::resize(&img, max_width, h, image::imageops::FilterType::Triangle);
    Preview {
        width: small.width(),
        height: small.height(),
        rgba: small.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn handles_are_the_low_32_bits() {
        assert_eq!(super::handle("0x55d1a2b3c4d5"), Some(0xa2b3_c4d5));
        assert_eq!(super::handle("nonsense"), None);
    }
}
