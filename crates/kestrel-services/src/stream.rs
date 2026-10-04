//! Building streams from an async producer.

use futures::channel::mpsc;
use futures::{Stream, StreamExt};
use std::future::Future;

/// A stream fed by `f` through the sender it is given. `f` runs as the stream is polled (no
/// extra task), and the stream ends when `f` returns and every value has been read.
pub fn channel<T, F, Fut>(size: usize, f: F) -> impl Stream<Item = T>
where
    F: FnOnce(mpsc::Sender<T>) -> Fut,
    Fut: Future<Output = ()>,
{
    let (tx, rx) = mpsc::channel(size);
    let producer = futures::stream::once(f(tx)).filter_map(|()| async { None });
    futures::stream::select(rx, producer)
}

/// A stream that never stays dead: when the one `make` returns ends (a connection dropped, a
/// service gone), a new one is made a second later and carries on. Every long-lived service
/// stream runs under this, so an indicator can never freeze on its last answer
/// (docs/ROADMAP-v2.md, rule 1).
pub fn forever<T, S, F>(make: F) -> impl Stream<Item = T>
where
    T: Send + 'static,
    S: Stream<Item = T> + Send + 'static,
    F: Fn() -> S + Send + 'static,
{
    channel(4, async move |mut out| {
        use futures::{SinkExt, StreamExt};
        // Waits 1 s before making it again, doubling to at most 30 s while it keeps ending at
        // once (a service that is not there at all costs nothing), back to 1 s once it works.
        // A panic in the service ends its stream like a dropped connection does (logged), and
        // it is made again: before, the panic unwound through here and killed the
        // subscription's task, freezing that indicator until kestrel restarted, unlogged.
        let mut wait = 1u64;
        loop {
            let worked = std::sync::atomic::AtomicBool::new(false);
            let run = async {
                let s = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&make))?;
                let s = std::panic::AssertUnwindSafe(s).catch_unwind();
                futures::pin_mut!(s);
                while let Some(v) = s.next().await {
                    worked.store(true, std::sync::atomic::Ordering::Relaxed);
                    if out.send(v?).await.is_err() {
                        return Ok(false);
                    }
                }
                Ok(true)
            };
            let r: Result<bool, Box<dyn std::any::Any + Send>> = run.await;
            match r {
                Ok(false) => return,
                Ok(true) => {}
                Err(e) => {
                    let why = e
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_string())
                        .or_else(|| e.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "unknown error".into());
                    log::error!(
                        "{} failed and is started again: {why}",
                        std::any::type_name::<T>()
                    );
                }
            }
            let worked = worked.load(std::sync::atomic::Ordering::Relaxed);
            wait = if worked { 1 } else { (wait * 2).min(30) };
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A service that panics is made again (and its panic does not end the stream).
    #[tokio::test]
    async fn a_panicking_service_is_started_again() {
        static MADE: AtomicU32 = AtomicU32::new(0);
        let s = forever(|| {
            let n = MADE.fetch_add(1, Ordering::SeqCst);
            futures::stream::once(async move {
                assert!(n > 0, "the first run fails");
                n
            })
        });
        futures::pin_mut!(s);
        let first = tokio::time::timeout(std::time::Duration::from_secs(5), s.next()).await;
        assert_eq!(first.ok().flatten(), Some(1));
    }
}
