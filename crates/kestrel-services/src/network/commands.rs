//! Changing the network: Wi-Fi on or off, joining (saved, new, hidden), leaving, forgetting,
//! auto-join, the hotspot and IP settings, and how a join ended.

use super::*;

pub async fn command(cmd: Command) {
    if let Err(e) = run_command(cmd.clone()).await {
        log::warn!("network {cmd:?}: {e}");
    }
}

/// A command whose failure is shown to the user (NetworkManager's own words).
pub async fn try_command(cmd: Command) -> Result<(), String> {
    run_command(cmd).await.map_err(|e| match e {
        zbus::Error::MethodError(_, Some(m), _) => m,
        zbus::Error::Failure(m) => m,
        e => e.to_string(),
    })
}

pub(super) async fn run_command(cmd: Command) -> zbus::Result<()> {
    let c = Connection::system().await?;
    match cmd {
        Command::SetWifi(on) => dbus::set(&c, NM, NM_PATH, NM, "WirelessEnabled", on).await?,
        Command::Scan => {
            if let Some(dev) = wifi_device(&c).await? {
                let opts: HashMap<&str, Value> = HashMap::new();
                let _: () = dbus::proxy(&c, NM, dev.as_str(), WIFI)
                    .await?
                    .call("RequestScan", &(opts,))
                    .await?;
            }
        }
        Command::Connect(ssid) => {
            let known = saved_all(&c).await?;
            let conn = known.iter().find(|(k, _, _)| *k == ssid).map(|(_, p, _)| p);
            if let (Some(conn), Some(dev)) = (conn, wifi_device(&c).await?) {
                // "/" (zvariant's default path): no specific object.
                let root = OwnedObjectPath::default();
                let ac: OwnedObjectPath = dbus::proxy(&c, NM, NM_PATH, NM)
                    .await?
                    .call("ActivateConnection", &(conn, &dev, &root))
                    .await?;
                outcome(&c, &ac).await?;
            }
        }
        Command::ConnectNew { ssid, ap, password } => {
            let Some(dev) = wifi_device(&c).await? else {
                return Ok(());
            };
            let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();
            let mut wifi = HashMap::new();
            wifi.insert("ssid", Value::from(ssid.as_bytes().to_vec()));
            settings.insert("802-11-wireless", wifi);
            if let Some(psk) = password {
                let mut sec = HashMap::new();
                sec.insert("key-mgmt", Value::from("wpa-psk"));
                sec.insert("psk", Value::from(psk));
                settings.insert("802-11-wireless-security", sec);
            }
            let ap = OwnedObjectPath::try_from(ap).map_err(zbus::Error::from)?;
            let (saved, ac): (OwnedObjectPath, OwnedObjectPath) = dbus::proxy(&c, NM, NM_PATH, NM)
                .await?
                .call("AddAndActivateConnection", &(settings, &dev, &ap))
                .await?;
            // A new network that would not join is not kept (a wrong password typed first
            // must not leave a broken saved network behind).
            if let Err(e) = outcome(&c, &ac).await {
                let _: zbus::Result<()> = async {
                    dbus::proxy(&c, NM, saved.as_str(), CONN)
                        .await?
                        .call("Delete", &())
                        .await
                }
                .await;
                return Err(e);
            }
        }
        Command::Disconnect => {
            if let Some(dev) = wifi_device(&c).await? {
                let _: () = dbus::proxy(&c, NM, dev.as_str(), DEV)
                    .await?
                    .call("Disconnect", &())
                    .await?;
            }
        }
        Command::Forget(ssid) => {
            for (_, path, _) in saved_all(&c)
                .await?
                .into_iter()
                .filter(|(s, _, _)| *s == ssid)
            {
                let _: () = dbus::proxy(
                    &c,
                    NM,
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Settings.Connection",
                )
                .await?
                .call("Delete", &())
                .await?;
            }
        }
        Command::AutoJoin(ssid, on) => {
            const CONN: &str = "org.freedesktop.NetworkManager.Settings.Connection";
            for (_, path, _) in saved_all(&c)
                .await?
                .into_iter()
                .filter(|(s, _, _)| *s == ssid)
            {
                let proxy = dbus::proxy(&c, NM, path.as_str(), CONN).await?;
                let mut settings: HashMap<String, HashMap<String, OwnedValue>> =
                    proxy.call("GetSettings", &()).await?;
                // Update replaces everything: the saved password goes back in with the change,
                // or it would be dropped.
                // GetSettings leaves the saved password out, and Update keeps the stored one
                // when none is sent (checked 2026-11-14 on a throwaway connection: the psk line
                // and its value stayed). So only the switch changes.
                settings
                    .entry("connection".into())
                    .or_default()
                    .insert("autoconnect".into(), OwnedValue::from(on));
                let _: () = proxy.call("Update", &(settings,)).await?;
            }
        }
        Command::ConnectHidden { ssid, password } => {
            let Some(dev) = wifi_device(&c).await? else {
                return Ok(());
            };
            let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();
            let mut wifi = HashMap::new();
            wifi.insert("ssid", Value::from(ssid.as_bytes().to_vec()));
            wifi.insert("hidden", Value::from(true));
            settings.insert("802-11-wireless", wifi);
            if let Some(psk) = password.filter(|p| !p.is_empty()) {
                let mut sec = HashMap::new();
                sec.insert("key-mgmt", Value::from("wpa-psk"));
                sec.insert("psk", Value::from(psk));
                settings.insert("802-11-wireless-security", sec);
            }
            let root = OwnedObjectPath::try_from("/").map_err(zbus::Error::from)?;
            let (saved, ac): (OwnedObjectPath, OwnedObjectPath) = dbus::proxy(&c, NM, NM_PATH, NM)
                .await?
                .call("AddAndActivateConnection", &(settings, &dev, &root))
                .await?;
            if let Err(e) = outcome(&c, &ac).await {
                let _: zbus::Result<()> = async {
                    dbus::proxy(&c, NM, saved.as_str(), CONN)
                        .await?
                        .call("Delete", &())
                        .await
                }
                .await;
                return Err(e);
            }
        }
        Command::Hotspot(None) => {
            // Ending the hotspot's active connection; NetworkManager then joins a saved
            // network by itself, as after any disconnect.
            if let Some(dev) = wifi_device(&c).await? {
                let ac: OwnedObjectPath =
                    dbus::get(&c, NM, dev.as_str(), DEV, "ActiveConnection").await?;
                if ac.as_str() != "/" {
                    let _: () = dbus::proxy(&c, NM, NM_PATH, NM)
                        .await?
                        .call("DeactivateConnection", &(&ac,))
                        .await?;
                }
            }
        }
        Command::Hotspot(Some((name, password))) => {
            let Some(dev) = wifi_device(&c).await? else {
                return Ok(());
            };
            let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();
            let mut conn = HashMap::new();
            conn.insert("id", Value::from(HOTSPOT_ID));
            conn.insert("type", Value::from("802-11-wireless"));
            conn.insert("autoconnect", Value::from(false));
            settings.insert("connection", conn);
            let mut wifi = HashMap::new();
            wifi.insert("ssid", Value::from(name.as_bytes().to_vec()));
            wifi.insert("mode", Value::from("ap"));
            settings.insert("802-11-wireless", wifi);
            let mut sec = HashMap::new();
            sec.insert("key-mgmt", Value::from("wpa-psk"));
            sec.insert("psk", Value::from(password));
            sec.insert("proto", Value::from(vec!["rsn"]));
            sec.insert("pairwise", Value::from(vec!["ccmp"]));
            sec.insert("group", Value::from(vec!["ccmp"]));
            settings.insert("802-11-wireless-security", sec);
            let mut ip4 = HashMap::new();
            // NetworkManager hands out addresses and routes the wired connection (NAT).
            ip4.insert("method", Value::from("shared"));
            settings.insert("ipv4", ip4);
            let mut ip6 = HashMap::new();
            ip6.insert("method", Value::from("ignore"));
            settings.insert("ipv6", ip6);
            // The one kestrel made before is replaced, so there is only ever one.
            for p in connections_with_id(&c, HOTSPOT_ID).await? {
                let _: () = dbus::proxy(&c, NM, p.as_str(), CONN)
                    .await?
                    .call("Delete", &())
                    .await?;
            }
            let root = OwnedObjectPath::try_from("/").map_err(zbus::Error::from)?;
            let _: (OwnedObjectPath, OwnedObjectPath) = dbus::proxy(&c, NM, NM_PATH, NM)
                .await?
                .call("AddAndActivateConnection", &(settings, &dev, &root))
                .await?;
        }
        Command::SetIp(path, ip) => set_ip(&c, &path, &ip).await?,
    }
    Ok(())
}

/// Waits for an activation to end: connected, or why not (in words people use). A password
/// being asked for counts as waiting, so this can take as long as the person types.
pub(super) async fn outcome(c: &Connection, ac: &OwnedObjectPath) -> zbus::Result<()> {
    const ACTIVATED: u32 = 2;
    const DEACTIVATED: u32 = 4;
    let proxy = dbus::proxy(c, NM, ac.as_str(), ACTIVE).await?;
    let mut changes = proxy.receive_signal("StateChanged").await?;
    // Already over before the signal was watched.
    match proxy.get_property::<u32>("State").await {
        Ok(ACTIVATED) => return Ok(()),
        Ok(DEACTIVATED) | Err(_) => return Err(zbus::Error::Failure("Could not connect".into())),
        Ok(_) => {}
    }
    let wait = async {
        while let Some(m) = changes.next().await {
            let Ok((state, reason)) = m.body().deserialize::<(u32, u32)>() else {
                continue;
            };
            match state {
                ACTIVATED => return Ok(()),
                DEACTIVATED => {
                    // NM_ACTIVE_CONNECTION_STATE_REASON_*
                    let why = match reason {
                        9 => "Password needed",
                        10 => "Wrong password",
                        6 => "Took too long to connect",
                        2 => "Cancelled",
                        _ => "Could not connect",
                    };
                    return Err(zbus::Error::Failure(why.into()));
                }
                _ => {}
            }
        }
        Err(zbus::Error::Failure("Could not connect".into()))
    };
    tokio::time::timeout(std::time::Duration::from_secs(180), wait)
        .await
        .unwrap_or_else(|_| Err(zbus::Error::Failure("Took too long to connect".into())))
}

/// Saved connections whose id is `id`.
pub(super) async fn connections_with_id(
    c: &Connection,
    id: &str,
) -> zbus::Result<Vec<OwnedObjectPath>> {
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
        let settings: HashMap<String, HashMap<String, OwnedValue>> =
            dbus::proxy(c, NM, p.as_str(), CONN)
                .await?
                .call("GetSettings", &())
                .await?;
        if settings
            .get("connection")
            .and_then(|s| prop::<String>(s, "id"))
            .is_some_and(|i| i == id)
        {
            out.push(p);
        }
    }
    Ok(out)
}
