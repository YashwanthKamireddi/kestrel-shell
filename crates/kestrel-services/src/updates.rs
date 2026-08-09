//! Pending package updates (crate::pacman: pacman's own library on a temporary sync database,
//! as checkupdates does, and the AUR's web API, as yay -Qua does). Checked a minute after
//! start, every two hours, and again right after any package is installed or upgraded
//! (pacman's database changes).

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

/// One pending update: name, version now, version offered, from the AUR.
#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    pub name: String,
    pub from: String,
    pub to: String,
    pub aur: bool,
}

/// Every pending update, repo and AUR: None if the repos could not be checked (offline), and
/// the AUR part None if the AUR could not be reached.
async fn pending() -> Option<(Vec<Package>, Option<Vec<Package>>)> {
    let crate::pacman::RepoCheck {
        updates: repo,
        foreign,
    } = tokio::task::spawn_blocking(crate::pacman::repo_updates)
        .await
        .ok()??;
    let to = |u: crate::pacman::Update| Package {
        name: u.name,
        from: u.from,
        to: u.to,
        aur: u.aur,
    };
    let aur = crate::pacman::aur_updates(&foreign)
        .await
        .map(|v| v.into_iter().map(to).collect());
    Some((repo.into_iter().map(to).collect(), aur))
}

/// Every pending update, by name (repo and AUR).
pub async fn list() -> Vec<Package> {
    let Some((mut v, aur)) = pending().await else {
        return Vec::new();
    };
    v.extend(aur.unwrap_or_default());
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
            // Offline (or the mirrors unreachable) the count is unknown, not zero: the last one
            // stays and it is tried again in ten minutes.
            let Some((repo, aur)) = pending().await else {
                tokio::time::sleep(Duration::from_secs(600)).await;
                continue;
            };
            let s = State {
                repo: repo.len(),
                aur: aur.map_or_else(|| last.map_or(0, |l| l.aur), |a| a.len()),
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

/// A firmware update fwupd offers (from the LVFS): the device, its version now and the new
/// one, and fwupd's id for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Firmware {
    pub device: String,
    pub from: String,
    pub to: String,
    pub id: String,
    /// Plugged in needed, and a restart after.
    pub needs_ac: bool,
    pub needs_reboot: bool,
}

/// Firmware updates from fwupd's metadata as it has it (no download; fwupd refreshes it on
/// its own timer), asked over its D-Bus API (org.freedesktop.fwupd: GetDevices, then
/// GetUpgrades for each updatable one), as fwupdmgr get-updates asks. Empty without fwupd.
pub async fn firmware() -> Vec<Firmware> {
    use std::collections::HashMap;
    use zbus::zvariant::OwnedValue;
    // fwupd's FwupdDeviceFlags (libfwupd/fwupd-device.rs).
    const UPDATABLE: u64 = 1 << 1;
    const REQUIRE_AC: u64 = 1 << 3;
    const NEEDS_REBOOT: u64 = 1 << 8;
    type Dict = HashMap<String, OwnedValue>;
    let Ok(c) = zbus::Connection::system().await else {
        return Vec::new();
    };
    let Ok(fwupd) =
        crate::dbus::proxy(&c, "org.freedesktop.fwupd", "/", "org.freedesktop.fwupd").await
    else {
        return Vec::new();
    };
    let devices: Vec<Dict> = fwupd.call("GetDevices", &()).await.unwrap_or_default();
    let text = |d: &Dict, k: &str| {
        d.get(k)
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default()
    };
    let mut list = Vec::new();
    for d in &devices {
        let flags = d
            .get("Flags")
            .and_then(|v| u64::try_from(v.clone()).ok())
            .unwrap_or(0);
        if flags & UPDATABLE == 0 {
            continue;
        }
        let id = text(d, "DeviceId");
        // An error ("nothing to do") means no update for it.
        let releases: Vec<Dict> = fwupd
            .call("GetUpgrades", &(id.as_str(),))
            .await
            .unwrap_or_default();
        let Some(r) = releases.first() else {
            continue;
        };
        let name = text(d, "Name");
        list.push(Firmware {
            device: if name.is_empty() {
                "Device".into()
            } else {
                name
            },
            from: text(d, "Version"),
            to: text(r, "Version"),
            id,
            needs_ac: flags & REQUIRE_AC != 0,
            needs_reboot: flags & NEEDS_REBOOT != 0,
        });
    }
    list
}

#[cfg(test)]
mod live_fw {
    /// Firmware updates as fwupd reports them here (by hand: `--ignored`).
    #[tokio::test]
    #[ignore]
    async fn asks_fwupd() {
        println!("{:?}", super::firmware().await);
    }
}
