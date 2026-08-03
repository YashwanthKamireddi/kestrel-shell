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
        let mut wait = 1u64;
        loop {
            let s = make();
            futures::pin_mut!(s);
            let mut worked = false;
            while let Some(v) = s.next().await {
                worked = true;
                if out.send(v).await.is_err() {
                    return;
                }
            }
            wait = if worked { 1 } else { (wait * 2).min(30) };
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        }
    })
}
