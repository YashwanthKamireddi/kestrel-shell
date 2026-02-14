//! AirPods' own battery and in-ear state. Linux sees AirPods as plain headphones (BlueZ shows
//! no battery for them), but they speak Apple's accessory protocol on a second L2CAP channel
//! (PSM 0x1001) of the existing connection: after a handshake they report each bud's and the
//! case's charge and whether each bud is in an ear. Nothing is paired or changed; kestrel only
//! listens. Packet layouts from LibrePods' notes (github.com/kavishdevar/librepods).

use futures::Stream;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

const PSM: u16 = 0x1001;
const HANDSHAKE: &[u8] = &[0, 0, 4, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const FEATURES: &[u8] = &[4, 0, 4, 0, 0x4d, 0, 0xd7, 0, 0, 0, 0, 0, 0, 0];
const NOTIFY: &[u8] = &[4, 0, 4, 0, 0x0f, 0, 0xff, 0xff, 0xff, 0xff, 0xff];
const HANDSHAKE_ACK: &[u8] = &[1, 0, 4, 0];
const BATTERY: &[u8] = &[4, 0, 4, 0, 4, 0];
const EAR: &[u8] = &[4, 0, 4, 0, 6, 0];

/// One part's charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Charge {
    pub level: u8,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    #[default]
    Unknown,
    Ear,
    Out,
    Case,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pods {
    pub left: Option<Charge>,
    pub right: Option<Charge>,
    /// The case reports only while a bud is in it.
    pub case: Option<Charge>,
    /// AirPods Max and other single-part headphones.
    pub headset: Option<Charge>,
    pub left_at: Place,
    pub right_at: Place,
}

impl Pods {
    /// The lowest bud charge, what the bar shows.
    pub fn level(&self) -> Option<u8> {
        [self.left, self.right, self.headset]
            .into_iter()
            .flatten()
            .map(|c| c.level)
            .min()
    }
    pub fn charging(&self) -> bool {
        [self.left, self.right, self.headset]
            .into_iter()
            .flatten()
            .any(|c| c.charging)
    }
}

/// Whether a BlueZ device is Apple audio gear (vendor 0x004C, Bluetooth SIG source).
pub fn is_airpods(modalias: &str, icon: &str) -> bool {
    modalias.to_ascii_lowercase().starts_with("bluetooth:v004c") && icon.starts_with("audio")
}

#[repr(C)]
struct SockaddrL2 {
    family: libc::sa_family_t,
    psm: u16,
    bdaddr: [u8; 6],
    cid: u16,
    bdaddr_type: u8,
}

fn connect(address: &str) -> Option<i32> {
    let mut bd = [0u8; 6];
    let parts: Vec<u8> = address
        .split(':')
        .filter_map(|p| u8::from_str_radix(p, 16).ok())
        .collect();
    if parts.len() != 6 {
        return None;
    }
    // BlueZ keeps addresses little-endian.
    for (i, b) in parts.iter().rev().enumerate() {
        bd[i] = *b;
    }
    let addr = SockaddrL2 {
        family: libc::AF_BLUETOOTH as libc::sa_family_t,
        psm: PSM.to_le(),
        bdaddr: bd,
        cid: 0,
        bdaddr_type: 0, // BR/EDR
    };
    // SAFETY: plain socket calls on a descriptor this function owns; `addr` outlives connect.
    unsafe {
        let fd = libc::socket(
            libc::AF_BLUETOOTH,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0, // BTPROTO_L2CAP
        );
        if fd < 0 {
            return None;
        }
        let r = libc::connect(
            fd,
            (&raw const addr).cast(),
            std::mem::size_of::<SockaddrL2>() as libc::socklen_t,
        );
        if r < 0 {
            libc::close(fd);
            return None;
        }
        Some(fd)
    }
}

fn send(fd: i32, b: &[u8]) -> bool {
    // SAFETY: writes from a live slice to an open descriptor.
    unsafe { libc::write(fd, b.as_ptr().cast(), b.len()) == b.len() as isize }
}

fn place(b: u8) -> Place {
    match b {
        0 => Place::Ear,
        1 => Place::Out,
        2 => Place::Case,
        _ => Place::Unknown,
    }
}

/// Reads one connection until it ends, sending every change.
fn session(fd: i32, out: &futures::channel::mpsc::UnboundedSender<Option<Pods>>) {
    let mut pods = Pods::default();
    let mut seen = false;
    // Which bud is the primary (ear packets list primary first).
    let mut primary_left = true;
    if !send(fd, HANDSHAKE) {
        return;
    }
    let mut buf = [0u8; 2048];
    loop {
        // SAFETY: reads into a live buffer of the given length.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            return;
        }
        let d = &buf[..n as usize];
        let before = pods.clone();
        if d.starts_with(HANDSHAKE_ACK) {
            if !(send(fd, FEATURES) && send(fd, NOTIFY)) {
                return;
            }
        } else if d.starts_with(BATTERY) && d.len() >= 7 {
            let count = d[6] as usize;
            let mut first = true;
            for part in d[7..].as_chunks::<5>().0.iter().take(count) {
                let charge = (part[3] != 4).then_some(Charge {
                    level: part[2].min(100),
                    charging: part[3] == 1,
                });
                match part[0] {
                    1 => pods.headset = charge,
                    2 => pods.right = charge,
                    4 => pods.left = charge,
                    8 => pods.case = charge,
                    _ => {}
                }
                if first && matches!(part[0], 2 | 4) && charge.is_some() {
                    primary_left = part[0] == 4;
                    first = false;
                }
            }
            seen = true;
        } else if d.starts_with(EAR) && d.len() >= 8 {
            let (p, s) = (place(d[6]), place(d[7]));
            (pods.left_at, pods.right_at) = if primary_left { (p, s) } else { (s, p) };
        }
        if seen
            && (pods != before || d.starts_with(BATTERY))
            && out.unbounded_send(Some(pods.clone())).is_err()
        {
            return;
        }
    }
}

/// Shuts the socket when the stream is dropped, which ends the blocked read.
struct Guard(Arc<AtomicI32>);
impl Drop for Guard {
    fn drop(&mut self) {
        let fd = self.0.swap(-1, Ordering::SeqCst);
        if fd >= 0 {
            // SAFETY: shutdown on a descriptor the reader thread still owns; it closes it.
            unsafe { libc::shutdown(fd, libc::SHUT_RDWR) };
        }
    }
}

/// The AirPods at `address` (connected): their state on every change, `None` when it is lost.
/// Retries quietly while they stay connected.
#[allow(clippy::ptr_arg)] // iced's Subscription::run_with hands the builder `&String`.
pub fn stream(address: &String) -> impl Stream<Item = Option<Pods>> + use<> {
    let address = address.clone();
    crate::stream::channel(4, async move |mut out| {
        use futures::{SinkExt, StreamExt};
        let shared = Arc::new(AtomicI32::new(-1));
        let _guard = Guard(shared.clone());
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let fd_slot = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("kestrel-airpods".into())
            .spawn(move || {
                let mut wait = Duration::from_secs(2);
                loop {
                    if let Some(fd) = connect(&address) {
                        // Stopped while connecting.
                        if fd_slot
                            .compare_exchange(-1, fd, Ordering::SeqCst, Ordering::SeqCst)
                            .is_err()
                        {
                            // SAFETY: closing the descriptor just opened here.
                            unsafe { libc::close(fd) };
                            return;
                        }
                        session(fd, &tx);
                        let mine = fd_slot
                            .compare_exchange(fd, -1, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok();
                        // SAFETY: this thread opened fd and is its only closer.
                        unsafe { libc::close(fd) };
                        if !mine || tx.unbounded_send(None).is_err() {
                            return; // The stream is gone.
                        }
                        wait = Duration::from_secs(2);
                    }
                    std::thread::sleep(wait);
                    wait = (wait * 2).min(Duration::from_secs(15));
                    if tx.is_closed() {
                        return;
                    }
                }
            });
        if spawned.is_err() {
            return;
        }
        while let Some(v) = rx.next().await {
            if out.send(v).await.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn apple_audio_only() {
        assert!(super::is_airpods(
            "bluetooth:v004Cp2019d242F",
            "audio-headphones"
        ));
        assert!(!super::is_airpods(
            "bluetooth:v004Cp2019d242F",
            "input-keyboard"
        ));
        assert!(!super::is_airpods("bluetooth:v0001p0001", "audio-headset"));
    }
}
