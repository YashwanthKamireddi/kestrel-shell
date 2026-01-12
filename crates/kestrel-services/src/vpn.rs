//! Cloudflare WARP and Tailscale, both started only on demand: the toggle starts the system
//! service (allowed without a password by the machine's polkit rule) and stops it again.

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use zbus::Connection;
use zbus::zvariant::OwnedObjectPath;

const SYSTEMD: &str = "org.freedesktop.systemd1";
const MANAGER: &str = "org.freedesktop.systemd1.Manager";
pub const WARP: &str = "warp-svc.service";
pub const TAILSCALE: &str = "tailscaled.service";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub warp: Option<bool>,
    pub tailscale: Option<bool>,
}

async fn unit_path(c: &Connection, unit: &str) -> Option<OwnedObjectPath> {
    dbus::proxy(c, SYSTEMD, "/org/freedesktop/systemd1", MANAGER)
        .await
        .ok()?
        .call("LoadUnit", &(unit,))
        .await
        .ok()
}

/// None when the unit is not installed.
async fn active(c: &Connection, unit: &str) -> Option<bool> {
    let p = unit_path(c, unit).await?;
    let load: String = dbus::get(
        c,
        SYSTEMD,
        p.as_str(),
        "org.freedesktop.systemd1.Unit",
        "LoadState",
    )
    .await
    .ok()?;
    if load != "loaded" {
        return None;
    }
    let st: String = dbus::get(
        c,
        SYSTEMD,
        p.as_str(),
        "org.freedesktop.systemd1.Unit",
        "ActiveState",
    )
    .await
    .ok()?;
    Some(matches!(st.as_str(), "active" | "activating" | "reloading"))
}

async fn read(c: &Connection) -> State {
    State {
        warp: active(c, WARP).await,
        tailscale: active(c, TAILSCALE).await,
    }
}

async fn unit(start: bool, name: &str) -> zbus::Result<()> {
    let c = Connection::system().await?;
    let _: OwnedObjectPath = dbus::proxy(&c, SYSTEMD, "/org/freedesktop/systemd1", MANAGER)
        .await?
        .call(
            if start { "StartUnit" } else { "StopUnit" },
            &(name, "replace"),
        )
        .await?;
    Ok(())
}

async fn warp_cli(arg: &str) -> bool {
    tokio::process::Command::new("warp-cli")
        .arg(arg)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

pub async fn set_warp(on: bool) {
    let r = async {
        if on {
            unit(true, WARP).await?;
            // The daemon takes a moment before it answers.
            for _ in 0..20 {
                if warp_cli("status").await {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            warp_cli("connect").await;
        } else {
            warp_cli("disconnect").await;
            unit(false, WARP).await?;
        }
        zbus::Result::Ok(())
    }
    .await;
    if let Err(e) = r {
        log::warn!("warp: {e}");
    }
}

pub async fn set_tailscale(on: bool) {
    if let Err(e) = unit(on, TAILSCALE).await {
        log::warn!("tailscale: {e}");
    }
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::system().await else {
            return;
        };
        // systemd only signals unit changes to subscribers.
        if let Ok(p) = dbus::proxy(&c, SYSTEMD, "/org/freedesktop/systemd1", MANAGER).await {
            let _: Result<(), _> = p.call("Subscribe", &()).await;
        }
        let mut streams = Vec::new();
        for u in [WARP, TAILSCALE] {
            if let Some(p) = unit_path(&c, u).await
                && let Ok(rule) = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface("org.freedesktop.DBus.Properties")
                    .and_then(|b| b.path(p.to_string()))
                    .map(|b| b.build())
                && let Ok(s) = zbus::MessageStream::for_match_rule(rule, &c, Some(16)).await
            {
                streams.push(s.map(|_| ()));
            }
        }
        let mut changes = Box::pin(dbus::debounce(
            futures::stream::select_all(streams),
            std::time::Duration::from_millis(150),
        ));
        let mut last = None;
        loop {
            let s = read(&c).await;
            if last.as_ref() != Some(&s) {
                last = Some(s.clone());
                let _ = out.send(s).await;
            }
            if changes.next().await.is_none() {
                std::future::pending::<()>().await;
            }
        }
    })
}
