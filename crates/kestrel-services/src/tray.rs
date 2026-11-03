//! System tray: kestrel is the StatusNotifier watcher and host, and renders the items' menus
//! (com.canonical.dbusmenu) itself.

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Structure, Value};
use zbus::{Connection, MatchRule, MessageStream, interface};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";

#[derive(Clone, Debug, PartialEq)]
pub enum TrayIcon {
    Name(String),
    Rgba(super::notifications::Pixels),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// "bus/path" key.
    pub key: String,
    pub bus: String,
    pub path: String,
    pub id: String,
    pub title: String,
    pub icon: Option<TrayIcon>,
    pub menu: Option<String>,
    pub item_is_menu: bool,
    pub passive: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    /// None = plain, Some(false/true) = check or radio state.
    pub toggle: Option<bool>,
    pub radio: bool,
    pub icon: Option<String>,
    pub children: Vec<MenuItem>,
}

fn split_service(service: &str, sender: &str) -> (String, String) {
    if service.starts_with('/') {
        (sender.to_string(), service.to_string())
    } else if let Some(i) = service.find('/') {
        (service[..i].to_string(), service[i..].to_string())
    } else {
        (service.to_string(), "/StatusNotifierItem".to_string())
    }
}

struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
    tx: mpsc::UnboundedSender<Event>,
}

/// The watcher's side of a registration: the key joins its list unless it is there already.
/// True when it is new (then announced).
pub(crate) fn register(items: &Mutex<Vec<String>>, key: &str) -> bool {
    let mut items = items
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if items.iter().any(|k| k == key) {
        return false;
    }
    items.push(key.to_string());
    true
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let (bus, path) = split_service(&service, &sender);
        let key = format!("{bus}{path}");
        if register(&self.items, &key) {
            let _ = Self::status_notifier_item_registered(&emitter, &key).await;
        }
        // Read even when the key is listed already: an app that restarts under the same
        // well-known name (a sandboxed app sees the same PID every time) registers again
        // before its old process's leaving has been handled. Dropped here, the new item was
        // then removed with the old one.
        let _ = self.tx.send(Event::Registered { key, sender });
    }

    fn register_status_notifier_host(&self, _service: String) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(e: &SignalEmitter<'_>, s: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_item_unregistered(e: &SignalEmitter<'_>, s: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_host_registered(e: &SignalEmitter<'_>) -> zbus::Result<()>;
}

fn best_pixmap(v: OwnedValue) -> Option<TrayIcon> {
    let arr = zbus::zvariant::Array::try_from(v).ok()?;
    let mut best: Option<(i32, i32, Vec<u8>)> = None;
    for el in arr.iter() {
        let Ok(s) = Structure::try_from(el.try_clone().ok()?) else {
            continue;
        };
        let f = s.fields();
        if f.len() != 3 {
            continue;
        }
        let (Ok(w), Ok(h)) = (i32::try_from(&f[0]), i32::try_from(&f[1])) else {
            continue;
        };
        let Ok(data) = Vec::<u8>::try_from(f[2].try_clone().ok()?) else {
            continue;
        };
        // The sizes come from the app: a negative or overflowing one is not a picture (it
        // panicked here, or wrapped to a size the data seemed to cover).
        let Some(len) = pixmap_len(w, h) else {
            continue;
        };
        // Closest to 32 px, preferring larger.
        let better = best
            .as_ref()
            .is_none_or(|(bw, _, _)| (w - 32).abs() < (bw - 32).abs() || (w >= 32 && *bw < 32));
        if better && data.len() >= len {
            best = Some((w, h, data));
        }
    }
    let (w, h, data) = best?;
    Some(TrayIcon::Rgba(argb_to_rgba(w, h, &data)?))
}

/// The bytes a `w` x `h` ARGB32 pixmap takes; None for a size that is not one.
fn pixmap_len(w: i32, h: i32) -> Option<usize> {
    let (w, h) = (usize::try_from(w).ok()?, usize::try_from(h).ok()?);
    (w > 0 && h > 0).then_some(())?;
    w.checked_mul(h)?.checked_mul(4)
}

/// ARGB32 in network byte order → RGBA, exactly `w` x `h` pixels (an app sending more bytes
/// than its size made a picture whose data did not match its size).
fn argb_to_rgba(w: i32, h: i32, data: &[u8]) -> Option<super::notifications::Pixels> {
    let data = data.get(..pixmap_len(w, h)?)?;
    let mut rgba = Vec::with_capacity(data.len());
    for px in data.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    Some(super::notifications::Pixels::new(
        w.unsigned_abs(),
        h.unsigned_abs(),
        rgba,
    ))
}

async fn read_item(c: &Connection, key: &str) -> Option<Item> {
    let i = key.find('/')?;
    let (bus, path) = (&key[..i], &key[i..]);
    let p = dbus::proxy(c, bus, path, ITEM).await.ok()?;
    let get = |n: &'static str| {
        let p = p.clone();
        async move { p.get_property::<OwnedValue>(n).await.ok() }
    };
    let s = |v: Option<OwnedValue>| v.and_then(|v| String::try_from(v).ok()).unwrap_or_default();
    // An app that no longer answers (it restarted under a new bus name, or quit) is gone:
    // reading it "worked" with every field empty and left a blank icon in the bar.
    let id = s(Some(get("Id").await?));
    let status = s(get("Status").await);
    let icon_name = s(get("IconName").await);
    let theme_path = s(get("IconThemePath").await);
    let icon = if !icon_name.is_empty() {
        let from_theme_path = (!theme_path.is_empty())
            .then(|| {
                ["png", "svg"]
                    .iter()
                    .map(|e| std::path::Path::new(&theme_path).join(format!("{icon_name}.{e}")))
                    .find(|p| p.is_file())
            })
            .flatten();
        match from_theme_path {
            Some(p) => Some(TrayIcon::Name(p.to_string_lossy().into_owned())),
            None => Some(TrayIcon::Name(icon_name)),
        }
    } else {
        get("IconPixmap").await.and_then(best_pixmap)
    };
    // Electron leaves Title empty and names the app in the tooltip instead.
    let mut title = s(get("Title").await);
    if title.is_empty() {
        type ToolTip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);
        title = p
            .get_property::<ToolTip>("ToolTip")
            .await
            .map(|t| t.2)
            .unwrap_or_default();
    }
    let menu = get("Menu")
        .await
        .and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v).ok())
        .map(|p| p.to_string())
        .filter(|p| p != "/");
    Some(Item {
        key: key.to_string(),
        bus: bus.to_string(),
        path: path.to_string(),
        title,
        id,
        icon,
        menu,
        item_is_menu: get("ItemIsMenu")
            .await
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false),
        passive: status == "Passive",
    })
}

pub async fn activate(bus: String, path: String, secondary: bool) {
    let r = async {
        let c = dbus::session().await?;
        let p = dbus::proxy(&c, &bus, &path, ITEM).await?;
        p.call::<_, _, ()>(
            if secondary {
                "SecondaryActivate"
            } else {
                "Activate"
            },
            &(0i32, 0i32),
        )
        .await
    }
    .await;
    if let Err(e) = r {
        log::debug!("tray activate: {e}");
    }
}

fn parse_layout(v: &Value) -> Option<MenuItem> {
    let s = Structure::try_from(v.try_clone().ok()?).ok()?;
    // (id, props, children): an app's menu can hold anything, and a shorter structure
    // panicked on the missing fields.
    let f = s.fields();
    if f.len() != 3 {
        return None;
    }
    let id = i32::try_from(&f[0]).ok()?;
    let props: HashMap<String, OwnedValue> =
        HashMap::try_from(f[1].try_clone().ok()?).unwrap_or_default();
    let get_s = |k: &str| props.get(k).and_then(|v| String::try_from(v.clone()).ok());
    let get_b = |k: &str| props.get(k).and_then(|v| bool::try_from(v.clone()).ok());
    let toggle_type = get_s("toggle-type").unwrap_or_default();
    let toggle_state = props
        .get("toggle-state")
        .and_then(|v| i32::try_from(v.clone()).ok())
        .unwrap_or(0);
    if get_b("visible") == Some(false) {
        return None;
    }
    let mut children = Vec::new();
    if let Ok(arr) = zbus::zvariant::Array::try_from(f[2].try_clone().ok()?) {
        for c in arr.iter() {
            let inner = match c {
                Value::Value(b) => (**b).try_clone().ok(),
                other => other.try_clone().ok(),
            };
            if let Some(m) = inner.as_ref().and_then(parse_layout) {
                children.push(m);
            }
        }
    }
    Some(MenuItem {
        id,
        label: get_s("label").unwrap_or_default().replace('_', ""),
        enabled: get_b("enabled").unwrap_or(true),
        separator: get_s("type").as_deref() == Some("separator"),
        toggle: (!toggle_type.is_empty()).then_some(toggle_state == 1),
        radio: toggle_type == "radio",
        icon: get_s("icon-name").filter(|s| !s.is_empty()),
        children,
    })
}

pub async fn menu(bus: String, path: String) -> Option<MenuItem> {
    let c = dbus::session().await.ok()?;
    let p = dbus::proxy(&c, &bus, &path, MENU).await.ok()?;
    let _: Result<bool, _> = p.call("AboutToShow", &(0i32,)).await;
    let props: Vec<&str> = Vec::new();
    // The reply is (u revision, (i id, a{sv} props, av children)); read it typed. Reading the
    // layout as a single variant fails, which left the menu stuck on "Loading…".
    type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);
    let reply: (u32, Layout) = match p.call("GetLayout", &(0i32, -1i32, props)).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("tray menu {bus}{path}: {e}");
            return None;
        }
    };
    let (id, props, children) = reply.1;
    let root = Value::Structure(
        zbus::zvariant::StructureBuilder::new()
            .add_field(id)
            .add_field(props)
            .add_field(children)
            .build()
            .ok()?,
    );
    parse_layout(&root)
}

pub async fn menu_event(bus: String, path: String, id: i32) {
    let r = async {
        let c = dbus::session().await?;
        let p = dbus::proxy(&c, &bus, &path, MENU).await?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0);
        p.call::<_, _, ()>("Event", &(id, "clicked", Value::from(""), ts))
            .await
    }
    .await;
    if let Err(e) = r {
        log::debug!("tray menu event: {e}");
    }
}

/// One tray item as last read, with the unique name its signals come from.
#[derive(Clone, Debug)]
pub(crate) struct Known {
    pub(crate) item: Item,
    pub(crate) owner: String,
}

/// Reads an item and who owns its name (signals carry the unique name, items often register a
/// well-known one: `org.kde.StatusNotifierItem-PID-N`).
async fn learn(c: &Connection, key: &str) -> Option<Known> {
    let item = read_item(c, key).await?;
    let owner = if item.bus.starts_with(':') {
        item.bus.clone()
    } else {
        let name = zbus::names::BusName::try_from(item.bus.clone()).ok()?;
        zbus::fdo::DBusProxy::new(c)
            .await
            .ok()?
            .get_name_owner(name)
            .await
            .ok()?
            .to_string()
    };
    Some(Known { item, owner })
}

/// What reached the tray, as far as its items are concerned.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    /// An app (`sender`, its unique name) registered an item with kestrel's watcher.
    Registered { key: String, sender: String },
    /// An item's own signal (NewIcon, NewTitle, NewStatus…), from its unique name.
    ItemSignal { sender: String, path: String },
    /// A name left the bus (`NameOwnerChanged` with no new owner); `old` owned it.
    OwnerLeft { name: String, old: String },
    /// Another watcher's news (kestrel is then only a host).
    Watcher { registered: bool, key: String },
}

impl Event {
    fn from_message(m: &zbus::Message) -> Option<Event> {
        let h = m.header();
        let iface = h.interface().map(|i| i.to_string()).unwrap_or_default();
        let member = h.member().map(|m| m.to_string()).unwrap_or_default();
        let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
        if iface == ITEM {
            let path = h.path().map(|p| p.to_string()).unwrap_or_default();
            Some(Event::ItemSignal { sender, path })
        } else if member == "NameOwnerChanged" {
            let (name, old, new) = m.body().deserialize::<(String, String, String)>().ok()?;
            new.is_empty().then_some(Event::OwnerLeft { name, old })
        } else if iface == WATCHER {
            let service = m.body().deserialize::<String>().ok()?;
            let (bus, path) = split_service(&service, &sender);
            let registered = match member.as_str() {
                "StatusNotifierItemRegistered" => true,
                "StatusNotifierItemUnregistered" => false,
                _ => return None,
            };
            Some(Event::Watcher {
                registered,
                key: format!("{bus}{path}"),
            })
        } else {
            None
        }
    }
}

/// What the tracker wants done after an event.
#[derive(Debug, PartialEq)]
pub(crate) enum Todo {
    Nothing,
    /// Read these items (each answer goes to `Tracker::learned`).
    Read(Vec<Ask>),
    /// These items left the list (as the watcher, announce it).
    Gone(Vec<String>),
}

/// One item to read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ask {
    pub(crate) key: String,
    /// The program it is read for: the one that registered it, or whose signal asked. A read
    /// goes to whoever owns the name when it arrives: if that is another program (the app
    /// restarted under the same name and has not registered yet), the item asked about is
    /// gone, and the new one comes when it registers. Before, the new program's object was
    /// taken for the old item and kept even if it never registered. None when kestrel only
    /// hosts another watcher's list (its signals do not say who registered).
    pub(crate) owner: Option<String>,
}

impl Ask {
    fn one(key: String, owner: Option<String>) -> Todo {
        Todo::Read(vec![Ask { key, owner }])
    }
}

/// What changed in the list after a read, for other hosts (as the watcher, announce it).
#[derive(Debug, PartialEq)]
pub(crate) enum News {
    Nothing,
    /// It joined the list (it had left it while this read was waiting).
    Added(String),
    /// It left the list (it does not answer: it quit already, or never was an item).
    Gone(Vec<String>),
}

/// Items kept current from their own signals, as KDE's and GNOME's hosts keep them: an item's
/// New* signal re-reads that item only; an item goes when its owner leaves the bus or the
/// watcher says it went. Before, any item's signal, and any program at all connecting to or
/// leaving the bus (each `notify-send`, each short-lived client), re-read every item and the
/// watcher's list: ~10 calls per item each time.
///
/// No I/O: it says which items to read and takes what was read (`learned`). `stream` is its
/// driver on the real bus; the simulation tests (`crate::sim::tray`) drive it against a
/// simulated one.
pub(crate) struct Tracker {
    /// The watcher's list (its RegisteredStatusNotifierItems, in order).
    items: Arc<Mutex<Vec<String>>>,
    /// In key order, so the items one signal asks for are read in the same order every time.
    known: BTreeMap<String, Known>,
    /// kestrel is the watcher (else a host of another watcher, following its signals).
    watcher: bool,
}

impl Tracker {
    pub(crate) fn new(items: Arc<Mutex<Vec<String>>>, watcher: bool) -> Self {
        Tracker {
            items,
            known: BTreeMap::new(),
            watcher,
        }
    }

    fn list(&self) -> Vec<String> {
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn state(&self) -> State {
        State {
            items: self
                .list()
                .iter()
                .filter_map(|k| self.known.get(k).map(|n| n.item.clone()))
                .collect(),
        }
    }

    /// Each item read, with its owner, in key order.
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Vec<(String, String)> {
        self.known
            .iter()
            .map(|(key, k)| (key.clone(), k.owner.clone()))
            .collect()
    }

    pub(crate) fn event(&mut self, e: Event) -> Todo {
        match e {
            Event::Registered { key, sender } => Ask::one(key, Some(sender)),
            // NewIcon, NewTitle, NewStatus…: that item read again.
            Event::ItemSignal { sender, path } => Todo::Read(
                self.known
                    .iter()
                    .filter(|(_, k)| k.owner == sender && k.item.path == path)
                    .map(|(key, _)| Ask {
                        key: key.clone(),
                        owner: Some(sender.clone()),
                    })
                    .collect(),
            ),
            Event::OwnerLeft { name, old } => {
                // What `old` owned goes. A name leaving says who held it: an item read since
                // from a new owner of the same name (an app restarted under it) stays. An item
                // not read yet is matched by its name alone.
                let gone: Vec<String> = self
                    .list()
                    .into_iter()
                    .filter(|key| match self.known.get(key) {
                        Some(k) => k.owner == old,
                        None => key.split_once('/').map(|(b, _)| b) == Some(name.as_str()),
                    })
                    .collect();
                Todo::gone(self.remove(&gone))
            }
            Event::Watcher { .. } if self.watcher => Todo::Nothing,
            Event::Watcher {
                registered: true,
                key,
            } => Ask::one(key, None),
            Event::Watcher {
                registered: false,
                key,
            } => Todo::gone(self.remove(std::slice::from_ref(&key))),
        }
    }

    /// What reading an item gave.
    pub(crate) fn learned(&mut self, ask: Ask, k: Option<Known>) -> News {
        let key = ask.key;
        // Another program answers for it now: the item asked about is gone.
        let k = k.filter(|k| ask.owner.as_ref().is_none_or(|o| *o == k.owner));
        match k {
            Some(k) => {
                let added = register(&self.items, &key);
                self.known.insert(key.clone(), k);
                if added {
                    News::Added(key)
                } else {
                    News::Nothing
                }
            }
            None => match self.remove(std::slice::from_ref(&key)) {
                gone if gone.is_empty() => News::Nothing,
                gone => News::Gone(gone),
            },
        }
    }

    /// Takes `keys` out; returns those that were in the list.
    fn remove(&mut self, keys: &[String]) -> Vec<String> {
        let mut gone = Vec::new();
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|k| {
                let go = keys.contains(k);
                if go {
                    gone.push(k.clone());
                }
                !go
            });
        for k in keys {
            self.known.remove(k);
        }
        gone
    }
}

impl Todo {
    fn gone(keys: Vec<String>) -> Todo {
        if keys.is_empty() {
            Todo::Nothing
        } else {
            Todo::Gone(keys)
        }
    }
}

/// Raw events: a registration with kestrel's watcher, or a bus message.
enum Ev {
    Local(Event),
    Bus(zbus::Message),
}

/// The tracker on the real bus: each event applied, the items it asks for read, and (as the
/// watcher) changes to the list announced.
struct Driver {
    c: Connection,
    t: Tracker,
    /// Items asked for during the current burst, read once when it ends (the latest ask per
    /// item): each icon, title or tooltip signal read the whole item again, ~10 calls, and an
    /// animated icon signals every frame.
    pending: Vec<Ask>,
}

impl Driver {
    async fn apply(&mut self, ev: Ev) {
        let e = match ev {
            Ev::Local(e) => e,
            Ev::Bus(m) => match Event::from_message(&m) {
                Some(e) => e,
                None => return,
            },
        };
        match self.t.event(e) {
            Todo::Nothing => {}
            Todo::Read(asks) => {
                for ask in asks {
                    self.pending.retain(|p| p.key != ask.key);
                    self.pending.push(ask);
                }
            }
            Todo::Gone(keys) => {
                // An item that went needs no read at the end of the burst.
                self.pending.retain(|p| !keys.contains(&p.key));
                self.announce(&[], &keys).await;
            }
        }
    }

    /// The burst ended: each item asked for is read once.
    async fn refresh(&mut self) {
        for ask in std::mem::take(&mut self.pending) {
            self.read(ask).await;
        }
    }

    async fn read(&mut self, ask: Ask) {
        let k = learn(&self.c, &ask.key).await;
        match self.t.learned(ask, k) {
            News::Nothing => {}
            News::Added(key) => self.announce(&[key], &[]).await,
            News::Gone(keys) => self.announce(&[], &keys).await,
        }
    }

    /// As the watcher, tells other hosts what joined and left the list.
    async fn announce(&self, added: &[String], gone: &[String]) {
        if (added.is_empty() && gone.is_empty()) || !self.t.watcher {
            return;
        }
        if let Ok(w) = self
            .c
            .object_server()
            .interface::<_, Watcher>("/StatusNotifierWatcher")
            .await
        {
            for k in added {
                let _ = Watcher::status_notifier_item_registered(w.signal_emitter(), k).await;
            }
            for k in gone {
                let _ = Watcher::status_notifier_item_unregistered(w.signal_emitter(), k).await;
            }
        }
    }
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::session().await else {
            return;
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let items = Arc::new(Mutex::new(Vec::<String>::new()));
        let watcher = Watcher {
            items: items.clone(),
            tx,
        };
        // Listening before anything is read: nothing said in between is missed.
        let sig_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(ITEM)
            .map(|b| b.build());
        // Only names leaving (arg2 empty): an item's owner quitting.
        let owner_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .and_then(|b| b.interface("org.freedesktop.DBus"))
            .and_then(|b| b.member("NameOwnerChanged"))
            .and_then(|b| b.arg(2, ""))
            .map(|b| b.build());
        let watcher_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(WATCHER)
            .map(|b| b.build());
        let streams = match (sig_rule, owner_rule, watcher_rule) {
            (Ok(a), Ok(b), Ok(w)) => (
                MessageStream::for_match_rule(a, &c, Some(32)).await,
                MessageStream::for_match_rule(b, &c, Some(32)).await,
                MessageStream::for_match_rule(w, &c, Some(32)).await,
            ),
            _ => return,
        };
        let (Ok(a), Ok(b), Ok(w)) = streams else {
            return;
        };
        let own = async {
            c.object_server()
                .at("/StatusNotifierWatcher", watcher)
                .await?;
            c.request_name(WATCHER).await
        }
        .await;
        let mut tray = Driver {
            c: c.clone(),
            t: Tracker::new(items, own.is_ok()),
            pending: Vec::new(),
        };
        if let Err(e) = own {
            // Someone else is the watcher: be a host and read its list.
            log::info!("tray: using the existing watcher ({e})");
            if let Ok(p) = dbus::proxy(&c, WATCHER, "/StatusNotifierWatcher", WATCHER).await {
                let _: Result<(), _> = p
                    .call(
                        "RegisterStatusNotifierHost",
                        &(c.unique_name().map(|n| n.to_string()).unwrap_or_default(),),
                    )
                    .await;
                if let Ok(list) = p
                    .get_property::<Vec<String>>("RegisteredStatusNotifierItems")
                    .await
                {
                    for key in list {
                        tray.read(Ask { key, owner: None }).await;
                    }
                }
            }
        }
        let local = tokio_stream_unbounded(rx).map(Ev::Local);
        let bus = futures::stream::select(futures::stream::select(a, b), w)
            .filter_map(|m| async { m.ok().map(Ev::Bus) });
        let mut events = Box::pin(futures::stream::select(local, bus));
        // Signals that come together (an icon and its title) are shown together.
        const QUIET: std::time::Duration = std::time::Duration::from_millis(100);
        let mut last = None;
        loop {
            let state = tray.t.state();
            if last.as_ref() != Some(&state) {
                last = Some(state.clone());
                let _ = out.send(state).await;
            }
            let Some(ev) = events.next().await else {
                return;
            };
            tray.apply(ev).await;
            let start = tokio::time::Instant::now();
            while let Some(ev) = crate::stream::in_burst(&mut events, start, QUIET).await {
                let Some(ev) = ev else { return };
                tray.apply(ev).await;
            }
            tray.refresh().await;
        }
    })
}

fn tokio_stream_unbounded<T>(mut rx: mpsc::UnboundedReceiver<T>) -> impl Stream<Item = T> {
    futures::stream::poll_fn(move |cx| rx.poll_recv(cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixmap_sizes_from_an_app_are_checked() {
        let p = argb_to_rgba(1, 1, &[9, 1, 2, 3, 7, 7, 7, 7]).unwrap();
        assert_eq!((p.width, p.height), (1, 1));
        assert_eq!(*p.data, [1, 2, 3, 9], "only the pixels its size covers");
        // Negative sizes made a 4-byte "picture" of 4 billion pixels; an overflowing one
        // panicked or wrapped to a size the data seemed to cover.
        assert!(argb_to_rgba(-1, -1, &[0; 4]).is_none());
        assert!(argb_to_rgba(65536, 65536, &[0; 4]).is_none());
        assert!(argb_to_rgba(2, 2, &[0; 15]).is_none());
    }

    #[test]
    fn a_malformed_menu_item_is_skipped_not_a_panic() {
        // A menu whose one child is a structure of a single field.
        let child = Value::Value(Box::new(Value::Structure(
            zbus::zvariant::StructureBuilder::new()
                .add_field(5i32)
                .build()
                .unwrap(),
        )));
        let children = zbus::zvariant::Array::from(vec![child]);
        let root = Value::Structure(
            zbus::zvariant::StructureBuilder::new()
                .add_field(0i32)
                .add_field(HashMap::<String, Value>::new())
                .append_field(Value::Array(children))
                .build()
                .unwrap(),
        );
        let menu = parse_layout(&root).unwrap();
        assert!(menu.children.is_empty());
    }

    #[test]
    fn an_items_address_is_read_every_way_apps_register_it() {
        let s = |service: &str| split_service(service, ":1.42");
        // A path only (Electron apps): the sender is the bus name.
        assert_eq!(
            s("/org/ayatana/NotificationItem/x"),
            (
                ":1.42".to_string(),
                "/org/ayatana/NotificationItem/x".to_string()
            )
        );
        // A bus name and a path together.
        assert_eq!(
            s(":1.7/StatusNotifierItem"),
            (":1.7".to_string(), "/StatusNotifierItem".to_string())
        );
        assert_eq!(
            s("org.kde.StatusNotifierItem-12-1/a/b"),
            (
                "org.kde.StatusNotifierItem-12-1".to_string(),
                "/a/b".to_string()
            )
        );
        // A bus name only: the standard path.
        assert_eq!(
            s("org.blueman.Tray"),
            (
                "org.blueman.Tray".to_string(),
                "/StatusNotifierItem".to_string()
            )
        );
    }
}
