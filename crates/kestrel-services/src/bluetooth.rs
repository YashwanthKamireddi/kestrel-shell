//! Bluetooth through BlueZ on the system bus: the adapter, paired devices and the ones nearby
//! while searching; connect, disconnect, pair (trusted and connected after), forget. Pairing
//! codes go through kestrel's own BlueZ agent (`agent`), which asks the shell to show them.
//! Forgetting is always confirmed in the shell first (AirPods share keys with Windows on this
//! machine: forgetting them means pairing again on both systems).

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tokio::sync::mpsc;
use zbus::Connection;
use zbus::interface;
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
    pub paired: bool,
    /// Signal strength while searching (dBm), for ordering nearby devices.
    pub rssi: Option<i16>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub available: bool,
    pub powered: bool,
    pub adapter: String,
    /// Paired devices.
    pub devices: Vec<Device>,
    /// Devices found nearby that are not paired (only those with a name), strongest first.
    pub nearby: Vec<Device>,
    /// The adapter is searching for devices.
    pub discovering: bool,
    /// The name other devices see, and the adapter's address.
    pub name: String,
    pub address: String,
    /// Other devices can find this computer now.
    pub discoverable: bool,
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
    /// Start or stop searching for devices.
    Search(bool),
    /// Pair with a device found nearby, then trust and connect it.
    Pair(String),
    /// Remove a paired device (asked for in the shell first).
    Forget(String),
    /// The name other devices see this computer by.
    Rename(String),
    /// A paired device's name here (its alias).
    RenameDevice(String, String),
    /// Other devices can find this computer (or not).
    Visible(bool),
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
            s.name = prop::<String>(a, "Alias")
                .or_else(|| prop(a, "Name"))
                .unwrap_or_default();
            s.address = prop(a, "Address").unwrap_or_default();
            s.discoverable = prop(a, "Discoverable").unwrap_or(false);
        }
        if let Some(d) = ifaces.get("org.bluez.Device1") {
            let paired = prop::<bool>(d, "Paired").unwrap_or(false);
            // Nearby devices without a name are noise (beacons, phones not advertising one).
            let named = prop::<String>(d, "Name").is_some();
            if !paired && !named {
                continue;
            }
            let dev = Device {
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
                paired,
                rssi: prop(d, "RSSI"),
            };
            if paired {
                s.devices.push(dev);
            } else {
                s.nearby.push(dev);
            }
        }
        if let Some(a) = ifaces.get("org.bluez.Adapter1")
            && path.as_str() == s.adapter
        {
            s.discovering = prop(a, "Discovering").unwrap_or(false);
        }
    }
    s.nearby.sort_by(|a, b| {
        b.rssi
            .unwrap_or(-127)
            .cmp(&a.rssi.unwrap_or(-127))
            .then(a.name.cmp(&b.name))
    });
    s.devices
        .sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
    Ok(s)
}

/// Runs a command; Err with a short reason the shell can show (pairing refused, …).
pub async fn command(cmd: Command) -> Result<(), String> {
    run_command(cmd.clone()).await.map_err(|e| {
        log::warn!("bluetooth {cmd:?}: {e}");
        reason(&e)
    })
}

/// A short, plain reason for a failed command.
fn reason(e: &zbus::Error) -> String {
    let text = e.to_string();
    if text.contains("AuthenticationCanceled") || text.contains("Canceled") {
        "Cancelled".into()
    } else if text.contains("AuthenticationRejected") || text.contains("Rejected") {
        "Refused by the device".into()
    } else if text.contains("AuthenticationFailed") || text.contains("AuthenticationTimeout") {
        "Pairing failed".into()
    } else if text.contains("ConnectionAttemptFailed") || text.contains("page-timeout") {
        "Not in range".into()
    } else if text.contains("AlreadyExists") {
        "Already paired".into()
    } else {
        "Did not work".into()
    }
}

/// One connection for every command, kept: BlueZ ends a search when the connection that
/// started it closes, so a connection per command stopped searching at once.
static CONN: tokio::sync::OnceCell<Connection> = tokio::sync::OnceCell::const_new();

async fn run_command(cmd: Command) -> zbus::Result<()> {
    let c = CONN.get_or_try_init(Connection::system).await?.clone();
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
        Command::Search(on) => {
            let s = read(&c).await?;
            if s.adapter.is_empty() || s.discovering == on {
                return Ok(());
            }
            let adapter = dbus::proxy(&c, BLUEZ, &s.adapter, "org.bluez.Adapter1").await?;
            let r: zbus::Result<()> = adapter
                .call(
                    if on {
                        "StartDiscovery"
                    } else {
                        "StopDiscovery"
                    },
                    &(),
                )
                .await;
            // Another app searching too, or it already stopped: not a failure.
            if let Err(e) = r
                && !e.to_string().contains("InProgress")
                && !e.to_string().contains("NotReady")
                && !e.to_string().contains("Failed")
            {
                return Err(e);
            }
        }
        Command::Pair(p) => {
            let dev = dbus::proxy(&c, BLUEZ, &p, "org.bluez.Device1").await?;
            // Pairing can take as long as the person needs to confirm a code on the device.
            let _: () = dev.call("Pair", &()).await?;
            dbus::set(&c, BLUEZ, &p, "org.bluez.Device1", "Trusted", true).await?;
            let _: () = dev.call("Connect", &()).await?;
        }
        Command::Rename(name) => {
            let s = read(&c).await?;
            if !s.adapter.is_empty() {
                dbus::set(&c, BLUEZ, &s.adapter, "org.bluez.Adapter1", "Alias", name).await?;
            }
        }
        Command::RenameDevice(p, name) => {
            dbus::set(&c, BLUEZ, &p, "org.bluez.Device1", "Alias", name).await?;
        }
        Command::Visible(on) => {
            let s = read(&c).await?;
            if !s.adapter.is_empty() && s.discoverable != on {
                dbus::set(
                    &c,
                    BLUEZ,
                    &s.adapter,
                    "org.bluez.Adapter1",
                    "Discoverable",
                    on,
                )
                .await?;
            }
        }
        Command::Forget(p) => {
            let s = read(&c).await?;
            let path = OwnedObjectPath::try_from(p.as_str())
                .map_err(|e| zbus::Error::Failure(e.to_string()))?;
            let _: () = dbus::proxy(&c, BLUEZ, &s.adapter, "org.bluez.Adapter1")
                .await?
                .call("RemoveDevice", &(path,))
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

/// Searches for devices for as long as the stream is polled (the Bluetooth page is open): the
/// search stops when it is dropped, however the page closed.
pub fn search() -> impl Stream<Item = ()> {
    crate::stream::channel(1, async |_out| {
        struct Stop;
        impl Drop for Stop {
            fn drop(&mut self) {
                if let Ok(h) = tokio::runtime::Handle::try_current() {
                    h.spawn(async {
                        let _ = command(Command::Search(false)).await;
                    });
                }
            }
        }
        let _stop = Stop;
        let _ = command(Command::Search(true)).await;
        futures::future::pending::<()>().await;
    })
}

/// Visible to other devices for as long as the stream is polled (Settings' Bluetooth page is
/// open), then hidden again however the page closed.
pub fn visible() -> impl Stream<Item = ()> {
    crate::stream::channel(1, async |_out| {
        struct Hide;
        impl Drop for Hide {
            fn drop(&mut self) {
                if let Ok(h) = tokio::runtime::Handle::try_current() {
                    h.spawn(async {
                        let _ = command(Command::Visible(false)).await;
                    });
                }
            }
        }
        let _hide = Hide;
        let _ = command(Command::Visible(true)).await;
        futures::future::pending::<()>().await;
    })
}

// --- Pairing agent ---------------------------------------------------------------------------

const AGENT_PATH: &str = "/org/kestrel/BluezAgent";

/// Something a device being paired needs from the person.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Does this code match the one on the device? (Yes / No)
    Confirm {
        id: u64,
        device: String,
        code: Option<String>,
    },
    /// Type the PIN shown on (or chosen for) the device.
    Pin { id: u64, device: String },
    /// Type this code on the device (a keyboard): nothing to answer, only to show.
    Show {
        id: u64,
        device: String,
        code: String,
    },
    /// The request is over (done, cancelled by the device, timed out).
    Done(u64),
}

impl Ask {
    /// The question's id (a finished one's too).
    pub fn id(&self) -> u64 {
        match self {
            Self::Confirm { id, .. } | Self::Pin { id, .. } | Self::Show { id, .. } => *id,
            Self::Done(id) => *id,
        }
    }
}

enum Reply {
    Yes,
    No,
    Pin(String),
}

static WAITING: LazyLock<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Reply>>>> =
    LazyLock::new(Default::default);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The person accepted (or declined) request `id`.
pub fn answer(id: u64, yes: bool) {
    if let Some(tx) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(if yes { Reply::Yes } else { Reply::No });
    }
}

/// The person typed `pin` for request `id`.
pub fn answer_pin(id: u64, pin: String) {
    if let Some(tx) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(Reply::Pin(pin));
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}

struct Agent {
    tx: mpsc::UnboundedSender<Ask>,
    conn: Connection,
}

impl Agent {
    async fn name(&self, device: &OwnedObjectPath) -> String {
        for key in ["Alias", "Name"] {
            if let Ok(n) =
                dbus::get::<String>(&self.conn, BLUEZ, device.as_str(), "org.bluez.Device1", key)
                    .await
            {
                return n;
            }
        }
        "A device".into()
    }

    async fn paired(&self, device: &OwnedObjectPath) -> bool {
        dbus::get::<bool>(
            &self.conn,
            BLUEZ,
            device.as_str(),
            "org.bluez.Device1",
            "Paired",
        )
        .await
        .unwrap_or(false)
    }

    /// Shows `ask` (built with the request's id) and waits for the answer.
    async fn ask(&self, ask: impl FnOnce(u64) -> Ask) -> Option<Reply> {
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Ok(mut w) = WAITING.lock() {
            w.insert(id, tx);
        }
        let _ = self.tx.send(ask(id));
        let r = rx.await.ok();
        let _ = self.tx.send(Ask::Done(id));
        r
    }
}

#[interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {}

    async fn request_pin_code(&self, device: OwnedObjectPath) -> Result<String, AgentError> {
        let name = self.name(&device).await;
        match self.ask(|id| Ask::Pin { id, device: name }).await {
            Some(Reply::Pin(p)) if !p.is_empty() => Ok(p),
            _ => Err(AgentError::Rejected("declined".into())),
        }
    }

    async fn request_passkey(&self, device: OwnedObjectPath) -> Result<u32, AgentError> {
        let name = self.name(&device).await;
        match self.ask(|id| Ask::Pin { id, device: name }).await {
            Some(Reply::Pin(p)) => p
                .trim()
                .parse()
                .map_err(|_| AgentError::Rejected("not a number".into())),
            _ => Err(AgentError::Rejected("declined".into())),
        }
    }

    async fn display_pin_code(&self, device: OwnedObjectPath, pincode: String) {
        let name = self.name(&device).await;
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = self.tx.send(Ask::Show {
            id,
            device: name,
            code: pincode,
        });
    }

    async fn display_passkey(&self, device: OwnedObjectPath, passkey: u32, _entered: u16) {
        let name = self.name(&device).await;
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = self.tx.send(Ask::Show {
            id,
            device: name,
            code: format!("{passkey:06}"),
        });
    }

    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), AgentError> {
        let name = self.name(&device).await;
        match self
            .ask(|id| Ask::Confirm {
                id,
                device: name,
                code: Some(format!("{passkey:06}")),
            })
            .await
        {
            Some(Reply::Yes) => Ok(()),
            _ => Err(AgentError::Rejected("declined".into())),
        }
    }

    async fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
        let name = self.name(&device).await;
        match self
            .ask(|id| Ask::Confirm {
                id,
                device: name,
                code: None,
            })
            .await
        {
            Some(Reply::Yes) => Ok(()),
            _ => Err(AgentError::Rejected("declined".into())),
        }
    }

    /// A paired device using one of its services (audio, input): allowed; anything else asks.
    async fn authorize_service(
        &self,
        device: OwnedObjectPath,
        _uuid: String,
    ) -> Result<(), AgentError> {
        if self.paired(&device).await {
            return Ok(());
        }
        self.request_authorization(device).await
    }

    async fn cancel(&self) {
        // BlueZ gave up on the request in progress: every open question is over.
        let ids: Vec<u64> = WAITING
            .lock()
            .map(|w| w.keys().copied().collect())
            .unwrap_or_default();
        for id in ids {
            if let Some(tx) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
                drop(tx);
            }
            let _ = self.tx.send(Ask::Done(id));
        }
    }
}

/// Registers kestrel as BlueZ's pairing agent (the default one) and streams what it needs to
/// ask. Fails quietly (logged) without BlueZ.
pub fn agent() -> impl Stream<Item = Ask> {
    crate::stream::channel(8, async |mut out| {
        // The test rig's shell shares the system bus: it must not take pairing over from the
        // desktop's.
        if std::env::var_os("KESTREL_NO_BT_AGENT").is_some() {
            return;
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        let conn = async {
            let c = Connection::system().await?;
            c.object_server()
                .at(
                    AGENT_PATH,
                    Agent {
                        tx,
                        conn: c.clone(),
                    },
                )
                .await?;
            let manager = dbus::proxy(&c, BLUEZ, "/org/bluez", "org.bluez.AgentManager1").await?;
            let path = OwnedObjectPath::try_from(AGENT_PATH)
                .map_err(|e| zbus::Error::Failure(e.to_string()))?;
            let _: () = manager
                .call("RegisterAgent", &(&path, "KeyboardDisplay"))
                .await?;
            let _: () = manager.call("RequestDefaultAgent", &(&path,)).await?;
            Ok::<_, zbus::Error>(c)
        }
        .await;
        let _conn = match conn {
            Ok(c) => {
                log::info!("bluetooth: kestrel is the pairing agent");
                c
            }
            Err(e) => {
                log::warn!("bluetooth: no pairing agent ({e})");
                return;
            }
        };
        while let Some(a) = rx.recv().await {
            if out.send(a).await.is_err() {
                return;
            }
        }
    })
}
