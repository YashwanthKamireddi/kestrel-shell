//! Wakes exactly when the minute changes; nothing ticks in between.

use chrono::{Local, Timelike};
use futures::{SinkExt, Stream};
use std::time::Duration;

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = ()> {
    crate::stream::channel(1, async |mut out| {
        loop {
            let now = Local::now();
            let ms_left =
                (60 - u64::from(now.second())) * 1000 - u64::from(now.timestamp_subsec_millis());
            tokio::time::sleep(Duration::from_millis(ms_left.max(1) + 5)).await;
            if out.send(()).await.is_err() {
                return;
            }
        }
    })
}
