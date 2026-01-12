//! Pending package updates: repo packages via checkupdates (a safe temporary database, never
//! the system one) and AUR packages via yay. Checked a minute after start, every two hours,
//! and again right after any package is installed or upgraded (pacman's database changes).

use futures::{SinkExt, Stream, StreamExt};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub repo: usize,
    pub aur: usize,
}

impl State {
    pub fn total(&self) -> usize {
        self.repo + self.aur
    }
}

async fn count(cmd: &str, args: &[&str]) -> usize {
    tokio::process::Command::new(cmd)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

/// Resolves when pacman's local database changes (packages installed, upgraded or removed).
async fn pacman_changed() {
    let Ok(ino) = inotify::Inotify::init() else {
        return std::future::pending().await;
    };
    let mask =
        inotify::WatchMask::CREATE | inotify::WatchMask::DELETE | inotify::WatchMask::MOVED_TO;
    if ino.watches().add("/var/lib/pacman/local", mask).is_err() {
        return std::future::pending().await;
    }
    let Ok(mut events) = ino.into_event_stream([0u8; 1024]) else {
        return std::future::pending().await;
    };
    let _ = events.next().await;
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(1, async |mut out| {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            let s = State {
                repo: count("checkupdates", &[]).await,
                aur: count("yay", &["-Qua"]).await,
            };
            if out.send(s).await.is_err() {
                return;
            }
            // Wait for the two-hour mark or a pacman transaction, whichever comes first.
            let timer = tokio::time::sleep(Duration::from_secs(2 * 3600));
            tokio::pin!(timer);
            tokio::select! {
                () = &mut timer => {}
                () = pacman_changed() => {
                    // Let the transaction finish before counting again.
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    })
}
