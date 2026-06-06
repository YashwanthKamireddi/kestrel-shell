//! Heap tracer (feature `memtrace`): every live allocation of 16 KiB or more is recorded with
//! where it was made; `kestrel memdump` writes them grouped by place, largest first, to
//! $XDG_RUNTIME_DIR/kestrel-memdump.txt. Smaller allocations are counted, not traced.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const MIN: usize = 16 * 1024;

pub struct Tracer;

#[global_allocator]
static GLOBAL: Tracer = Tracer;

thread_local! {
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

static SMALL: AtomicUsize = AtomicUsize::new(0);
static LIVE: Mutex<Option<HashMap<usize, (usize, String)>>> = Mutex::new(None);

fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    INSIDE.with(|i| {
        if i.get() {
            return None;
        }
        i.set(true);
        let r = f();
        i.set(false);
        Some(r)
    })
}

fn record(ptr: *mut u8, size: usize) {
    if size < MIN {
        SMALL.fetch_add(size, Ordering::Relaxed);
        return;
    }
    let _ = guarded(|| {
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        // Only kestrel's own frames (and the first foreign one under them) say where.
        let place: String = bt
            .lines()
            .filter(|l| l.contains("kestrel") || l.contains("iced") || l.contains("cosmic_text"))
            .filter(|l| !l.contains("memtrace") && !l.contains(" at "))
            .take(8)
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" <- ");
        if let Ok(mut m) = LIVE.lock() {
            m.get_or_insert_with(HashMap::new)
                .insert(ptr as usize, (size, place));
        }
    });
}

fn forget(ptr: *mut u8, size: usize) {
    if size < MIN {
        SMALL.fetch_sub(size, Ordering::Relaxed);
        return;
    }
    let _ = guarded(|| {
        if let Ok(mut m) = LIVE.lock()
            && let Some(m) = m.as_mut()
        {
            m.remove(&(ptr as usize));
        }
    });
}

unsafe impl GlobalAlloc for Tracer {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // Safety: forwarded to the system allocator unchanged.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            record(p, l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        forget(p, l.size());
        // Safety: as above.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        forget(p, l.size());
        // Safety: as above.
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            record(q, new);
        }
        q
    }
}

/// Writes the report; returns its path.
pub fn dump() -> String {
    let path =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()) + "/kestrel-memdump.txt";
    let text = guarded(|| {
        let mut by: HashMap<String, (usize, usize)> = HashMap::new();
        let mut total = 0;
        if let Ok(m) = LIVE.lock()
            && let Some(m) = m.as_ref()
        {
            for (size, place) in m.values() {
                let e = by.entry(place.clone()).or_default();
                e.0 += size;
                e.1 += 1;
                total += size;
            }
        }
        let mut v: Vec<_> = by.into_iter().collect();
        v.sort_by(|a, b| b.1.0.cmp(&a.1.0));
        let mut out = format!(
            "traced (>= 16 KiB): {} KiB; small allocations: {} KiB\n\n",
            total / 1024,
            SMALL.load(Ordering::Relaxed) / 1024
        );
        for (place, (size, n)) in v.into_iter().take(40) {
            out += &format!("{:>8} KiB  {n:>4}×  {place}\n", size / 1024);
        }
        out
    })
    .unwrap_or_default();
    let _ = std::fs::write(&path, text);
    path
}
