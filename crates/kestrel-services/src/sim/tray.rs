//! The tray tracker against a simulated session bus, as the watcher (kestrel's own) and as a
//! host of another watcher: apps registering items under a well-known name or a bare path,
//! registering twice, changing their icon or title (signals dropped or repeated), quitting, and
//! restarting under the same well-known name (a sandboxed app sees itself as the same PID every
//! time, so it asks for the same `org.kde.StatusNotifierItem-2-1`), with the world moving on
//! while a read is in flight and the watcher's registrations racing the bus's signals.
//!
//! Invariants: after every step nothing panics and no item is shown twice; whenever the world
//! goes quiet, the items shown are exactly the registered items of the apps still running, as
//! they are now, each with its current owner (a stale owner would ignore its signals for
//! good), and as the watcher, the list other hosts read holds exactly those keys.

use super::{Digest, Rng};
use crate::tray::{Event, Item, Known, News, State, Todo, Tracker, TrayIcon, register};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

const TITLES: [&str; 3] = ["", "Chat", "Sync"];
const ICONS: [&str; 3] = ["chat", "chat-unread", "sync"];

#[derive(Clone, Debug)]
struct ItemTruth {
    path: String,
    key: String,
    id: String,
    title: &'static str,
    icon: &'static str,
    passive: bool,
    registered: bool,
}

struct App {
    /// Its well-known name, when it registered under one.
    name: Option<String>,
    item: ItemTruth,
    /// Its item changed without a signal (the signal was dropped).
    unsignalled: bool,
}

struct World {
    rng: Rng,
    next_id: u32,
    /// kestrel is the watcher; else another watcher is, and kestrel follows its signals.
    watcher: bool,
    /// Well-known name to its owner's unique name.
    names: BTreeMap<String, String>,
    /// Unique name to the app behind it.
    apps: BTreeMap<String, App>,
    /// The list kestrel's watcher keeps (shared with the tracker, as in the service).
    list: Arc<Mutex<Vec<String>>>,
    /// The other watcher's list (host mode).
    other: Vec<String>,
    /// The list as another host knows it from kestrel's watcher's signals.
    heard: BTreeSet<String>,
    /// Registrations with kestrel's watcher, and the bus's signals: two streams, each in
    /// order, the driver's `select` may take from either first.
    local_q: VecDeque<Event>,
    bus_q: VecDeque<Event>,
}

impl World {
    fn new(seed: u64) -> World {
        let mut rng = Rng::new(seed);
        let watcher = rng.chance(75);
        World {
            rng,
            next_id: 1,
            watcher,
            names: BTreeMap::new(),
            apps: BTreeMap::new(),
            list: Arc::new(Mutex::new(Vec::new())),
            other: Vec::new(),
            heard: BTreeSet::new(),
            local_q: VecDeque::new(),
            bus_q: VecDeque::new(),
        }
    }

    /// An app starts. `name`: the well-known name it asks for (a restart reuses its old one).
    fn spawn(&mut self, name: Option<String>) {
        let owner = format!(":1.{}", self.next_id);
        self.next_id += 1;
        let name = match name {
            Some(n) => Some(n),
            None if self.rng.chance(60) => {
                // Sandboxed apps all see PID 2 or 3; others get their own.
                let pid = match self.rng.below(3) {
                    0 => 2,
                    1 => 3,
                    _ => 1000 + self.next_id,
                };
                Some(format!("org.kde.StatusNotifierItem-{pid}-1"))
            }
            None => None,
        };
        // A name someone else holds is not granted: the app falls back to a bare path.
        let name = name.filter(|n| !self.names.contains_key(n));
        let (path, key) = match &name {
            Some(n) => {
                self.names.insert(n.clone(), owner.clone());
                (
                    "/StatusNotifierItem".to_string(),
                    format!("{n}/StatusNotifierItem"),
                )
            }
            None => {
                let path = format!("/org/ayatana/NotificationItem/app{}", self.next_id);
                (path.clone(), format!("{owner}{path}"))
            }
        };
        let item = ItemTruth {
            path,
            key,
            id: format!("app{}", self.rng.below(3)),
            title: self.rng.pick(&TITLES),
            icon: self.rng.pick(&ICONS),
            passive: self.rng.chance(20),
            registered: false,
        };
        self.apps.insert(
            owner.clone(),
            App {
                name,
                item,
                unsignalled: false,
            },
        );
        if self.rng.chance(70) {
            self.register(&owner);
        }
    }

    /// The app registers its item (again, perhaps) with the watcher.
    fn register(&mut self, owner: &str) {
        let Some(a) = self.apps.get_mut(owner) else {
            return;
        };
        a.item.registered = true;
        let key = a.item.key.clone();
        if self.watcher {
            // What kestrel's watcher does with the call.
            if register(&self.list, &key) {
                self.heard.insert(key.clone());
            }
            self.local_q.push_back(Event::Registered {
                key,
                sender: owner.to_string(),
            });
        } else if !self.other.contains(&key) {
            self.other.push(key.clone());
            self.bus_q.push_back(Event::Watcher {
                registered: true,
                key,
            });
        }
    }

    fn some_app(&mut self) -> Option<String> {
        let owners: Vec<String> = self.apps.keys().cloned().collect();
        (!owners.is_empty()).then(|| self.rng.pick(&owners).clone())
    }

    /// The app quits: its names leave the bus (the well-known one first, as the bus daemon
    /// sends them), and another watcher drops its item.
    fn quit(&mut self, owner: &str) -> Option<App> {
        let a = self.apps.remove(owner)?;
        if let Some(n) = &a.name {
            self.names.remove(n);
            self.bus_q.push_back(Event::OwnerLeft {
                name: n.clone(),
                old: owner.to_string(),
            });
        }
        self.bus_q.push_back(Event::OwnerLeft {
            name: owner.to_string(),
            old: owner.to_string(),
        });
        if !self.watcher && self.other.contains(&a.item.key) {
            self.other.retain(|k| *k != a.item.key);
            self.bus_q.push_back(Event::Watcher {
                registered: false,
                key: a.item.key.clone(),
            });
        }
        Some(a)
    }

    /// One random thing happens on the bus.
    fn step(&mut self) {
        let what = self.rng.below(10);
        super::trace(|| format!("world {what} (apps {:?})", self.apps.keys()));
        match what {
            0 | 1 if self.apps.len() < 6 => self.spawn(None),
            2..=4 => {
                let Some(o) = self.some_app() else { return };
                let (title, icon, passive) = (
                    *self.rng.pick(&TITLES),
                    *self.rng.pick(&ICONS),
                    self.rng.chance(20),
                );
                let dropped = self.rng.chance(10);
                let Some(a) = self.apps.get_mut(&o) else {
                    return;
                };
                match self.rng.below(3) {
                    0 => a.item.title = title,
                    1 => a.item.icon = icon,
                    _ => a.item.passive = passive,
                }
                if dropped {
                    a.unsignalled = true;
                } else {
                    let path = a.item.path.clone();
                    self.bus_q.push_back(Event::ItemSignal { sender: o, path });
                }
            }
            5 => {
                // Registers (or registers again).
                let Some(o) = self.some_app() else { return };
                self.register(&o);
            }
            6 => {
                let Some(o) = self.some_app() else { return };
                self.quit(&o);
            }
            7 => {
                // Quits and starts again at once, asking for the same name.
                let Some(o) = self.some_app() else { return };
                if let Some(a) = self.quit(&o) {
                    self.spawn(a.name);
                }
            }
            8 => {
                // A signal repeated with nothing changed.
                let Some(o) = self.some_app() else { return };
                let path = self.apps[&o].item.path.clone();
                self.bus_q.push_back(Event::ItemSignal { sender: o, path });
            }
            _ => {
                // A signal from an app already gone.
                self.bus_q.push_back(Event::ItemSignal {
                    sender: ":1.9999".into(),
                    path: "/StatusNotifierItem".into(),
                });
            }
        }
    }

    fn item(key: &str, t: &ItemTruth) -> Item {
        let (bus, path) = key.split_at(key.find('/').unwrap_or(key.len()));
        Item {
            key: key.to_string(),
            bus: bus.to_string(),
            path: path.to_string(),
            id: t.id.clone(),
            title: t.title.to_string(),
            icon: Some(TrayIcon::Name(t.icon.to_string())),
            menu: None,
            item_is_menu: false,
            passive: t.passive,
        }
    }

    /// Who answers for a bus name now.
    fn owner_of(&self, bus: &str) -> Option<String> {
        if bus.starts_with(':') {
            self.apps.contains_key(bus).then(|| bus.to_string())
        } else {
            self.names.get(bus).cloned()
        }
    }

    /// What the service's `learn` would get for `key` now: the item's properties, then (for a
    /// well-known name) who owns it. `busy` lets the world move between the two.
    fn learn(&mut self, key: &str, busy: bool) -> Option<Known> {
        let (bus, path) = key.split_at(key.find('/')?);
        let app = self.apps.get(&self.owner_of(bus)?)?;
        if app.item.path != path {
            return None;
        }
        let item = World::item(key, &app.item);
        if busy && self.rng.chance(30) {
            self.step();
        }
        let owner = self.owner_of(bus)?;
        Some(Known { item, owner })
    }

    /// What the driver does with the tracker's answer; as the watcher, its announcements reach
    /// the other hosts.
    fn todo(&mut self, t: &mut Tracker, todo: Todo, busy: bool) {
        match todo {
            Todo::Nothing => {}
            Todo::Gone(keys) => self.announce(&[], &keys),
            Todo::Read(asks) => {
                for ask in asks {
                    let k = self.learn(&ask.key, busy);
                    super::trace(|| format!("  read {ask:?}: {:?}", k.as_ref().map(|k| &k.owner)));
                    match t.learned(ask, k) {
                        News::Nothing => {}
                        News::Added(key) => self.announce(&[key], &[]),
                        News::Gone(keys) => self.announce(&[], &keys),
                    }
                }
            }
        }
    }

    fn announce(&mut self, added: &[String], gone: &[String]) {
        if self.watcher {
            self.heard.extend(added.iter().cloned());
            for k in gone {
                self.heard.remove(k);
            }
        }
    }

    /// The driver handling the next event (from either stream).
    fn deliver(&mut self, t: &mut Tracker, busy: bool) -> bool {
        let local = match (self.local_q.is_empty(), self.bus_q.is_empty()) {
            (true, true) => return false,
            (false, true) => true,
            (true, false) => false,
            (false, false) => self.rng.chance(50),
        };
        let e = if local {
            self.local_q.pop_front()
        } else {
            self.bus_q.pop_front()
        };
        if let Some(e) = e {
            super::trace(|| format!("handle {e:?}"));
            let todo = t.event(e);
            self.todo(t, todo, busy);
        }
        true
    }

    /// The world goes quiet: lost signals are sent again (the next change of that item), and
    /// every event is handled.
    fn quiet(&mut self, t: &mut Tracker) {
        let lost: Vec<String> = self
            .apps
            .iter()
            .filter(|(_, a)| a.unsignalled)
            .map(|(o, _)| o.clone())
            .collect();
        for o in lost {
            if let Some(a) = self.apps.get_mut(&o) {
                a.unsignalled = false;
                let path = a.item.path.clone();
                self.bus_q.push_back(Event::ItemSignal { sender: o, path });
            }
        }
        while self.deliver(t, false) {}
    }
}

/// Checked after every step.
fn always(s: &State) -> Result<(), String> {
    for (i, a) in s.items.iter().enumerate() {
        if s.items[i + 1..].iter().any(|b| b.key == a.key) {
            return Err(format!("item shown twice: {}", a.key));
        }
    }
    Ok(())
}

/// Checked when the world is quiet.
fn settled(w: &World, t: &Tracker) -> Result<(), String> {
    let want: BTreeMap<String, (Item, String)> = w
        .apps
        .iter()
        .filter(|(_, a)| a.item.registered)
        .map(|(o, a)| {
            (
                a.item.key.clone(),
                (World::item(&a.item.key, &a.item), o.clone()),
            )
        })
        .collect();
    let have: BTreeMap<String, Item> = t
        .state()
        .items
        .into_iter()
        .map(|i| (i.key.clone(), i))
        .collect();
    let want_items: BTreeMap<String, Item> = want
        .iter()
        .map(|(k, (i, _))| (k.clone(), i.clone()))
        .collect();
    if have != want_items {
        return Err(format!(
            "after the quiet period the tray shows\n  {:?}\nbut the running apps registered\n  {:?}",
            have.keys().collect::<Vec<_>>(),
            want_items.keys().collect::<Vec<_>>()
        ));
    }
    let owners: Vec<(String, String)> = want
        .iter()
        .map(|(k, (_, o))| (k.clone(), o.clone()))
        .collect();
    if t.snapshot() != owners {
        return Err(format!(
            "after the quiet period the tray knows the owners\n  {:?}\nbut they are\n  {owners:?}",
            t.snapshot()
        ));
    }
    if w.watcher {
        let mut list = w
            .list
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let n = list.len();
        list.sort();
        list.dedup();
        let keys: Vec<String> = want.keys().cloned().collect();
        if list != keys || n != keys.len() {
            return Err(format!(
                "the watcher's list (what other hosts read) is\n  {list:?}\nbut should be\n  {keys:?}"
            ));
        }
        let heard: Vec<String> = w.heard.iter().cloned().collect();
        if heard != keys {
            return Err(format!(
                "other hosts, from the watcher's signals, think the list is\n  {heard:?}\nbut it \
                 is\n  {keys:?}"
            ));
        }
    }
    Ok(())
}

const STEPS: usize = 300;

/// One seeded run; the digest is every quiet state seen.
fn run(seed: u64) -> Result<u64, String> {
    let mut w = World::new(seed);
    let mut t = Tracker::new(w.list.clone(), w.watcher);
    let mut digest = Digest::new();
    for step in 0..STEPS {
        match w.rng.below(20) {
            0..=8 => w.step(),
            _ => {
                w.deliver(&mut t, true);
            }
        }
        always(&t.state()).map_err(|e| format!("step {step}: {e}"))?;
        if step % 50 == 49 || step == STEPS - 1 {
            w.quiet(&mut t);
            settled(&w, &t).map_err(|e| format!("step {step}: {e}"))?;
            digest.add(&format!("{:?}", t.state()));
        }
    }
    Ok(digest.value())
}

#[test]
fn seeds() {
    super::seeds("tray", 300, run);
}
