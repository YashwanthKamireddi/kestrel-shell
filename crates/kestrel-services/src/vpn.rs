//! Cloudflare WARP and Tailscale, both started only on demand: the toggle starts the system
//! service (a local polkit rule may allow that without a password) and stops it again.

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

/// A device on the tailnet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    pub name: String,
    pub ip: String,
    pub os: String,
    pub online: bool,
    /// It is this device's exit node now.
    pub exit_node: bool,
    /// It offers itself as an exit node.
    pub exit_option: bool,
    /// When it was last seen, "YYYY-MM-DD" or empty while online.
    pub last_seen: String,
}

/// This device on the tailnet, its settings, and the others.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tailnet {
    /// "Running", "NeedsLogin", "Stopped", …
    pub state: String,
    pub name: String,
    pub tailnet: String,
    pub ip: String,
    pub ip6: String,
    pub magic_dns: bool,
    pub ssh: bool,
    pub shields_up: bool,
    pub peers: Vec<Peer>,
}

async fn output(cmd: &str, args: &[&str]) -> Option<std::process::Output> {
    tokio::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()
}

/// `tailscale status` and its settings, read (None while the daemon is not running).
pub async fn tailnet() -> Option<Tailnet> {
    use serde_json::Value;
    let out = output("tailscale", &["status", "--json"]).await?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let bool_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    let ips = |v: &Value| -> Vec<String> {
        v.get("TailscaleIPs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let me = v.get("Self");
    let my_ips = me.map(ips).unwrap_or_default();
    let mut peers: Vec<Peer> = v
        .get("Peer")
        .and_then(Value::as_object)
        .map(|m| {
            m.values()
                .map(|p| {
                    let online = bool_of(p, "Online");
                    let seen = str_of(p, "LastSeen");
                    Peer {
                        name: str_of(p, "HostName"),
                        ip: ips(p).into_iter().next().unwrap_or_default(),
                        os: str_of(p, "OS"),
                        online,
                        exit_node: bool_of(p, "ExitNode"),
                        exit_option: bool_of(p, "ExitNodeOption"),
                        last_seen: if online || seen.starts_with("0001") {
                            String::new()
                        } else {
                            seen.get(..10).unwrap_or("").to_string()
                        },
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    peers.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.cmp(&b.name)));
    // The device's own settings (MagicDNS, SSH, shields).
    let prefs: Value = output("tailscale", &["debug", "prefs"])
        .await
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(Value::Null);
    Some(Tailnet {
        state: str_of(&v, "BackendState"),
        name: me.map(|m| str_of(m, "HostName")).unwrap_or_default(),
        tailnet: v
            .get("CurrentTailnet")
            .map(|t| str_of(t, "Name"))
            .unwrap_or_default(),
        ip: my_ips
            .iter()
            .find(|i| !i.contains(':'))
            .cloned()
            .unwrap_or_default(),
        ip6: my_ips
            .iter()
            .find(|i| i.contains(':'))
            .cloned()
            .unwrap_or_default(),
        magic_dns: bool_of(&prefs, "CorpDNS"),
        ssh: bool_of(&prefs, "RunSSH"),
        shields_up: bool_of(&prefs, "ShieldsUp"),
        peers,
    })
}

/// `tailscale <args>`: Ok, or what it said went wrong (e.g. that it needs an operator).
pub async fn tailscale(args: Vec<String>) -> Result<(), String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = output("tailscale", &a)
        .await
        .ok_or_else(|| "tailscale is not installed".to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(
        if err.contains("Access denied") || err.contains("checkprefs") {
            "Needs permission once: sudo tailscale set --operator=$USER".to_string()
        } else {
            err.lines().next().unwrap_or("Failed").to_string()
        },
    )
}

/// The round trip to a peer, "23 ms", or None.
pub async fn tailscale_ping(ip: String) -> Option<String> {
    let out = output("tailscale", &["ping", "-c", "1", "--timeout", "5s", &ip]).await?;
    let text = String::from_utf8_lossy(&out.stdout);
    let at = text.rfind(" in ")?;
    Some(text[at + 4..].trim().to_string())
}

/// Starts signing in: the login link Tailscale prints, opened in the browser.
pub async fn tailscale_login() -> Option<String> {
    use tokio::io::AsyncBufReadExt;
    let mut child = tokio::process::Command::new("tailscale")
        .arg("up")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let mut lines = tokio::io::BufReader::new(child.stderr.take()?).lines();
    // `tailscale up` keeps waiting for the sign-in; it is left to finish on its own.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    while let Ok(Some(l)) = lines.next_line().await {
        if let Some(u) = l.split_whitespace().find(|w| w.starts_with("https://")) {
            let u = u.to_string();
            crate::compositor::run(crate::compositor::Action::Open(u.clone())).await;
            return Some(u);
        }
    }
    None
}

/// What WARP says about itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Warp {
    /// "Connected", "Disconnected", "Connecting", …
    pub status: String,
    pub reason: String,
    /// The mode as warp-cli names it ("warp+doh", "tunnel_only", …) when known.
    pub mode: String,
    pub account: String,
    /// Tunnel figures (protocol, endpoint, handshake, traffic), as warp-cli words them.
    pub stats: Vec<(String, String)>,
}

fn pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let k = k.trim().trim_start_matches("(user set)").trim();
            let v = v.trim();
            (!k.is_empty() && !v.is_empty()).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

async fn warp_text(args: &[&str]) -> Option<String> {
    let mut a = vec!["--accept-tos"];
    a.extend_from_slice(args);
    let out = output("warp-cli", &a).await?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// WARP's status, mode, account and tunnel figures (None while its service is off).
pub async fn warp() -> Option<Warp> {
    let status = pairs(&warp_text(&["status"]).await?);
    let get = |v: &[(String, String)], k: &str| {
        v.iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(k))
            .map(|(_, b)| b.clone())
            .unwrap_or_default()
    };
    let settings = pairs(&warp_text(&["settings"]).await.unwrap_or_default());
    let mode = match get(&settings, "Mode").as_str() {
        "WarpWithDnsOverHttps" => "warp+doh",
        "WarpWithDnsOverTls" => "warp+dot",
        "Warp" => "warp",
        "DnsOverHttps" => "doh",
        "DnsOverTls" => "dot",
        "WarpProxy" | "Proxy" => "proxy",
        "TunnelOnly" | "WarpTunnelOnly" => "tunnel_only",
        _ => "",
    }
    .to_string();
    let reg = pairs(
        &warp_text(&["registration", "show"])
            .await
            .unwrap_or_default(),
    );
    let stats = pairs(&warp_text(&["stats"]).await.unwrap_or_default())
        .into_iter()
        .take(6)
        .collect();
    Some(Warp {
        status: get(&status, "Status update"),
        reason: get(&status, "Reason"),
        mode,
        account: get(&reg, "Account type"),
        stats,
    })
}

/// Sets WARP's mode (`warp-cli mode <mode>`).
pub async fn warp_mode(mode: String) {
    let _ = warp_text(&["mode", &mode]).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warp_output_becomes_pairs() {
        let text = "Status update: Connected\n\
                    (user set) Mode: WarpWithDnsOverHttps\n\
                    Reason:\n\
                    : orphan value\n\
                    Endpoint: 162.159.193.1:2408\n\
                    not a pair\n";
        assert_eq!(
            pairs(text),
            [
                ("Status update".to_string(), "Connected".to_string()),
                ("Mode".to_string(), "WarpWithDnsOverHttps".to_string()),
                // Only the first colon splits: an address keeps its port.
                ("Endpoint".to_string(), "162.159.193.1:2408".to_string()),
            ]
        );
        assert!(pairs("").is_empty());
    }
}
