//! systemd's watchdog: the UI loop says "still alive" every few seconds, and systemd restarts
//! kestrel if it stops (a hang). A crash already restarts it (Restart=on-failure); this covers
//! a freeze, which would otherwise leave the desktop without a working shell
//! (docs/ARCHITECTURE.md, Stability). The ping is sent from the update loop itself, so a stuck UI
//! thread stops it. Nothing at all when not started by systemd with a watchdog.

use std::os::unix::net::UnixDatagram;
use std::time::Duration;

/// How often to ping: half of systemd's watchdog interval, or None when there is no watchdog.
pub fn interval() -> Option<Duration> {
    let usec: u64 = std::env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    if let Ok(pid) = std::env::var("WATCHDOG_PID")
        && pid.parse::<u32>().ok() != Some(std::process::id())
    {
        return None;
    }
    Some(Duration::from_micros(usec / 2))
}

/// Sends `state` ("READY=1", "WATCHDOG=1") to systemd's notify socket, if there is one.
pub fn notify(state: &str) {
    let Ok(path) = std::env::var("NOTIFY_SOCKET") else {
        return;
    };
    let Ok(sock) = UnixDatagram::unbound() else {
        return;
    };
    if let Some(abs) = path.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(addr) = std::os::unix::net::SocketAddr::from_abstract_name(abs.as_bytes()) {
            let _ = sock.send_to_addr(state.as_bytes(), &addr);
        }
    } else {
        let _ = sock.send_to(state.as_bytes(), path);
    }
}
