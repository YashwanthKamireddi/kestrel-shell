//! Fault isolation: a module or panel that panics while drawing is switched off for the rest of
//! the session (it then draws nothing), the failure is logged, and the shell says so once in a
//! notification. Everything else keeps working. Release builds unwind for this (Cargo.toml).

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

/// Parts that failed, and failures not yet reported.
static BROKEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
static UNREPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Runs `f` for the part `name`; a panic switches the part off and gives `fallback()` instead.
pub fn run<T>(name: &str, f: impl FnOnce() -> T, fallback: impl FnOnce() -> T) -> T {
    if BROKEN.lock().is_ok_and(|b| b.contains(name)) {
        return fallback();
    }
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(e) => {
            let why = e
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown error".into());
            log::error!("{name} failed and is switched off until kestrel restarts: {why}");
            if let Ok(mut b) = BROKEN.lock() {
                b.insert(name.to_string());
            }
            if let Ok(mut u) = UNREPORTED.lock() {
                u.push(format!("{name}: {why}"));
            }
            fallback()
        }
    }
}

/// Runs `f` (one update: an event being handled); a panic drops just that update and keeps the
/// shell up, reporting each distinct failure once. Nothing is switched off: a part of the state
/// that stopped updating would be worse than one missed event. Before this, a panic in an
/// update ended kestrel, and systemd restarted it into the same panic while its cause lasted
/// (76 times in a minute on 2026-11-15).
pub fn isolate<T>(name: &str, f: impl FnOnce() -> T, fallback: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(e) => {
            let why = e
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown error".into());
            log::error!("{name} failed (that one event was dropped): {why}");
            let key = format!("{name}: {why}");
            if BROKEN.lock().is_ok_and(|mut b| b.insert(key.clone()))
                && let Ok(mut u) = UNREPORTED.lock()
            {
                u.push(key);
            }
            fallback()
        }
    }
}

/// Failures not yet shown to the user (each once).
pub fn take_unreported() -> Vec<String> {
    UNREPORTED
        .lock()
        .map(|mut u| std::mem::take(&mut *u))
        .unwrap_or_default()
}

/// An update slower than this delays the frame after it (8 ms is half a frame at 60 Hz).
pub const SLOW_UPDATE: std::time::Duration = std::time::Duration::from_millis(8);

/// A message's name as its `Debug` form starts: its variant and the variants it wraps
/// ("Panel(Files(Listed"). Formatting stops at the first character that is not part of a
/// name, so a message carrying a long list costs no more than one carrying nothing, and
/// nothing is allocated.
pub struct Kind {
    buf: [u8; 48],
    len: usize,
}

impl Kind {
    pub fn of(msg: &impl std::fmt::Debug) -> Self {
        use std::fmt::Write;
        let mut k = Self {
            buf: [0; 48],
            len: 0,
        };
        // The writer refuses (an error) where the name ends, which ends the formatting too.
        let _ = write!(k, "{msg:?}");
        while k.len > 0 && k.buf[k.len - 1] == b'(' {
            k.len -= 1;
        }
        k
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl std::fmt::Write for Kind {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            let name = b.is_ascii_alphanumeric() || b == b'_' || b == b'(';
            if !name || self.len == self.buf.len() {
                return Err(std::fmt::Error);
            }
            self.buf[self.len] = b;
            self.len += 1;
        }
        Ok(())
    }
}

/// When each kind of message was last reported slow: a handler that is slow every time is
/// reported once a minute, not on every event.
#[derive(Default)]
pub struct SlowLog {
    last: std::collections::HashMap<String, std::time::Instant>,
}

impl SlowLog {
    const EVERY: std::time::Duration = std::time::Duration::from_secs(60);

    /// Whether an update of `kind` that took `took` is reported now.
    pub fn report(
        &mut self,
        kind: &str,
        took: std::time::Duration,
        now: std::time::Instant,
    ) -> bool {
        if took <= SLOW_UPDATE {
            return false;
        }
        if self
            .last
            .get(kind)
            .is_some_and(|t| now.saturating_duration_since(*t) < Self::EVERY)
        {
            return false;
        }
        self.last.insert(kind.to_string(), now);
        true
    }
}

/// Runs one update and measures it: one that kept the UI thread longer than `SLOW_UPDATE` is
/// logged with its message's kind (at most once a minute per kind), so slow handlers show in
/// the journal. Costs two clock reads; the log is only touched by a slow update.
pub fn timed<T>(kind: &Kind, f: impl FnOnce() -> T) -> T {
    static LOG: Mutex<Option<SlowLog>> = Mutex::new(None);
    let start = std::time::Instant::now();
    let v = f();
    let took = start.elapsed();
    if took > SLOW_UPDATE
        && let Ok(mut log) = LOG.lock()
        && log.get_or_insert_with(SlowLog::default).report(
            kind.as_str(),
            took,
            std::time::Instant::now(),
        )
    {
        log::warn!(
            "slow update: {} kept the UI thread {:.1} ms",
            kind.as_str(),
            took.as_secs_f64() * 1000.0
        );
    }
    v
}

#[cfg(test)]
mod tests {
    #[derive(Debug)]
    #[allow(dead_code)]
    enum Inner {
        Listed(Vec<String>),
        Plain,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    enum Outer {
        Panel(Inner),
        Minute,
        Moved { x: i32 },
        Text(String),
    }

    #[test]
    fn a_message_kind_is_its_variants() {
        use super::Kind;
        let big = Outer::Panel(Inner::Listed(vec!["x".repeat(10_000); 100]));
        assert_eq!(Kind::of(&big).as_str(), "Panel(Listed");
        assert_eq!(
            Kind::of(&Outer::Panel(Inner::Plain)).as_str(),
            "Panel(Plain"
        );
        assert_eq!(Kind::of(&Outer::Minute).as_str(), "Minute");
        assert_eq!(Kind::of(&Outer::Moved { x: 3 }).as_str(), "Moved");
        assert_eq!(Kind::of(&Outer::Text("hi".into())).as_str(), "Text");
    }

    #[test]
    fn a_slow_kind_is_reported_once_a_minute() {
        use std::time::{Duration, Instant};
        let mut log = super::SlowLog::default();
        let t0 = Instant::now();
        let slow = Duration::from_millis(20);
        // Fast updates never are.
        assert!(!log.report("Hypr", Duration::from_millis(3), t0));
        assert!(!log.report("Hypr", super::SLOW_UPDATE, t0));
        assert!(log.report("Hypr", slow, t0));
        // The same kind within the minute: quiet; another kind: reported.
        assert!(!log.report("Hypr", slow, t0 + Duration::from_secs(30)));
        assert!(log.report("Media", slow, t0 + Duration::from_secs(30)));
        // A minute on: reported again.
        assert!(log.report("Hypr", slow, t0 + Duration::from_secs(61)));
    }

    #[test]
    fn a_panicking_part_is_switched_off() {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let v = super::run("test-part", || -> i32 { panic!("boom") }, || 7);
        std::panic::set_hook(hook);
        assert_eq!(v, 7);
        // Switched off: not even called again.
        assert_eq!(super::run("test-part", || 1, || 8), 8);
        assert!(super::take_unreported().iter().any(|m| m.contains("boom")));

        // An update: dropped, reported once, not switched off. (One test: both share the
        // report list, and parallel tests would take each other's reports.)
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let a = super::isolate("test-update", || -> i32 { panic!("bad event") }, || 0);
        let b = super::isolate("test-update", || -> i32 { panic!("bad event") }, || 0);
        std::panic::set_hook(hook);
        assert_eq!((a, b), (0, 0));
        // Not switched off: the next one runs.
        assert_eq!(super::isolate("test-update", || 5, || 0), 5);
        let r = super::take_unreported();
        assert_eq!(r.iter().filter(|m| m.contains("bad event")).count(), 1);
    }
}
