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
    tx: mpsc::UnboundedSender<()>,
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
            let mut items = self.items.lock().unwrap();
            if items.contains(&key) {
                return;
            }
            items.push(key.clone());
        }
        let _ = Self::status_notifier_item_registered(&emitter, &key).await;
        let _ = self.tx.send(());
    }

    fn register_status_notifier_host(&self, _service: String) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
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
        // Closest to 32 px, preferring larger.
        let better = best
            .as_ref()
            .is_none_or(|(bw, _, _)| (w - 32).abs() < (bw - 32).abs() || (w >= 32 && *bw < 32));
        if better && data.len() >= (w * h * 4) as usize {
            best = Some((w, h, data));
        }
    }
    let (w, h, data) = best?;
    // ARGB32 in network byte order → RGBA.
    let mut rgba = Vec::with_capacity(data.len());
    for px in data.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    Some(TrayIcon::Rgba(super::notifications::Pixels::new(
        w as u32, h as u32, rgba,
    )))
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
        let c = Connection::session().await?;
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
    let f = s.fields();
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
    let c = Connection::session().await.ok()?;
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
        let c = Connection::session().await?;
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

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::session().await else {
            return;
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let items = Arc::new(Mutex::new(Vec::<String>::new()));
        let watcher = Watcher {
            items: items.clone(),
            tx: tx.clone(),
        };
        let own = async {
            c.object_server()
                .at("/StatusNotifierWatcher", watcher)
                .await?;
            c.request_name(WATCHER).await
        }
        .await;
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
                    *items.lock().unwrap() = list;
                }
            }
        }
        // Any item signal (new icon, status, title) or a vanished owner means re-read.
        let sig_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(ITEM)
            .map(|b| b.build());
        let owner_rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus")
            .and_then(|b| b.member("NameOwnerChanged"))
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
        let local = futures::stream::poll_fn(move |cx| rx.poll_recv(cx));
        let merged = futures::stream::select(
            futures::stream::select(a.map(|_| ()), b.map(|_| ())),
            futures::stream::select(w.map(|_| ()), local),
        );
        let mut changes = Box::pin(dbus::debounce(
            merged,
            std::time::Duration::from_millis(100),
        ));
        let mut last = None;
        loop {
            // Drop items whose owner has gone.
            let keys = items.lock().unwrap().clone();
            let mut alive = Vec::new();
            let mut state = State::default();
            for k in keys {
                if let Some(it) = read_item(&c, &k).await {
                    alive.push(k);
                    state.items.push(it);
                }
            }
            if let Ok(p) = dbus::proxy(&c, WATCHER, "/StatusNotifierWatcher", WATCHER).await
                && let Ok(list) = p
                    .get_property::<Vec<String>>("RegisteredStatusNotifierItems")
                    .await
                && list != alive
            {
                // Host mode: follow the external watcher's list.
                for k in list {
                    if !alive.contains(&k)
                        && let Some(it) = read_item(&c, &k).await
                    {
                        alive.push(k);
                        state.items.push(it);
                    }
                }
            }
            *items.lock().unwrap() = alive;
            if last.as_ref() != Some(&state) {
                last = Some(state.clone());
                let _ = out.send(state).await;
            }
            if changes.next().await.is_none() {
                return;
            }
        }
    })
}
