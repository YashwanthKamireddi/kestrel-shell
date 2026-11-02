//! Networking through NetworkManager on the system bus: Wi-Fi on/off, networks in range,
//! saved networks (join, forget, auto-join), hidden networks, the active connection's details
//! (addresses, gateway, DNS, band, security, speed), Ethernet, a hotspot, and a connection's
//! IP settings (automatic or manual address, automatic or custom DNS).

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

mod agent;
mod commands;
mod ip;
pub use agent::*;
pub use commands::*;
pub use ip::*;

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DEV: &str = "org.freedesktop.NetworkManager.Device";
const WIRED: &str = "org.freedesktop.NetworkManager.Device.Wired";
const CONN: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
/// The hotspot's saved connection is found by this id.
const HOTSPOT_ID: &str = "Hotspot";
const WIFI: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Link {
    #[default]
    None,
    Wifi,
    Wired,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Network {
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
    /// A saved connection exists for this network.
    pub known: bool,
    pub active: bool,
    pub ap: String,
}

/// A saved Wi-Fi network (in range or not).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub ssid: String,
    /// Joined by itself when in range.
    pub auto: bool,
}

/// The active Wi-Fi connection, in detail.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    /// "192.168.1.23/24".
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    /// MHz.
    pub frequency: u32,
    /// Mbit/s.
    pub speed: u32,
    pub security: String,
    pub mac: String,
    /// The saved connection in use (its settings path), for its IP settings.
    pub connection: String,
}

/// The Ethernet port.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wired {
    /// A cable is in and the other end answers.
    pub plugged: bool,
    pub connected: bool,
    /// Mbit/s, while plugged.
    pub speed: u32,
    pub mac: String,
    /// Addresses and the connection, while connected.
    pub details: Option<Details>,
}

/// The hotspot while it runs: its name and password, as NetworkManager keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hotspot {
    pub name: String,
    pub password: String,
}

/// A connection's IPv4 settings, as saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IpSettings {
    /// None: from the network (DHCP); Some((address/prefix, router)).
    pub manual: Option<(String, String)>,
    /// Empty: the network's DNS; otherwise these servers only.
    pub dns: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub available: bool,
    pub wifi_enabled: bool,
    pub has_wifi: bool,
    pub link: Link,
    pub ssid: Option<String>,
    pub strength: u8,
    /// The connection is being brought up.
    pub connecting: bool,
    pub networks: Vec<Network>,
    /// Every saved Wi-Fi network, by name.
    pub saved: Vec<Saved>,
    /// The active Wi-Fi connection's details.
    pub details: Option<Details>,
    /// The Ethernet port, when the machine has one.
    pub wired: Option<Wired>,
    /// The Wi-Fi is sharing a connection (hotspot mode).
    pub hotspot: bool,
}

#[derive(Clone, Debug)]
pub enum Command {
    SetWifi(bool),
    Scan,
    Connect(String),
    ConnectNew {
        ssid: String,
        ap: String,
        password: Option<String>,
    },
    Disconnect,
    /// Remove every saved connection for this network.
    Forget(String),
    /// Join this saved network by itself when in range (or not).
    AutoJoin(String, bool),
    /// Join a network that does not announce its name.
    ConnectHidden {
        ssid: String,
        password: Option<String>,
    },
    /// Share the wired connection over Wi-Fi with this name and password (WPA2), or stop.
    Hotspot(Option<(String, String)>),
    /// A connection's (settings path) IPv4 settings, then applied to it at once.
    SetIp(String, IpSettings),
}

async fn wifi_device(c: &Connection) -> zbus::Result<Option<OwnedObjectPath>> {
    let devices: Vec<OwnedObjectPath> = dbus::proxy(c, NM, NM_PATH, NM)
        .await?
        .call("GetDevices", &())
        .await?;
    for d in devices {
        let t: u32 = dbus::get(c, NM, d.as_str(), DEV, "DeviceType").await?;
        if t == 2 {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

/// Saved Wi-Fi connections: (ssid, connection path, auto-join).
async fn saved_all(c: &Connection) -> zbus::Result<Vec<(String, OwnedObjectPath, bool)>> {
    let list: Vec<OwnedObjectPath> = dbus::proxy(
        c,
        NM,
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?
    .call("ListConnections", &())
    .await?;
    let mut out = Vec::new();
    for p in list {
        let settings: HashMap<String, HashMap<String, OwnedValue>> = dbus::proxy(
            c,
            NM,
            p.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?
        .call("GetSettings", &())
        .await?;
        if let Some(w) = settings.get("802-11-wireless")
            && let Some(ssid) = w.get("ssid")
            && let Ok(bytes) = Vec::<u8>::try_from(ssid.clone())
            // The hotspot kestrel made is not a network to join.
            && w.get("mode")
                .and_then(|m| String::try_from(m.clone()).ok())
                .is_none_or(|m| m != "ap")
        {
            let auto = settings
                .get("connection")
                .and_then(|c| c.get("autoconnect"))
                .and_then(|v| bool::try_from(v.clone()).ok())
                .unwrap_or(true);
            out.push((String::from_utf8_lossy(&bytes).into_owned(), p, auto));
        }
    }
    Ok(out)
}

/// The Wi-Fi and Ethernet devices (the first of each).
async fn devices(
    c: &Connection,
) -> zbus::Result<(Option<OwnedObjectPath>, Option<OwnedObjectPath>)> {
    let all: Vec<OwnedObjectPath> = dbus::proxy(c, NM, NM_PATH, NM)
        .await?
        .call("GetDevices", &())
        .await?;
    let (mut wifi, mut wired) = (None, None);
    for d in all {
        match dbus::get::<u32>(c, NM, d.as_str(), DEV, "DeviceType").await? {
            2 if wifi.is_none() => wifi = Some(d),
            1 if wired.is_none() => wired = Some(d),
            _ => {}
        }
    }
    Ok((wifi, wired))
}

fn prop<T: TryFrom<OwnedValue>>(m: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    m.get(k).and_then(|v| T::try_from(v.clone()).ok())
}

async fn read(c: &Connection) -> zbus::Result<State> {
    let nm = dbus::get_all(c, NM, NM_PATH, NM).await?;
    let wifi_enabled: bool = prop(&nm, "WirelessEnabled").unwrap_or(false);
    let kind: String = prop(&nm, "PrimaryConnectionType").unwrap_or_default();
    let nm_state: u32 = prop(&nm, "State").unwrap_or(0);
    let mut s = State {
        available: true,
        wifi_enabled,
        link: match kind.as_str() {
            "802-11-wireless" => Link::Wifi,
            "802-3-ethernet" => Link::Wired,
            _ => Link::None,
        },
        // NM_STATE_CONNECTING
        connecting: nm_state == 40,
        ..State::default()
    };
    let (wifi, wired) = devices(c).await?;
    if let Some(dev) = wired {
        s.wired = Some(read_wired(c, &dev).await);
    }
    let Some(dev) = wifi else {
        return Ok(s);
    };
    s.has_wifi = true;
    let w = dbus::get_all(c, NM, dev.as_str(), WIFI).await?;
    // NM_802_11_MODE_AP
    s.hotspot = prop::<u32>(&w, "Mode") == Some(3);
    let active: OwnedObjectPath = prop(&w, "ActiveAccessPoint").unwrap_or_default();
    let aps: Vec<OwnedObjectPath> = prop(&w, "AccessPoints").unwrap_or_default();
    let saved = saved_all(c).await.unwrap_or_default();
    let mut best: HashMap<String, Network> = HashMap::new();
    for ap in aps {
        // One call per access point for everything it says.
        let Ok(a) = dbus::get_all(c, NM, ap.as_str(), AP).await else {
            continue;
        };
        let ssid =
            String::from_utf8_lossy(&prop::<Vec<u8>>(&a, "Ssid").unwrap_or_default()).into_owned();
        if ssid.is_empty() {
            continue;
        }
        let wpa: u32 = prop(&a, "WpaFlags").unwrap_or(0);
        let rsn: u32 = prop(&a, "RsnFlags").unwrap_or(0);
        let n = Network {
            known: saved.iter().any(|(k, _, _)| *k == ssid),
            ssid: ssid.clone(),
            strength: prop(&a, "Strength").unwrap_or(0),
            secure: wpa != 0 || rsn != 0,
            active: ap == active,
            ap: ap.to_string(),
        };
        // One row per SSID: the active access point, otherwise the strongest.
        let replace = best
            .get(&ssid)
            .is_none_or(|b| (n.active && !b.active) || (!b.active && n.strength > b.strength));
        if replace {
            best.insert(ssid, n);
        }
    }
    let mut networks: Vec<Network> = best.into_values().collect();
    // A steady order (rows must not jump under the pointer as signal strength wobbles with
    // every scan): the one in use, then saved ones, each group by signal in the icon's bars
    // (the same steps as `strength_icon`), then by name.
    let bars = |n: &Network| match n.strength {
        0..=15 => 0,
        16..=35 => 1,
        36..=55 => 2,
        56..=75 => 3,
        _ => 4,
    };
    networks.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.known.cmp(&a.known))
            .then(bars(b).cmp(&bars(a)))
            .then(a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
    });
    if let Some(a) = networks.iter().find(|n| n.active) {
        s.ssid = Some(a.ssid.clone());
        s.strength = a.strength;
    }
    // The hotspot's own access point is not a network in range.
    if s.hotspot {
        networks.retain(|n| !n.active);
        s.ssid = None;
    }
    s.networks = networks;
    let mut all: Vec<Saved> = saved
        .into_iter()
        .map(|(ssid, _, auto)| Saved { ssid, auto })
        .collect();
    all.sort_by_key(|a| a.ssid.to_lowercase());
    all.dedup_by(|a, b| a.ssid == b.ssid);
    s.saved = all;
    if active.as_str() != "/" && !s.hotspot {
        s.details = details(c, &dev, &active).await.ok();
    }
    Ok(s)
}

async fn read_wired(c: &Connection, dev: &OwnedObjectPath) -> Wired {
    let w = dbus::get_all(c, NM, dev.as_str(), WIRED)
        .await
        .unwrap_or_default();
    let state: u32 = dbus::get(c, NM, dev.as_str(), DEV, "State")
        .await
        .unwrap_or(0);
    let plugged = prop(&w, "Carrier").unwrap_or(false);
    // NM_DEVICE_STATE_ACTIVATED
    let connected = state == 100;
    Wired {
        plugged,
        connected,
        speed: if plugged {
            prop(&w, "Speed").unwrap_or(0)
        } else {
            0
        },
        mac: prop(&w, "HwAddress")
            .or_else(|| prop(&w, "PermHwAddress"))
            .unwrap_or_default(),
        details: if connected {
            ip_details(c, dev).await.ok()
        } else {
            None
        },
    }
}

/// The active connection's details, from the device, its IP configs and the access point.
async fn details(
    c: &Connection,
    dev: &OwnedObjectPath,
    ap: &OwnedObjectPath,
) -> zbus::Result<Details> {
    let mut d = ip_details(c, dev).await?;
    d.frequency = dbus::get(c, NM, ap.as_str(), AP, "Frequency")
        .await
        .unwrap_or(0);
    // Kbit/s.
    d.speed = dbus::get::<u32>(c, NM, dev.as_str(), WIFI, "Bitrate")
        .await
        .unwrap_or(0)
        / 1000;
    d.mac = dbus::get(c, NM, dev.as_str(), WIFI, "HwAddress")
        .await
        .unwrap_or_default();
    let rsn: u32 = dbus::get(c, NM, ap.as_str(), AP, "RsnFlags")
        .await
        .unwrap_or(0);
    let wpa: u32 = dbus::get(c, NM, ap.as_str(), AP, "WpaFlags")
        .await
        .unwrap_or(0);
    // NM_802_11_AP_SEC_KEY_MGMT_SAE (WPA3 personal) is 0x400, _PSK 0x100, _802_1X 0x200.
    d.security = if rsn & 0x400 != 0 {
        "WPA3".into()
    } else if rsn & 0x200 != 0 || wpa & 0x200 != 0 {
        "WPA2 Enterprise".into()
    } else if rsn != 0 {
        "WPA2".into()
    } else if wpa != 0 {
        "WPA".into()
    } else {
        "None".into()
    };
    Ok(d)
}

/// A device's addresses, router and DNS, and the saved connection it runs.
async fn ip_details(c: &Connection, dev: &OwnedObjectPath) -> zbus::Result<Details> {
    let mut d = Details::default();
    let ac: OwnedObjectPath = dbus::get(c, NM, dev.as_str(), DEV, "ActiveConnection").await?;
    if ac.as_str() != "/" {
        d.connection = dbus::get::<OwnedObjectPath>(c, NM, ac.as_str(), ACTIVE, "Connection")
            .await
            .map(|p| p.to_string())
            .unwrap_or_default();
    }
    type AddressData = Vec<HashMap<String, OwnedValue>>;
    let address = |data: &AddressData| {
        data.first().and_then(|a| {
            let addr = String::try_from(a.get("address")?.clone()).ok()?;
            let prefix = u32::try_from(a.get("prefix")?.clone()).ok()?;
            Some(format!("{addr}/{prefix}"))
        })
    };
    let ip4: OwnedObjectPath = dbus::get(c, NM, dev.as_str(), DEV, "Ip4Config").await?;
    if ip4.as_str() != "/" {
        const IP4: &str = "org.freedesktop.NetworkManager.IP4Config";
        let data: AddressData = dbus::get(c, NM, ip4.as_str(), IP4, "AddressData")
            .await
            .unwrap_or_default();
        d.ipv4 = address(&data);
        d.gateway = dbus::get::<String>(c, NM, ip4.as_str(), IP4, "Gateway")
            .await
            .ok()
            .filter(|g| !g.is_empty());
        let dns: AddressData = dbus::get(c, NM, ip4.as_str(), IP4, "NameserverData")
            .await
            .unwrap_or_default();
        d.dns = dns
            .iter()
            .filter_map(|a| String::try_from(a.get("address")?.clone()).ok())
            .collect();
    }
    let ip6: OwnedObjectPath = dbus::get(c, NM, dev.as_str(), DEV, "Ip6Config").await?;
    if ip6.as_str() != "/" {
        const IP6: &str = "org.freedesktop.NetworkManager.IP6Config";
        let data: AddressData = dbus::get(c, NM, ip6.as_str(), IP6, "AddressData")
            .await
            .unwrap_or_default();
        // The global address, not the link-local one.
        d.ipv6 = data
            .iter()
            .filter_map(|a| {
                let addr = String::try_from(a.get("address")?.clone()).ok()?;
                let prefix = u32::try_from(a.get("prefix")?.clone()).ok()?;
                (!addr.starts_with("fe80")).then(|| format!("{addr}/{prefix}"))
            })
            .next();
    }
    Ok(d)
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::system().await else {
            log::warn!("network: no system bus");
            return;
        };
        let Ok(mut changes) = dbus::changes(&c, NM).await else {
            log::warn!("network: NetworkManager not available");
            return;
        };
        let mut last = None;
        loop {
            match read(&c).await {
                Ok(s) if last.as_ref() != Some(&s) => {
                    last = Some(s.clone());
                    let _ = out.send(s).await;
                }
                Ok(_) => {}
                Err(e) => log::warn!("network: {e}"),
            }
            if changes.next().await.is_none() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_typed_settings() {
        let ok = IpSettings {
            manual: Some(("192.168.1.50/24".into(), "192.168.1.1".into())),
            dns: vec!["1.1.1.1".into()],
        };
        assert!(check_ip(&ok).is_ok());
        let no_prefix = IpSettings {
            manual: Some(("192.168.1.50".into(), String::new())),
            dns: vec![],
        };
        assert!(check_ip(&no_prefix).is_err());
        let bad_dns = IpSettings {
            manual: None,
            dns: vec!["1.1.1".into()],
        };
        assert!(check_ip(&bad_dns).is_err());
    }

    #[test]
    fn each_typed_field_says_what_is_wrong() {
        let manual = |addr: &str, gw: &str| IpSettings {
            manual: Some((addr.into(), gw.into())),
            dns: vec![],
        };
        assert!(
            check_ip(&manual("10.0.0.2/8", "")).is_ok(),
            "no router is fine"
        );
        assert!(check_ip(&manual("10.0.0.2/32", "10.0.0.1")).is_ok());
        for p in ["0", "33", "x", ""] {
            let e = check_ip(&manual(&format!("10.0.0.2/{p}"), "")).unwrap_err();
            assert!(e.contains("is not a prefix"), "/{p}: {e}");
        }
        let e = check_ip(&manual("10.0.0.256/24", "")).unwrap_err();
        assert_eq!(e, "10.0.0.256 is not an IPv4 address");
        let e = check_ip(&manual("10.0.0.2/24", "router")).unwrap_err();
        assert_eq!(e, "router is not a router address");
        let e = check_ip(&IpSettings {
            manual: None,
            dns: vec!["9.9.9.9".into(), "::1".into()],
        })
        .unwrap_err();
        assert_eq!(e, "::1 is not a DNS server address");
        assert!(check_ip(&IpSettings::default()).is_ok(), "automatic");
    }

    /// Writes IP settings into a throwaway saved connection named by `KESTREL_TEST_CONN` (its
    /// settings path) and reads them back. Run by hand: it changes NetworkManager's settings.
    #[test]
    #[ignore]
    fn writes_ip_settings() {
        let Ok(path) = std::env::var("KESTREL_TEST_CONN") else {
            return;
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let c = Connection::system().await.unwrap();
            let want = IpSettings {
                manual: Some(("10.20.30.40/24".into(), "10.20.30.1".into())),
                dns: vec!["1.1.1.1".into(), "9.9.9.9".into()],
            };
            set_ip(&c, &path, &want).await.unwrap();
            assert_eq!(ip_settings_on(&c, &path).await, Some(want));
            let auto = IpSettings::default();
            set_ip(&c, &path, &auto).await.unwrap();
            assert_eq!(ip_settings_on(&c, &path).await, Some(auto));
        });
    }
}
