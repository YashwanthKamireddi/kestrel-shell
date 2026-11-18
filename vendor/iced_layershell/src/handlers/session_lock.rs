//! kestrel patch: the session lock (ext-session-lock-v1), for a lock screen.
//!
//! The protocol's guarantees are what make a lock screen safe: once the compositor has sent
//! `locked`, the session stays locked until this client sends `unlock_and_destroy`. If the
//! client crashes or disconnects first, the compositor keeps everything hidden (it does not
//! unlock). So nothing here ever unlocks on its own: only `unlock_session()` does, and only
//! after `locked` has been received.
//!
//! The app asks for the lock (`lock_session()`), makes one lock surface per output
//! (`new_lock_surface(output)`), hears `Locked` / `Finished` through `session_lock_events()`
//! and finally calls `unlock_session()`. Lock surfaces are never committed without a buffer
//! (a protocol error that would drop the client and leave the session locked).
//!
//! Protocol: <https://wayland.app/protocols/ext-session-lock-v1>.

use futures::channel::mpsc;
use iced_futures::Subscription;
use smithay_client_toolkit::delegate_session_lock;
use smithay_client_toolkit::session_lock::{
    SessionLock, SessionLockHandler, SessionLockSurface, SessionLockSurfaceConfigure,
};
use std::sync::{Mutex, OnceLock};
use wayland_client::{Connection, QueueHandle};

use crate::state::WaylandState;

/// What the compositor says about the lock this client asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockEvent {
    /// The session is locked: every output shows lock surfaces (or the compositor's own
    /// blank), and nothing else gets input until this client unlocks.
    Locked,
    /// The lock is over without this client unlocking: the compositor refused it (another
    /// locker holds the session, or it has no session lock at all) or ended it itself.
    Finished,
}

type Channel = (
    mpsc::UnboundedSender<LockEvent>,
    Mutex<Option<mpsc::UnboundedReceiver<LockEvent>>>,
);

static CHANNEL: OnceLock<Channel> = OnceLock::new();

fn channel() -> &'static Channel {
    CHANNEL.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded();
        (tx, Mutex::new(Some(rx)))
    })
}

pub(crate) fn push(event: LockEvent) {
    channel().0.unbounded_send(event).ok();
}

/// The lock's events. Events that arrive before the subscription starts are kept.
pub fn session_lock_events() -> Subscription<LockEvent> {
    Subscription::run(|| {
        let rx = channel().1.lock().ok().and_then(|mut g| g.take());
        match rx {
            Some(rx) => futures::stream::StreamExt::left_stream(rx),
            None => futures::stream::StreamExt::right_stream(futures::stream::pending()),
        }
    })
}

impl SessionLockHandler for WaylandState {
    fn locked(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _lock: SessionLock) {
        crate::ktrace!("session locked");
        push(LockEvent::Locked);
    }

    fn finished(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _lock: SessionLock) {
        crate::ktrace!("session lock finished");
        // The lock is inert now; destroying it is allowed (it is not locked).
        self.session_lock = None;
        let lock_surfaces: Vec<_> = self
            .surfaces
            .values()
            .filter(|d| d.role.is_lock())
            .map(|d| d.id)
            .collect();
        self.closed_surfaces.extend(lock_surfaces);
        push(LockEvent::Finished);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _serial: u32,
    ) {
        // SCTK has acknowledged it. Always a new frame at the new size, never a bare commit:
        // the buffer must match the configured size exactly.
        if let Some(data) = self.surfaces.get_mut(surface.wl_surface()) {
            let (w, h) = configure.new_size;
            crate::ktrace!("configure lock {:?} {}x{}", data.id, w, h);
            if w > 0 && h > 0 {
                data.size = (w, h);
            }
            data.configured = true;
            self.surfaces_need_redraw.insert(data.id);
        }
    }
}

delegate_session_lock!(WaylandState);
