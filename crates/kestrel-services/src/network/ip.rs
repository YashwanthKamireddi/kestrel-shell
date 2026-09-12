//! The hotspot and a connection's IP settings: read, checked and written.

use super::*;

/// The running hotspot's name and password (the password from NetworkManager's secrets).
pub async fn hotspot() -> Option<Hotspot> {
    let c = Connection::system().await.ok()?;
    let p = connections_with_id(&c, HOTSPOT_ID)
        .await
        .ok()?
        .into_iter()
        .next()?;
    let proxy = dbus::proxy(&c, NM, p.as_str(), CONN).await.ok()?;
    let settings: HashMap<String, HashMap<String, OwnedValue>> =
        proxy.call("GetSettings", &()).await.ok()?;
    let name = settings
        .get("802-11-wireless")
        .and_then(|w| prop::<Vec<u8>>(w, "ssid"))
        .map(|b| String::from_utf8_lossy(&b).into_owned())?;
    let secrets: HashMap<String, HashMap<String, OwnedValue>> = proxy
        .call("GetSecrets", &("802-11-wireless-security",))
        .await
        .ok()?;
    let password = secrets
        .get("802-11-wireless-security")
        .and_then(|s| prop::<String>(s, "psk"))
        .unwrap_or_default();
    Some(Hotspot { name, password })
}

/// The connection the machine is online through: its settings path, its name, its IPv4
/// settings. None when offline (or only the hotspot runs).
pub async fn primary() -> Option<(String, String, IpSettings)> {
    let c = Connection::system().await.ok()?;
    let ac: OwnedObjectPath = dbus::get(&c, NM, NM_PATH, NM, "PrimaryConnection")
        .await
        .ok()?;
    if ac.as_str() == "/" {
        return None;
    }
    let path: OwnedObjectPath = dbus::get(&c, NM, ac.as_str(), ACTIVE, "Connection")
        .await
        .ok()?;
    let name: String = dbus::get(&c, NM, ac.as_str(), ACTIVE, "Id")
        .await
        .unwrap_or_default();
    let ip = ip_settings_on(&c, path.as_str()).await?;
    Some((path.to_string(), name, ip))
}

/// A saved connection's IPv4 settings.
pub async fn ip_settings(path: String) -> Option<IpSettings> {
    let c = Connection::system().await.ok()?;
    ip_settings_on(&c, &path).await
}

pub(super) async fn ip_settings_on(c: &Connection, path: &str) -> Option<IpSettings> {
    let settings: HashMap<String, HashMap<String, OwnedValue>> = dbus::proxy(c, NM, path, CONN)
        .await
        .ok()?
        .call("GetSettings", &())
        .await
        .ok()?;
    let ip4 = settings.get("ipv4")?;
    let method: String = prop(ip4, "method").unwrap_or_else(|| "auto".into());
    let manual = if method == "manual" {
        let data: Vec<HashMap<String, OwnedValue>> = prop(ip4, "address-data").unwrap_or_default();
        let addr = data.first().and_then(|a| {
            let ip: String = prop(a, "address")?;
            let prefix: u32 = prop(a, "prefix")?;
            Some(format!("{ip}/{prefix}"))
        });
        Some((
            addr.unwrap_or_default(),
            prop::<String>(ip4, "gateway").unwrap_or_default(),
        ))
    } else {
        None
    };
    let ignore_auto: bool = prop(ip4, "ignore-auto-dns").unwrap_or(false);
    let dns = if ignore_auto {
        prop::<Vec<u32>>(ip4, "dns")
            .unwrap_or_default()
            .into_iter()
            .map(|n| std::net::Ipv4Addr::from(n.to_ne_bytes()).to_string())
            .collect()
    } else {
        Vec::new()
    };
    Some(IpSettings { manual, dns })
}

/// Checks typed settings: an address with its prefix, a router, DNS servers (IPv4).
pub fn check_ip(ip: &IpSettings) -> Result<(), String> {
    use std::net::Ipv4Addr;
    if let Some((addr, gw)) = &ip.manual {
        let (a, p) = addr
            .split_once('/')
            .ok_or("The address needs its prefix, like 192.168.1.50/24")?;
        a.parse::<Ipv4Addr>()
            .map_err(|_| format!("{a} is not an IPv4 address"))?;
        match p.parse::<u32>() {
            Ok(1..=32) => {}
            _ => return Err(format!("/{p} is not a prefix (1 to 32)")),
        }
        if !gw.is_empty() {
            gw.parse::<Ipv4Addr>()
                .map_err(|_| format!("{gw} is not a router address"))?;
        }
    }
    for d in &ip.dns {
        d.parse::<Ipv4Addr>()
            .map_err(|_| format!("{d} is not a DNS server address"))?;
    }
    Ok(())
}

/// Saves `ip` into the connection and applies it to its device at once (Reapply: no
/// reconnect; a full reactivation when the device cannot take it live).
pub(super) async fn set_ip(c: &Connection, path: &str, ip: &IpSettings) -> zbus::Result<()> {
    check_ip(ip).map_err(zbus::Error::Failure)?;
    let proxy = dbus::proxy(c, NM, path, CONN).await?;
    let mut settings: HashMap<String, HashMap<String, OwnedValue>> =
        proxy.call("GetSettings", &()).await?;
    let ip4 = settings.entry("ipv4".into()).or_default();
    // The deprecated forms would contradict the new values: only the current keys are sent.
    for k in [
        "addresses",
        "address-data",
        "gateway",
        "dns",
        "ignore-auto-dns",
        "method",
    ] {
        ip4.remove(k);
    }
    let own = |v: Value<'_>| OwnedValue::try_from(v).map_err(zbus::Error::from);
    match &ip.manual {
        Some((addr, gw)) => {
            let (a, p) = addr.split_once('/').unwrap_or((addr, "24"));
            let mut entry: HashMap<String, OwnedValue> = HashMap::new();
            entry.insert("address".into(), own(Value::from(a.to_string()))?);
            entry.insert(
                "prefix".into(),
                own(Value::from(p.parse::<u32>().unwrap_or(24)))?,
            );
            ip4.insert("method".into(), own(Value::from("manual"))?);
            ip4.insert("address-data".into(), own(Value::from(vec![entry]))?);
            if !gw.is_empty() {
                ip4.insert("gateway".into(), own(Value::from(gw.clone()))?);
            }
        }
        None => {
            ip4.insert("method".into(), own(Value::from("auto"))?);
        }
    }
    if !ip.dns.is_empty() {
        let servers: Vec<u32> = ip
            .dns
            .iter()
            .filter_map(|d| d.parse::<std::net::Ipv4Addr>().ok())
            .map(|a| u32::from_ne_bytes(a.octets()))
            .collect();
        ip4.insert("dns".into(), own(Value::from(servers))?);
        ip4.insert("ignore-auto-dns".into(), own(Value::from(true))?);
    }
    let _: () = proxy.call("Update", &(settings,)).await?;
    // Applied to the device now: the device that runs this connection.
    let (wifi, wired) = devices(c).await?;
    for dev in [wifi, wired].into_iter().flatten() {
        let ac: OwnedObjectPath = dbus::get(c, NM, dev.as_str(), DEV, "ActiveConnection").await?;
        if ac.as_str() == "/" {
            continue;
        }
        let runs: OwnedObjectPath = dbus::get(c, NM, ac.as_str(), ACTIVE, "Connection").await?;
        if runs.as_str() != path {
            continue;
        }
        let empty: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        let reapplied: zbus::Result<()> = dbus::proxy(c, NM, dev.as_str(), DEV)
            .await?
            .call("Reapply", &(empty, 0u64, 0u32))
            .await;
        if reapplied.is_err() {
            let conn = OwnedObjectPath::try_from(path).map_err(zbus::Error::from)?;
            let root = OwnedObjectPath::try_from("/").map_err(zbus::Error::from)?;
            let _: OwnedObjectPath = dbus::proxy(c, NM, NM_PATH, NM)
                .await?
                .call("ActivateConnection", &(&conn, &dev, &root))
                .await?;
        }
    }
    Ok(())
}
