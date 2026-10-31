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

/// The longest a burst of change signals is gathered before the state is read anyway.
pub const BURST_MOST: std::time::Duration = std::time::Duration::from_millis(500);

/// The next signal of a burst that began at `start`: Some(item) when one comes within `quiet`
/// (Some(None): the stream ended), None when the burst is over, because it went quiet or has
/// lasted `BURST_MOST`. Before the cap, a source that never paused for `quiet` (Bluetooth
/// signal strengths while searching, an animated tray icon) was never read at all: the state
/// stayed on its old answer for as long as the signals kept coming.
pub async fn in_burst<S: Stream + Unpin>(
    s: &mut S,
    start: tokio::time::Instant,
    quiet: std::time::Duration,
) -> Option<Option<S::Item>> {
    let until = (tokio::time::Instant::now() + quiet).min(start + BURST_MOST);
    tokio::time::timeout_at(until, s.next()).await.ok()
}

/// How often a service may fail before it is reported failed (Erlang/OTP's restart intensity):
/// more than `max` failures within `within` and it is held off for `hold` between tries,
/// instead of spinning. A failure is a panic, or an end before its first reading; a stream
/// that ran and then ended (a dropped connection) is a normal reconnect and counts nothing.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub max: usize,
    pub within: std::time::Duration,
    pub hold: std::time::Duration,
}

/// Every service's policy: at most 5 failures in 60 s (docs/ROADMAP-v3.md, Phase 1), then
/// one try every 2 minutes. 2 minutes is longer than the 30 s backoff cap, so a failed
/// service costs less than a restarting one, and short enough that one that comes back
/// (a daemon restarted) is picked up again soon.
pub const POLICY: Policy = Policy {
    max: 5,
    within: std::time::Duration::from_secs(60),
    hold: std::time::Duration::from_secs(120),
};

/// A stream that never stays dead: when the one `make` returns ends (a connection dropped, a
/// service gone), a new one is made a second later and carries on. Every long-lived service
/// stream runs under this, so an indicator can never freeze on its last answer
/// (docs/ROADMAP-v2.md, rule 1). `name` is its row in the health table (`kestrel health`).
pub fn forever<T, S, F>(name: &'static str, make: F) -> impl Stream<Item = T>
where
    T: Send + 'static,
    S: Stream<Item = T> + Send + 'static,
    F: Fn() -> S + Send + 'static,
{
    supervise(name, POLICY, make)
}

/// `forever` under a policy of its own.
pub fn supervise<T, S, F>(name: &'static str, policy: Policy, make: F) -> impl Stream<Item = T>
where
    T: Send + 'static,
    S: Stream<Item = T> + Send + 'static,
    F: Fn() -> S + Send + 'static,
{
    channel(4, async move |mut out| {
        use futures::{SinkExt, StreamExt};
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::time::Instant;
        let health = crate::health::Handle::new(name);
        // Waits 1 s before making it again, doubling to at most 30 s while it keeps ending at
        // once (a service that is not there at all costs nothing), back to 1 s once it works.
        // A panic in the service ends its stream like a dropped connection does (logged), and
        // it is made again: before, the panic unwound through here and killed the
        // subscription's task, freezing that indicator until kestrel restarted, unlogged.
        let mut wait = 1u64;
        // When it failed within the policy's window; cleared once it works again.
        let mut recent = std::collections::VecDeque::<Instant>::new();
        let mut failed = false;
        loop {
            health.update(|e| e.state = crate::health::State::Running);
            let started = Instant::now();
            let worked = AtomicBool::new(false);
            let run = async {
                let s = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&make))?;
                let s = std::panic::AssertUnwindSafe(s).catch_unwind();
                futures::pin_mut!(s);
                // Each item is the reading or the panic that ended it: only a reading counts as
                // working (before, a panic did too, so a panicking service never backed off).
                while let Some(v) = s.next().await {
                    let v = v?;
                    worked.store(true, Ordering::Relaxed);
                    if out.send(v).await.is_err() {
                        return Ok(false);
                    }
                }
                Ok(true)
            };
            let r: Result<bool, Box<dyn std::any::Any + Send>> = run.await;
            let worked = worked.load(Ordering::Relaxed);
            let why = match r {
                Ok(false) => return,
                Ok(true) if worked => None,
                Ok(true) => Some("ended before its first reading".to_string()),
                Err(e) => {
                    let why = e
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_string())
                        .or_else(|| e.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "unknown error".into());
                    log::error!("{name} panicked and is started again: {why}");
                    Some(format!("panicked: {why}"))
                }
            };
            let now = Instant::now();
            // Up for a whole window, or ended after it worked: it recovered, its count starts
            // again.
            if now.duration_since(started) >= policy.within || why.is_none() {
                recent.clear();
                failed = false;
            }
            if why.is_some() {
                recent.push_back(now);
                while recent
                    .front()
                    .is_some_and(|t| now.duration_since(*t) > policy.within)
                {
                    recent.pop_front();
                }
                if !failed && recent.len() > policy.max {
                    failed = true;
                    log::error!(
                        "{name} failed {} times in {} s; tried again every {} s until it works",
                        recent.len(),
                        policy.within.as_secs(),
                        policy.hold.as_secs()
                    );
                }
            }
            health.update(|e| {
                e.restarts += 1;
                e.state = if failed {
                    crate::health::State::Failed
                } else {
                    crate::health::State::Restarting
                };
                if let Some(why) = why {
                    e.error = Some((why, std::time::SystemTime::now()));
                }
            });
            wait = if worked { 1 } else { (wait * 2).min(30) };
            let pause = if failed {
                policy.hold
            } else {
                std::time::Duration::from_secs(wait)
            };
            tokio::time::sleep(pause).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Signals that never pause still let the state be read, at most `BURST_MOST` apart.
    #[tokio::test]
    async fn a_burst_that_never_pauses_still_ends() {
        let every = std::time::Duration::from_millis(10);
        let mut s = Box::pin(futures::stream::unfold((), move |()| async move {
            tokio::time::sleep(every).await;
            Some(((), ()))
        }));
        let start = tokio::time::Instant::now();
        let mut n = 0;
        while let Some(item) = in_burst(&mut s, start, every * 5).await {
            assert!(item.is_some());
            n += 1;
        }
        assert!(n > 1, "the burst was followed");
        let took = start.elapsed();
        assert!(took >= BURST_MOST && took < BURST_MOST * 2, "{took:?}");
    }

    /// A service that panics is made again (and its panic does not end the stream).
    #[tokio::test]
    async fn a_panicking_service_is_started_again() {
        static MADE: AtomicU32 = AtomicU32::new(0);
        let s = forever("test-again", || {
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

    use crate::health::{self, State};
    use std::task::Poll;
    use std::time::Duration;

    const TIGHT: Policy = Policy {
        max: 3,
        within: Duration::from_secs(60),
        hold: Duration::from_secs(120),
    };

    fn row(name: &str) -> Option<health::Entry> {
        health::snapshot().into_iter().find(|e| e.name == name)
    }

    /// Over its limit a service that keeps panicking is failed (with its reason) and held
    /// off, not spun; a healthy service next to it keeps running; both rows go with them.
    #[tokio::test(start_paused = true)]
    async fn repeated_panics_end_in_failed_and_the_rest_keep_running() {
        let mut readings = 0;
        {
            let bad = supervise("test-bad", TIGHT, || {
                futures::stream::poll_fn(|_| -> Poll<Option<u32>> { panic!("boom") })
            });
            let steady = supervise("test-steady", TIGHT, || {
                futures::stream::unfold(0u32, |n| async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Some((n, n + 1))
                })
            });
            futures::pin_mut!(bad, steady);
            let mut run = async |secs| {
                let _ = tokio::time::timeout(Duration::from_secs(secs), async {
                    loop {
                        tokio::select! {
                            _ = bad.next() => {}
                            v = steady.next() => readings += usize::from(v.is_some()),
                        }
                    }
                })
                .await;
            };
            // Fails at 0, 2, 6 and 14 s (waits double): the fourth is over 3 in 60 s.
            run(100).await;
            let b = row("test-bad").expect("bad has a row");
            assert_eq!((b.state, b.restarts), (State::Failed, 4));
            assert!(b.error.is_some_and(|(why, _)| why == "panicked: boom"));
            let s = row("test-steady").expect("steady has a row");
            assert_eq!((s.state, s.restarts), (State::Running, 0));
            // Held off 120 s after the fourth (not spinning), tried once at 134 s, still failed.
            run(40).await;
            let b = row("test-bad").expect("bad has a row");
            assert_eq!((b.state, b.restarts), (State::Failed, 5));
        }
        assert!(readings >= 135, "steady kept reading: {readings}");
        assert!(row("test-bad").is_none() && row("test-steady").is_none());
    }

    /// A service that works again starts its count again: three failures, a reading, then
    /// three more are not over a limit of 3, the fourth after the reading is.
    #[tokio::test(start_paused = true)]
    async fn a_service_that_recovers_starts_its_count_again() {
        static RUNS: AtomicU32 = AtomicU32::new(0);
        let s = supervise("test-recovers", TIGHT, || {
            let n = RUNS.fetch_add(1, Ordering::SeqCst);
            futures::stream::once(async move {
                assert!(n == 3, "run {n} fails");
                n
            })
        });
        futures::pin_mut!(s);
        // Runs at 0, 2, 6 s fail; at 14 s it reads and ends; at 15, 17, 21 s it fails again.
        let first = tokio::time::timeout(Duration::from_secs(21), s.next()).await;
        assert_eq!(first.ok().flatten(), Some(3));
        let _ = tokio::time::timeout_at(
            tokio::time::Instant::now() + Duration::from_secs(14),
            s.next(),
        )
        .await;
        let e = row("test-recovers").expect("it has a row");
        assert_eq!((e.state, e.restarts), (State::Restarting, 7));
        // The fourth failure since it recovered (29 s) is over the limit.
        let _ = tokio::time::timeout(Duration::from_secs(9), s.next()).await;
        let e = row("test-recovers").expect("it has a row");
        assert_eq!((e.state, e.restarts), (State::Failed, 8));
        assert!(
            e.error
                .is_some_and(|(why, _)| why == "panicked: run 7 fails")
        );
    }
}
