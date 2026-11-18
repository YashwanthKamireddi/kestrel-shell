//! The shell's own files are written off the UI thread, by one thread, in the order the
//! changes were made: config.toml, state.toml, the notification history, the game-mode flag,
//! the clipboard history's pins. One writer, not a thread per write: two quick changes to the
//! same file (a slider dragged, a switch flicked twice) must land in order, and two threads
//! writing one file could leave the older change on disk.
//!
//! Reads that must see every earlier write (config.toml read back after a change) go through
//! `run`, which answers once the job has run in that same order.

use std::sync::mpsc::{Sender, channel};
use std::sync::{LazyLock, Mutex};

type Job = Box<dyn FnOnce() + Send>;

static QUEUE: LazyLock<Mutex<Sender<Job>>> = LazyLock::new(|| {
    let (tx, rx) = channel::<Job>();
    let started = std::thread::Builder::new()
        .name("kestrel-disk".into())
        .spawn(move || {
            for job in rx {
                // A failing job is that job's loss only; the writer goes on.
                crate::guard::isolate("Writing a file", job, || ());
            }
        });
    if let Err(e) = started {
        log::error!("disk writer: {e}; files are written in place");
    }
    Mutex::new(tx)
});

/// Does `job` on the writer thread, after everything queued before it.
pub fn write(job: impl FnOnce() + Send + 'static) {
    let job: Job = Box::new(job);
    let refused = match QUEUE.lock() {
        Ok(q) => q.send(job).err().map(|e| e.0),
        Err(_) => Some(job),
    };
    // No writer thread (it could not start): written here rather than lost.
    if let Some(job) = refused {
        job();
    }
}

/// Does `job` on the writer thread, after everything queued before it, and answers with its
/// result (None if it did not run to its end).
pub async fn run<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    write(move || {
        let _ = tx.send(job());
    });
    rx.await.ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_land_in_the_order_they_were_made() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        for i in 0..100 {
            let s = seen.clone();
            super::write(move || s.lock().unwrap().push(i));
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        // `run` answers after everything queued before it.
        let n = rt.block_on(super::run({
            let s = seen.clone();
            move || s.lock().unwrap().len()
        }));
        assert_eq!(n, Some(100));
        assert_eq!(*seen.lock().unwrap(), (0..100).collect::<Vec<_>>());
    }
}
