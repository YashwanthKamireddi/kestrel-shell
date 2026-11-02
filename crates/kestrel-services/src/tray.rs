//! System tray: kestrel is the StatusNotifier watcher and host, and renders the items' menus
//! (com.canonical.dbusmenu) itself.

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
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
    tx: mpsc::UnboundedSender<String>,
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
        {
            let mut items = self
                .items
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if items.contains(&key) {
                return;
            }
            items.push(key.clone());
        }
        let _ = Self::status_notifier_item_registered(&emitter, &key).await;
        let _ = self.tx.send(key);
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
struct Known {
    item: Item,
    owner: String,
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

/// What reached the tray.
enum Ev {
    /// An app registered an item with kestrel's watcher.
    Registered(String),
    Bus(zbus::Message),
}

/// Items kept current from their own signals, as KDE's and GNOME's hosts keep them: an item's
/// New* signal re-reads that item only; an item goes when its owner leaves the bus or the
/// watcher says it went. Before, any item's signal, and any program at all connecting to or
/// leaving the bus (each `notify-send`, each short-lived client), re-read every item and the
/// watcher's list: ~10 calls per item each time.
struct Tray {
    c: Connection,
    /// The watcher's list (its RegisteredStatusNotifierItems, in order).
    items: Arc<Mutex<Vec<String>>>,
    known: HashMap<String, Known>,
    /// kestrel is the watcher (else a host of another watcher, following its signals).
    watcher: bool,
    /// Items that signalled during the current burst, read again once it ends: an app changing
    /// its icon, title and tooltip together was read whole three times, an animated icon on
    /// every frame of the animation.
    stale: std::collections::HashSet<String>,
}

impl Tray {
    fn list(&self) -> Vec<String> {
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn state(&self) -> State {
        State {
            items: self
                .list()
                .iter()
                .filter_map(|k| self.known.get(k).map(|n| n.item.clone()))
                .collect(),
        }
    }

    async fn add(&mut self, key: String) {
        match learn(&self.c, &key).await {
            Some(k) => {
                let mut list = self
                    .items
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if !list.contains(&key) {
                    list.push(key.clone());
                }
                drop(list);
                self.known.insert(key, k);
            }
            // It does not answer (it quit already, or never was an item).
            None => self.remove(&[key]).await,
        }
    }

    /// Items gone: out of the list, and (as the watcher) announced gone.
    async fn remove(&mut self, keys: &[String]) {
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|k| !keys.contains(k));
        for k in keys {
            self.known.remove(k);
            self.stale.remove(k);
        }
        if self.watcher
            && let Ok(w) = self
                .c
                .object_server()
                .interface::<_, Watcher>("/StatusNotifierWatcher")
                .await
        {
            for k in keys {
                let _ = Watcher::status_notifier_item_unregistered(w.signal_emitter(), k).await;
            }
        }
    }

    /// Each item that signalled during the burst, read again (once).
    async fn refresh(&mut self) {
        for key in std::mem::take(&mut self.stale) {
            if self.known.contains_key(&key) {
                self.add(key).await;
            }
        }
    }

    async fn apply(&mut self, ev: Ev) {
        let m = match ev {
            Ev::Registered(key) => return self.add(key).await,
            Ev::Bus(m) => m,
        };
        let h = m.header();
        let iface = h.interface().map(|i| i.to_string()).unwrap_or_default();
        let member = h.member().map(|m| m.to_string()).unwrap_or_default();
        let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
        if iface == ITEM {
            // NewIcon, NewTitle, NewStatus…: that item read again when the burst ends.
            let path = h.path().map(|p| p.to_string()).unwrap_or_default();
            let keys = self
                .known
                .iter()
                .filter(|(_, k)| k.owner == sender && k.item.path == path)
                .map(|(key, _)| key.clone());
            self.stale.extend(keys);
        } else if member == "NameOwnerChanged" {
            let Ok((name, _old, new)) = m.body().deserialize::<(String, String, String)>() else {
                return;
            };
            if !new.is_empty() {
                return;
            }
            let gone: Vec<String> = self
                .list()
                .into_iter()
                .filter(|key| {
                    key.split_once('/').map(|(b, _)| b) == Some(name.as_str())
                        || self.known.get(key).is_some_and(|k| k.owner == name)
                })
                .collect();
            if !gone.is_empty() {
                self.remove(&gone).await;
            }
        } else if iface == WATCHER && !self.watcher {
            // The other watcher's news.
            let Ok(service) = m.body().deserialize::<String>() else {
                return;
            };
            let (bus, path) = split_service(&service, &sender);
            let key = format!("{bus}{path}");
            match member.as_str() {
                "StatusNotifierItemRegistered" => self.add(key).await,
                "StatusNotifierItemUnregistered" => self.remove(&[key]).await,
                _ => {}
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
        let mut tray = Tray {
            c: c.clone(),
            items,
            known: HashMap::new(),
            watcher: own.is_ok(),
            stale: std::collections::HashSet::new(),
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
                        tray.add(key).await;
                    }
                }
            }
        }
        let local = tokio_stream_unbounded(rx).map(Ev::Registered);
        let bus = futures::stream::select(futures::stream::select(a, b), w)
            .filter_map(|m| async { m.ok().map(Ev::Bus) });
        let mut events = Box::pin(futures::stream::select(local, bus));
        // Signals that come together (an icon and its title) are shown together.
        const QUIET: std::time::Duration = std::time::Duration::from_millis(100);
        let mut last = None;
        loop {
            let state = tray.state();
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
