//! What every supervised service is doing right now: one row per running `stream::forever`,
//! added when it starts and removed when its subscription is dropped (a switched-off module
//! leaves no row). `kestrel health` prints it.
//!
//! One short lock per state change; nothing here waits, polls or runs on its own.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Its stream is up.
    Running,
    /// It ended or panicked and is made again after a short wait.
    Restarting,
    /// It failed more often than its policy allows; it is tried again only after a long wait.
    Failed,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Restarting => "restarting",
            State::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    /// The stable name the service was started under ("network", "audio", …).
    pub name: &'static str,
    pub state: State,
    /// Times it was made again since it started.
    pub restarts: u32,
    /// Why it last stopped (a panic's message, or that it ended without a reading), and when.
    pub error: Option<(String, SystemTime)>,
}

static TABLE: Mutex<Vec<(u64, Entry)>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

fn table_lock() -> std::sync::MutexGuard<'static, Vec<(u64, Entry)>> {
    // A panic never happens while the lock is held (only plain field writes), but a poisoned
    // lock must not take the health table down with it.
    TABLE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One service's row; the row goes when this is dropped.
pub(crate) struct Handle(u64);

impl Handle {
    pub(crate) fn new(name: &'static str) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        table_lock().push((
            id,
            Entry {
                name,
                state: State::Running,
                restarts: 0,
                error: None,
            },
        ));
        Handle(id)
    }

    pub(crate) fn update(&self, f: impl FnOnce(&mut Entry)) {
        if let Some((_, e)) = table_lock().iter_mut().find(|(id, _)| *id == self.0) {
            f(e);
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        table_lock().retain(|(id, _)| *id != self.0);
    }
}

/// Every supervised service now, by name.
pub fn snapshot() -> Vec<Entry> {
    let mut v: Vec<Entry> = table_lock().iter().map(|(_, e)| e.clone()).collect();
    v.sort_by_key(|e| e.name);
    v
}

/// The table `kestrel health` prints: name, state, restarts, last error and its time.
pub fn table() -> String {
    let rows = snapshot();
    let width = rows.iter().map(|e| e.name.len()).max().unwrap_or(0).max(7);
    let mut s = format!(
        "{:width$}  {:10}  {:>8}  last error\n",
        "service", "state", "restarts"
    );
    for e in &rows {
        let error = e.error.as_ref().map_or_else(String::new, |(why, at)| {
            let at: chrono::DateTime<chrono::Local> = (*at).into();
            format!("{} {why}", at.format("%H:%M:%S"))
        });
        let line = format!(
            "{:width$}  {:10}  {:>8}  {error}",
            e.name,
            e.state.label(),
            e.restarts
        );
        s.push_str(line.trim_end());
        s.push('\n');
    }
    if rows.is_empty() {
        s.push_str("no services running\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_lives_as_long_as_its_handle() {
        let h = Handle::new("test-row");
        h.update(|e| {
            e.state = State::Restarting;
            e.restarts = 2;
            e.error = Some(("gone".into(), SystemTime::now()));
        });
        let e = snapshot().into_iter().find(|e| e.name == "test-row");
        let e = e.expect("the row is there");
        assert_eq!((e.state, e.restarts), (State::Restarting, 2));
        assert!(
            table().lines().any(|l| l.starts_with("test-row")
                && l.contains("restarting")
                && l.ends_with("gone"))
        );
        drop(h);
        assert!(!snapshot().iter().any(|e| e.name == "test-row"));
    }
}
