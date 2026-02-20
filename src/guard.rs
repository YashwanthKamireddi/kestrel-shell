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

/// Failures not yet shown to the user (each once).
pub fn take_unreported() -> Vec<String> {
    UNREPORTED
        .lock()
        .map(|mut u| std::mem::take(&mut *u))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
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
    }
}
