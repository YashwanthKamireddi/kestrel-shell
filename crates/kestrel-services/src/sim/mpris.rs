//! The MPRIS tracker against a simulated session bus: players appearing (alone, with two
//! names, a browser and its integration helper for the same tab), their properties changing,
//! signals dropped or repeated, names taken over, players not ready when they appear, owners
//! leaving, and the world moving on while a read is in flight.
//!
//! Invariants: after every step nothing panics, no two players shown are the same media and
//! none is untitled; whenever the world goes quiet (every player that changed has signalled and
//! every signal has been handled) what the tracker holds equals a full re-read of the
//! simulated bus, which is what a fresh start would see: the same names, the same owners (a
//! stale owner would ignore that player's signals for good) and the same players shown.

use super::{Digest, Rng};
use crate::mpris::{Event, Known, State, Tracker, same_media};
use std::collections::{BTreeMap, HashMap, VecDeque};
use zbus::zvariant::{OwnedValue, Value};

const PIDS: [u32; 3] = [100, 101, 102];
const TITLES: [&str; 9] = [
    "Song A",
    "Song A - YouTube",
    "song a",
    "\u{200e}Song A\u{a0}",
    "Song A, live at the hall",
    "Video B",
    "Apple Music - Web Player",
    "",
    "Monday",
];
const ARTISTS: [&str; 3] = ["", "Someone", "Someone & Other"];

#[derive(Clone, Debug)]
struct Media {
    identity: &'static str,
    title: &'static str,
    artist: &'static str,
    art: Option<u8>,
    playing: bool,
    length: i64,
    /// The browser this helper reports for, in its metadata.
    meta_pid: Option<(&'static str, u32)>,
}

struct App {
    names: Vec<String>,
    media: Media,
    /// False while its object is not exported yet: reading it fails.
    ready: bool,
    /// It changed without a signal (the signal was dropped).
    unsignalled: bool,
}

struct World {
    rng: Rng,
    next_id: u32,
    /// Well-known name to its owner's unique name.
    names: BTreeMap<String, String>,
    /// Unique name to the program behind it.
    apps: BTreeMap<String, App>,
    /// The two match rules' streams: zbus keeps each in order, the driver's `select` may take
    /// from either first.
    props_q: VecDeque<Event>,
    owner_q: VecDeque<Event>,
}

fn ov(v: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(v).expect("a plain value converts")
}

/// The properties a player gives, as the bus carries them.
fn props(m: &Media) -> (HashMap<String, OwnedValue>, HashMap<String, OwnedValue>) {
    let mut meta: HashMap<String, Value<'static>> = HashMap::new();
    meta.insert("xesam:title".into(), Value::from(m.title.to_string()));
    if !m.artist.is_empty() {
        meta.insert(
            "xesam:artist".into(),
            Value::from(vec![m.artist.to_string()]),
        );
    }
    if let Some(a) = m.art {
        meta.insert(
            "mpris:artUrl".into(),
            Value::from(format!("file:///art/{a}")),
        );
    }
    if m.length > 0 {
        meta.insert("mpris:length".into(), Value::from(m.length));
    }
    if let Some((k, pid)) = m.meta_pid {
        meta.insert(k.into(), Value::from(i64::from(pid)));
    }
    let mut player = HashMap::new();
    player.insert("Metadata".to_string(), ov(Value::from(meta)));
    player.insert(
        "PlaybackStatus".to_string(),
        ov(Value::from(if m.playing { "Playing" } else { "Paused" })),
    );
    for k in ["CanGoNext", "CanGoPrevious", "CanSeek"] {
        player.insert(k.to_string(), ov(Value::from(true)));
    }
    let mut root = HashMap::new();
    root.insert("Identity".to_string(), ov(Value::from(m.identity)));
    (player, root)
}

impl World {
    fn new(seed: u64) -> World {
        World {
            rng: Rng::new(seed),
            next_id: 1,
            names: BTreeMap::new(),
            apps: BTreeMap::new(),
            props_q: VecDeque::new(),
            owner_q: VecDeque::new(),
        }
    }

    fn media(&mut self, identity: &'static str, meta_pid: Option<(&'static str, u32)>) -> Media {
        Media {
            identity,
            title: self.rng.pick(&TITLES),
            artist: self.rng.pick(&ARTISTS),
            art: self.rng.chance(50).then(|| self.rng.below(4) as u8),
            playing: self.rng.chance(50),
            length: if self.rng.chance(50) { 200_000_000 } else { 0 },
            meta_pid,
        }
    }

    /// A program appears and takes its names (or takes one over, `--replace`).
    fn spawn(&mut self) {
        let pid = *self.rng.pick(&PIDS);
        let p = |s: &str| format!("org.mpris.MediaPlayer2.{s}");
        let (identity, names, meta_pid): (&str, Vec<String>, _) = match self.rng.below(7) {
            0 => ("Spotify", vec![p("spotify")], None),
            // VLC owns two names for one player.
            1 => (
                "VLC",
                vec![p("vlc"), p(&format!("vlc.instance{pid}"))],
                None,
            ),
            2 => ("mpv", vec![p(&format!("mpv.instance{pid}"))], None),
            3 => (
                "Chromium",
                vec![p(&format!("chromium.instance{pid}"))],
                None,
            ),
            4 => (
                "kestrel media",
                vec![p(&format!("kestrel_browser.instance{pid}"))],
                self.rng.chance(50).then_some(("kestrel:pid", pid)),
            ),
            5 => (
                "Chromium",
                vec![p("plasma-browser-integration")],
                Some(("kde:pid", pid)),
            ),
            // playerctld mirrors the others and is not followed.
            _ => ("playerctld", vec![p("playerctld")], None),
        };
        let owner = format!(":1.{}", self.next_id);
        self.next_id += 1;
        let media = self.media(identity, meta_pid);
        let ready = self.rng.chance(85);
        let mut taken = Vec::new();
        for n in names {
            match self.names.get(&n).cloned() {
                None => {}
                Some(old) if self.rng.chance(50) => {
                    if let Some(a) = self.apps.get_mut(&old) {
                        a.names.retain(|x| *x != n);
                    }
                }
                Some(_) => continue,
            }
            self.names.insert(n.clone(), owner.clone());
            self.owner_q.push_back(Event::Owner {
                name: n.clone(),
                new: owner.clone(),
            });
            taken.push(n);
        }
        if taken.is_empty() {
            return;
        }
        self.apps.insert(
            owner,
            App {
                names: taken,
                media,
                ready,
                unsignalled: false,
            },
        );
    }

    fn some_app(&mut self) -> Option<String> {
        let owners: Vec<String> = self.apps.keys().cloned().collect();
        (!owners.is_empty()).then(|| self.rng.pick(&owners).clone())
    }

    /// A player says something changed, unless the signal is lost.
    fn signal(&mut self, owner: &str) {
        if self.rng.chance(10) {
            if let Some(a) = self.apps.get_mut(owner) {
                a.unsignalled = true;
            }
        } else {
            self.props_q.push_back(Event::Changed {
                sender: owner.to_string(),
            });
        }
    }

    fn leave(&mut self, name: &str) {
        self.names.remove(name);
        self.owner_q.push_back(Event::Owner {
            name: name.to_string(),
            new: String::new(),
        });
    }

    /// One random thing happens on the bus.
    fn step(&mut self) {
        match self.rng.below(10) {
            0 | 1 if self.apps.len() < 6 => self.spawn(),
            2..=4 => {
                let Some(o) = self.some_app() else { return };
                let (identity, meta_pid) = {
                    let m = &self.apps[&o].media;
                    (m.identity, m.meta_pid)
                };
                let mut fresh = self.media(identity, meta_pid);
                // Usually one field moves, sometimes all of them (a new track).
                if self.rng.chance(70) {
                    let old = self.apps[&o].media.clone();
                    fresh = match self.rng.below(4) {
                        0 => Media {
                            title: fresh.title,
                            ..old
                        },
                        1 => Media {
                            playing: !old.playing,
                            ..old
                        },
                        2 => Media {
                            art: fresh.art,
                            ..old
                        },
                        _ => Media {
                            artist: fresh.artist,
                            length: fresh.length,
                            ..old
                        },
                    };
                }
                if let Some(a) = self.apps.get_mut(&o) {
                    a.media = fresh;
                }
                self.signal(&o);
            }
            5 => {
                // Not ready until now: its object is exported and it says so.
                let Some(o) = self.some_app() else { return };
                if let Some(a) = self.apps.get_mut(&o)
                    && !a.ready
                {
                    a.ready = true;
                    self.signal(&o);
                }
            }
            6 => {
                // A program quits: each of its names goes.
                let Some(o) = self.some_app() else { return };
                if let Some(a) = self.apps.remove(&o) {
                    for n in a.names {
                        self.leave(&n);
                    }
                }
            }
            7 => {
                // A program drops one name and keeps running.
                let Some(o) = self.some_app() else { return };
                let Some(n) = self.apps.get_mut(&o).and_then(|a| a.names.pop()) else {
                    return;
                };
                self.leave(&n);
            }
            8 => {
                // A signal repeated with nothing changed.
                let Some(o) = self.some_app() else { return };
                self.props_q.push_back(Event::Changed { sender: o });
            }
            _ => {
                // A signal from a program already gone.
                self.props_q.push_back(Event::Changed {
                    sender: ":1.9999".into(),
                });
            }
        }
    }

    /// What the service's `learn` would get for `name` now. `busy` lets the world move between
    /// the calls (GetNameOwner, then GetAll of the player, then GetAll of the root).
    fn learn(&mut self, name: &str, busy: bool) -> Option<Known> {
        let owner = self.names.get(name)?.clone();
        if busy && self.rng.chance(30) {
            self.step();
        }
        // A call goes to whoever owns the name when it arrives; it fails when no one does, or
        // the object is not there yet.
        let player = self
            .names
            .get(name)
            .and_then(|o| self.apps.get(o))
            .filter(|a| a.ready)
            .map(|a| props(&a.media).0);
        if busy && self.rng.chance(30) {
            self.step();
        }
        let root = self
            .names
            .get(name)
            .and_then(|o| self.apps.get(o))
            .filter(|a| a.ready)
            .map(|a| props(&a.media).1)
            .unwrap_or_default();
        Some(Known {
            owner,
            player,
            root,
        })
    }

    /// The driver handling the next signal (from either stream).
    fn deliver(&mut self, t: &mut Tracker, busy: bool) -> bool {
        let from_props = match (self.props_q.is_empty(), self.owner_q.is_empty()) {
            (true, true) => return false,
            (false, true) => true,
            (true, false) => false,
            (false, false) => self.rng.chance(50),
        };
        let e = if from_props {
            self.props_q.pop_front()
        } else {
            self.owner_q.pop_front()
        };
        super::trace(|| format!("handle {e:?}"));
        if let Some(name) = e.and_then(|e| t.event(e)) {
            let k = self.learn(&name, busy);
            super::trace(|| format!("  read {name}: {:?}", k.as_ref().map(|k| &k.owner)));
            t.learned(name, k);
        }
        true
    }

    /// The end of a burst: each player that signalled read again.
    fn settle(&mut self, t: &mut Tracker, busy: bool) {
        for bus in t.take_stale() {
            let k = self.learn(&bus, busy);
            t.learned(bus, k);
        }
    }

    /// What a fresh start would see: every followed name read.
    fn full_read(&mut self) -> Tracker {
        let mut t = Tracker::default();
        let names: Vec<String> = self.names.keys().cloned().collect();
        for n in names {
            if let Some(name) = t.event(Event::Owner {
                name: n.clone(),
                new: self.names[&n].clone(),
            }) {
                let k = self.learn(&name, false);
                t.learned(name, k);
            }
        }
        t
    }

    /// The world goes quiet: lost signals are sent again (the next change of that player),
    /// and every signal is handled.
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
            }
            self.props_q.push_back(Event::Changed { sender: o });
        }
        while self.deliver(t, false) {}
        self.settle(t, false);
    }
}

/// Checked after every step.
fn always(s: &State) -> Result<(), String> {
    for (i, a) in s.players.iter().enumerate() {
        if a.title.trim().is_empty() {
            return Err(format!("an untitled player is shown: {a:?}"));
        }
        if let Some(b) = s.players[i + 1..].iter().find(|b| same_media(a, b)) {
            return Err(format!("one media shown twice:\n  {a:?}\n  {b:?}"));
        }
    }
    Ok(())
}

/// Checked when the world is quiet: the tracker equals a full re-read.
fn settled(w: &mut World, t: &Tracker) -> Result<(), String> {
    let truth = w.full_read();
    let (have, want) = (t.snapshot(), truth.snapshot());
    if have != want {
        let names = |v: &[(String, String, crate::mpris::Player)]| {
            v.iter()
                .map(|(n, o, p)| format!("{n} ({o}) {:?}", p.title))
                .collect::<Vec<_>>()
        };
        return Err(format!(
            "after the quiet period the tracker holds\n  {:?}\nbut the bus has\n  {:?}",
            names(&have),
            names(&want)
        ));
    }
    if t.state() != truth.state() {
        return Err(format!(
            "after the quiet period the tracker shows\n  {:?}\nbut a fresh read shows\n  {:?}",
            t.state(),
            truth.state()
        ));
    }
    Ok(())
}

const STEPS: usize = 300;

/// One seeded run; the digest is every quiet state seen.
fn run(seed: u64) -> Result<u64, String> {
    let mut w = World::new(seed);
    let mut digest = Digest::new();
    // Some players exist before the service starts; it subscribes, then lists and reads them
    // while the world goes on.
    for _ in 0..w.rng.below(8) {
        w.step();
    }
    w.props_q.clear();
    w.owner_q.clear();
    let mut t = Tracker::default();
    let names: Vec<String> = w.names.keys().cloned().collect();
    for n in names {
        if crate::mpris::followed(&n) {
            let k = w.learn(&n, true);
            t.learned(n, k);
        }
    }
    for step in 0..STEPS {
        match w.rng.below(20) {
            0..=7 => w.step(),
            8..=16 => {
                w.deliver(&mut t, true);
            }
            _ => w.settle(&mut t, true),
        }
        always(&t.state()).map_err(|e| format!("step {step}: {e}"))?;
        if step % 50 == 49 || step == STEPS - 1 {
            w.quiet(&mut t);
            settled(&mut w, &t).map_err(|e| format!("step {step}: {e}"))?;
            digest.add(&format!("{:?}", t.state()));
        }
    }
    Ok(digest.value())
}

#[test]
fn seeds() {
    super::seeds("mpris", 100, run);
}
