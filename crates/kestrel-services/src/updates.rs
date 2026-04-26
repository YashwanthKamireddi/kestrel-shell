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

/// Pending repo updates; None when checkupdates failed (it exits 0 with updates, 2 with none).
async fn repo_count() -> Option<usize> {
    let o = tokio::process::Command::new("checkupdates")
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    match o.status.code() {
        Some(0) => Some(
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count(),
        ),
        Some(2) => Some(0),
        _ => None,
    }
}

/// One pending update: name, version now, version offered, from the AUR.
#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    pub name: String,
    pub from: String,
    pub to: String,
    pub aur: bool,
}

async fn lines(cmd: &str, args: &[&str]) -> Vec<String> {
    tokio::process::Command::new(cmd)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// "name 1.0-1 -> 1.1-1" as a package.
fn parse(line: &str, aur: bool) -> Option<Package> {
    let mut w = line.split_whitespace();
    let name = w.next()?.to_string();
    let from = w.next()?.to_string();
    let _arrow = w.next()?;
    let to = w.next()?.to_string();
    Some(Package {
        name,
        from,
        to,
        aur,
    })
}

/// Every pending update, by name (repo and AUR).
pub async fn list() -> Vec<Package> {
    let (repo, aur) = tokio::join!(lines("checkupdates", &[]), lines("yay", &["-Qua"]));
    let mut v: Vec<Package> = repo
        .iter()
        .filter_map(|l| parse(l, false))
        .chain(aur.iter().filter_map(|l| parse(l, true)))
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
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
        let mut last: Option<State> = None;
        loop {
            // Offline (or the mirror unreachable) checkupdates fails: the count is unknown, not
            // zero, so the last one stays and it is tried again in ten minutes.
            let Some(repo) = repo_count().await else {
                tokio::time::sleep(Duration::from_secs(600)).await;
                continue;
            };
            let s = State {
                repo,
                aur: count("yay", &["-Qua"]).await,
            };
            if last != Some(s) {
                last = Some(s);
                if out.send(s).await.is_err() {
                    return;
                }
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
