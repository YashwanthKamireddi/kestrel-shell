//! Bluetooth through BlueZ on the system bus. kestrel connects and disconnects devices that
//! are already paired; it never pairs or removes them.

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const BLUEZ: &str = "org.bluez";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub path: String,
    pub name: String,
    pub connected: bool,
    /// BlueZ icon name: audio-headset, audio-headphones, input-mouse, phone, …
    pub icon: String,
    pub battery: Option<u8>,
    /// Vendor and product, "bluetooth:v004Cp2019d242F" (0x004C is Apple).
    pub modalias: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub available: bool,
    pub powered: bool,
    pub adapter: String,
    pub devices: Vec<Device>,
}

impl State {
    pub fn connected(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|d| d.connected)
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    SetPowered(bool),
    Connect(String),
    Disconnect(String),
}

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn prop<T: TryFrom<OwnedValue>>(m: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    m.get(k).and_then(|v| T::try_from(v.clone()).ok())
}

async fn read(c: &Connection) -> zbus::Result<State> {
    let objects: Objects = dbus::proxy(c, BLUEZ, "/", "org.freedesktop.DBus.ObjectManager")
        .await?
        .call("GetManagedObjects", &())
        .await?;
    let mut s = State::default();
    for (path, ifaces) in &objects {
        if let Some(a) = ifaces.get("org.bluez.Adapter1")
            && !s.available
        {
            s.available = true;
            s.powered = prop(a, "Powered").unwrap_or(false);
            s.adapter = path.to_string();
        }
        if let Some(d) = ifaces.get("org.bluez.Device1") {
            if !prop::<bool>(d, "Paired").unwrap_or(false) {
                continue;
            }
            s.devices.push(Device {
                path: path.to_string(),
                name: prop::<String>(d, "Alias")
                    .or_else(|| prop(d, "Name"))
                    .unwrap_or_default(),
                connected: prop(d, "Connected").unwrap_or(false),
                modalias: prop(d, "Modalias").unwrap_or_default(),
                icon: prop(d, "Icon").unwrap_or_default(),
                battery: ifaces
                    .get("org.bluez.Battery1")
                    .and_then(|b| prop::<u8>(b, "Percentage")),
            });
        }
    }
    s.devices
        .sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
    Ok(s)
}

pub async fn command(cmd: Command) {
    if let Err(e) = run_command(cmd.clone()).await {
        log::warn!("bluetooth {cmd:?}: {e}");
    }
}

async fn run_command(cmd: Command) -> zbus::Result<()> {
    let c = Connection::system().await?;
    match cmd {
        Command::SetPowered(on) => {
            let s = read(&c).await?;
            if !s.adapter.is_empty() {
                dbus::set(&c, BLUEZ, &s.adapter, "org.bluez.Adapter1", "Powered", on).await?;
            }
        }
        Command::Connect(p) => {
            let _: () = dbus::proxy(&c, BLUEZ, &p, "org.bluez.Device1")
                .await?
                .call("Connect", &())
                .await?;
        }
        Command::Disconnect(p) => {
            let _: () = dbus::proxy(&c, BLUEZ, &p, "org.bluez.Device1")
                .await?
                .call("Disconnect", &())
                .await?;
        }
    }
    Ok(())
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::system().await else {
            return;
        };
        let Ok(mut changes) = dbus::changes(&c, BLUEZ).await else {
            return;
        };
        let mut last = None;
        loop {
            let s = read(&c).await.unwrap_or_default();
            if last.as_ref() != Some(&s) {
                last = Some(s.clone());
                let _ = out.send(s).await;
            }
            if changes.next().await.is_none() {
                return;
            }
        }
    })
}
