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
