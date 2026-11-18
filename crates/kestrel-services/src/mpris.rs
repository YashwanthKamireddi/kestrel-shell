//! Media players over MPRIS on the session bus.
//!
//! A browser often appears twice (its own player and the browser-integration extension, which
//! carries the artwork and artist). Players without a title are hidden, and players showing
//! the same title are merged into one.

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::{HashMap, HashSet};
use zbus::zvariant::OwnedValue;
use zbus::{Connection, MatchRule, MessageStream};

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PREFIX: &str = "org.mpris.MediaPlayer2.";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    /// Bus name commands go to.
    pub bus: String,
    pub identity: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Local file path of the artwork.
    pub art: Option<String>,
    pub playing: bool,
    /// Microseconds.
    pub length: i64,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_seek: bool,
    /// The browser behind it: a browser's own player ends its bus name in `.instance<PID>`,
    /// the integration players carry it in their metadata (`kestrel:pid`, `kde:pid`). Used to
    /// join the two.
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub players: Vec<Player>,
}

impl State {
    /// The player to show: the first one playing, else the first.
    pub fn main(&self) -> Option<&Player> {
        self.players
            .iter()
            .find(|p| p.playing)
            .or(self.players.first())
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    PlayPause(String),
    Next(String),
    Previous(String),
    /// Absolute position in microseconds.
    Seek(String, i64),
}

fn clean(t: &str) -> String {
    t.chars()
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{200b}' | '\u{feff}'))
        .map(|c| if c == '\u{a0}' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

fn s(m: &HashMap<String, OwnedValue>, k: &str) -> Option<String> {
    m.get(k).and_then(|v| String::try_from(v.clone()).ok())
}

/// What is known of one player, as read from it. Read whole when it appears and again, whole,
/// whenever it signals a change (the signal only says "look again": truth over events,
/// docs/ROADMAP-v2.md rule 1). Only the player that signalled is read: before, any signal from
/// any player re-listed the bus and re-read every player (~6 calls per player per signal).
#[derive(Clone, Debug, Default)]
struct Known {
    /// Its unique name (`:1.42`): its signals come from it.
    owner: String,
    player: HashMap<String, OwnedValue>,
    root: HashMap<String, OwnedValue>,
}

/// Reads a player that appeared: who owns the name, and every property, in one call each.
async fn learn(c: &Connection, bus: &str) -> Option<Known> {
    let name = zbus::names::BusName::try_from(bus.to_string()).ok()?;
    let owner = zbus::fdo::DBusProxy::new(c)
        .await
        .ok()?
        .get_name_owner(name)
        .await
        .ok()?
        .to_string();
    Some(Known {
        owner,
        player: dbus::get_all(c, bus, PATH, PLAYER).await.ok()?,
        root: dbus::get_all(c, bus, PATH, ROOT).await.unwrap_or_default(),
    })
}

fn flag(m: &HashMap<String, OwnedValue>, k: &str) -> bool {
    m.get(k)
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false)
}

fn player(bus: &str, k: &Known) -> Player {
    let meta: HashMap<String, OwnedValue> = k
        .player
        .get("Metadata")
        .and_then(|v| HashMap::try_from(v.clone()).ok())
        .unwrap_or_default();
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        .map(|a| a.join(", "))
        .or_else(|| s(&meta, "xesam:artist"))
        .unwrap_or_default();
    let length = meta
        .get("mpris:length")
        .and_then(|v| {
            i64::try_from(v.clone())
                .ok()
                .or_else(|| u64::try_from(v.clone()).ok().map(|u| u as i64))
        })
        .unwrap_or(0);
    let art = s(&meta, "mpris:artUrl").and_then(|u| u.strip_prefix("file://").map(str::to_string));
    // kestrel's own browser host says it (`kestrel:pid`), as KDE's integration does (`kde:pid`).
    let pid = meta
        .get("kestrel:pid")
        .or_else(|| meta.get("kde:pid"))
        .and_then(|v| {
            i64::try_from(v.clone())
                .ok()
                .or_else(|| u32::try_from(v.clone()).ok().map(i64::from))
        })
        .and_then(|p| u32::try_from(p).ok())
        .or_else(|| bus.rsplit(".instance").next().and_then(|p| p.parse().ok()));
    Player {
        bus: bus.to_string(),
        identity: s(&k.root, "Identity").unwrap_or_default(),
        // Pages put invisible direction marks and non-breaking spaces in titles.
        title: clean(&s(&meta, "xesam:title").unwrap_or_default()),
        artist,
        album: s(&meta, "xesam:album").unwrap_or_default(),
        art,
        playing: s(&k.player, "PlaybackStatus").as_deref() == Some("Playing"),
        length,
        can_next: flag(&k.player, "CanGoNext"),
        can_prev: flag(&k.player, "CanGoPrevious"),
        can_seek: flag(&k.player, "CanSeek"),
        pid,
    }
}

/// One browser tab can be reported twice: by the browser itself (true playback state,
/// position and controls, but a generic title like "Apple Music - Web Player") and by the
/// browser integration (the song, artist and cover, but a playback state that goes stale).
/// Join them: what is playing from the integration, how it is playing from the browser.
fn join_browser_players(players: &mut Vec<Player>) {
    // kestrel's own browser player (the "kestrel media" extension) reads the page itself and
    // is exact: with it there, the browser's generic player and the integration's go.
    // Only one that knows what is playing counts (a freshly started helper has nothing yet).
    // It is stale when the browser says something plays and it says nothing does: the
    // playing tab is one it does not follow (an old Apple Music tab paused, YouTube playing in
    // another). Then the browser's own player is the truth.
    // It is also stale when the browser is playing something else: the helper follows one tab
    // and can keep saying "playing" for it after you moved to another (2026-11-15: it showed
    // the Omarchy video at 0:00 while the herdr video played; the browser's player was right).
    let norm = |t: &str| {
        let t = t.trim().to_lowercase();
        t.strip_suffix(" - youtube")
            .unwrap_or(&t)
            .trim()
            .to_string()
    };
    let same = |a: &str, b: &str| {
        let (a, b) = (norm(a), norm(b));
        a.is_empty() || b.is_empty() || a.contains(&b) || b.contains(&a)
    };
    // The browser's own player leads whenever there is one for that browser: its state, position
    // and controls have been right every time, while the helper fell behind (the wrong tab,
    // "Stopped" with no title while a video played: 0:00, paused, no visualiser). The helper only
    // fills in what the browser's player lacks for the same video (an exact title, a picture),
    // then goes.
    let helpers: Vec<Player> = players
        .iter()
        .filter(|p| p.bus.contains(".kestrel_browser."))
        .cloned()
        .collect();
    let is_native = |p: &Player| {
        !p.bus.contains(".kestrel_browser.")
            && !p.bus.contains("browser_integration")
            && !p.bus.contains("browser-integration")
    };
    let mut led: Vec<String> = Vec::new();
    for n in players
        .iter_mut()
        .filter(|p| is_native(p) && p.pid.is_some())
    {
        for h in helpers.iter().filter(|h| h.pid == n.pid) {
            led.push(h.bus.clone());
            if !h.title.trim().is_empty() && same(&n.title, &h.title) {
                if n.art.is_none() {
                    n.art.clone_from(&h.art);
                }
                if h.title.len() > n.title.len() {
                    n.title.clone_from(&h.title);
                }
                if n.artist.trim().is_empty() {
                    n.artist.clone_from(&h.artist);
                }
            }
        }
    }
    players.retain(|p| !led.contains(&p.bus));
    let stale = |helper: &Player, all: &[Player]| {
        !helper.playing
            && all
                .iter()
                .any(|p| !p.bus.contains(".kestrel_browser.") && p.pid == helper.pid && p.playing)
    };
    // The browser's own player for the same video as a trusted helper is a duplicate; one for
    // a different video (two tabs, two windows) is another player and stays.
    let duplicate = |p: &Player, helpers: &[&Player]| {
        helpers
            .iter()
            .any(|h| h.pid == p.pid && same(&h.title, &p.title))
    };
    let snapshot = players.clone();
    let trusted = |p: &Player| {
        p.bus.contains(".kestrel_browser.") && !p.title.trim().is_empty() && !stale(p, &snapshot)
    };
    let own: Vec<u32> = players
        .iter()
        .filter(|p| trusted(p))
        .filter_map(|p| p.pid)
        .collect();
    if !own.is_empty() {
        // The page may not give a picture (YouTube live, some Shorts); the browser's own player
        // for the same tab usually has its thumbnail: borrow it rather than show none.
        for helper in players.iter_mut() {
            if trusted(helper) && helper.art.is_none() {
                let pid = helper.pid;
                // Only from the same video: another tab's thumbnail would be wrong.
                let title = helper.title.clone();
                helper.art = snapshot
                    .iter()
                    .find(|p| {
                        !p.bus.contains(".kestrel_browser.")
                            && p.pid == pid
                            && p.art.is_some()
                            && same(&p.title, &title)
                    })
                    .and_then(|p| p.art.clone());
            }
        }
        let helpers: Vec<Player> = snapshot.iter().filter(|p| trusted(p)).cloned().collect();
        let helpers: Vec<&Player> = helpers.iter().collect();
        players.retain(|p| {
            trusted(p)
                || !(p.bus.contains(".kestrel_browser.")
                    || (p.pid.is_some_and(|pid| own.contains(&pid)) && duplicate(p, &helpers)))
        });
        return;
    }
    // No helper to trust: its stale entries go, the browser's own players stay.
    players.retain(|p| !p.bus.contains(".kestrel_browser.") || trusted(p));
    let integration: Vec<Player> = players
        .iter()
        .filter(|p| p.bus.contains("browser_integration") || p.bus.contains("browser-integration"))
        .cloned()
        .collect();
    for rich in integration {
        let Some(pid) = rich.pid else {
            continue;
        };
        let Some(native) = players
            .iter_mut()
            .find(|p| p.bus != rich.bus && p.pid == Some(pid))
        else {
            continue;
        };
        native.title = rich.title.clone();
        native.artist = rich.artist.clone();
        native.album = rich.album.clone();
        if rich.art.is_some() {
            native.art = rich.art.clone();
        }
        players.retain(|p| p.bus != rich.bus);
    }
}

/// Richer metadata wins when two players show the same thing.
fn richness(p: &Player) -> u8 {
    u8::from(p.art.is_some()) * 2 + u8::from(!p.artist.is_empty()) + u8::from(p.length > 0)
}

/// The players to show, from what is known of each.
fn build(known: &HashMap<String, Known>) -> State {
    let mut all: Vec<Player> = known.iter().map(|(bus, k)| player(bus, k)).collect();
    join_browser_players(&mut all);
    let mut players: Vec<Player> = Vec::new();
    for p in all {
        if p.title.trim().is_empty() {
            continue;
        }
        if let Some(dup) = players.iter_mut().find(|q| same_media(q, &p)) {
            if richness(&p) > richness(dup) {
                *dup = p;
            }
        } else {
            players.push(p);
        }
    }
    // A browser often keeps a bare placeholder ("Apple Music - Web Player": no cover, no
    // artist, no length) next to the real track; drop it when the same app has a real one.
    let real: Vec<String> = players
        .iter()
        .filter(|p| richness(p) > 0)
        .map(|p| p.identity.clone())
        .collect();
    players.retain(|p| richness(p) > 0 || !real.contains(&p.identity));
    players.sort_by(|a, b| b.playing.cmp(&a.playing).then(a.bus.cmp(&b.bus)));
    State { players }
}

fn same_media(a: &Player, b: &Player) -> bool {
    let norm = |t: &str| {
        t.trim()
            .trim_end_matches(" - YouTube")
            .trim_end_matches(" - YouTube Music")
            .to_lowercase()
    };
    norm(&a.title) == norm(&b.title)
}

pub async fn command(cmd: Command) {
    let r = async {
        let c = dbus::session().await?;
        match &cmd {
            Command::PlayPause(b) => {
                dbus::proxy(&c, b, PATH, PLAYER)
                    .await?
                    .call::<_, _, ()>("PlayPause", &())
                    .await
            }
            Command::Next(b) => {
                dbus::proxy(&c, b, PATH, PLAYER)
                    .await?
                    .call("Next", &())
                    .await
            }
            Command::Previous(b) => {
                dbus::proxy(&c, b, PATH, PLAYER)
                    .await?
                    .call("Previous", &())
                    .await
            }
            Command::Seek(b, pos) => {
                let p = dbus::proxy(&c, b, PATH, PLAYER).await?;
                let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await?;
                let track = meta
                    .get("mpris:trackid")
                    .and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v.clone()).ok());
                match track {
                    Some(t) => p.call("SetPosition", &(t, *pos)).await,
                    None => Ok(()),
                }
            }
        }
    }
    .await;
    if let Err(e) = r {
        log::warn!("mpris {cmd:?}: {e}");
    }
}

/// Current position (microseconds); MPRIS does not signal it, so it is read on demand.
pub async fn position(bus: String) -> Option<i64> {
    let c = dbus::session().await.ok()?;
    dbus::get(&c, &bus, PATH, PLAYER, "Position").await.ok()
}

/// A player's name worth following (playerctld only mirrors the others).
fn followed(name: &str) -> bool {
    name.starts_with(PREFIX) && !name.ends_with(".playerctld")
}

/// One signal: a player that said something changed is marked to be read again (`dirty`, its
/// unique name); a player appearing is read, one leaving goes.
async fn apply(
    c: &Connection,
    known: &mut HashMap<String, Known>,
    dirty: &mut HashSet<String>,
    m: &zbus::Message,
) {
    let h = m.header();
    match h.member().map(|m| m.as_str()) {
        Some("PropertiesChanged") => {
            if let Some(sender) = h.sender() {
                dirty.insert(sender.to_string());
            }
        }
        Some("NameOwnerChanged") => {
            let Ok((name, _old, new)) = m.body().deserialize::<(String, String, String)>() else {
                return;
            };
            if !followed(&name) {
                return;
            }
            match (new.is_empty(), learn(c, &name).await) {
                (false, Some(k)) => known.insert(name, k),
                _ => known.remove(&name),
            };
        }
        _ => {}
    }
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::session().await else {
            return;
        };
        let props = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .and_then(|b| b.member("PropertiesChanged"))
            .and_then(|b| b.path(PATH))
            .map(|b| b.build());
        let owners = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus")
            .and_then(|b| b.member("NameOwnerChanged"))
            .and_then(|b| b.arg0ns(ROOT))
            .map(|b| b.build());
        let (Ok(props), Ok(owners)) = (props, owners) else {
            return;
        };
        // Listening before the first read: nothing said in between is missed.
        let (Ok(a), Ok(b)) = (
            MessageStream::for_match_rule(props, &c, Some(64)).await,
            MessageStream::for_match_rule(owners, &c, Some(16)).await,
        ) else {
            return;
        };
        let mut events = Box::pin(futures::stream::select(a, b).filter_map(|m| async { m.ok() }));
        let mut known: HashMap<String, Known> = HashMap::new();
        let names = match zbus::fdo::DBusProxy::new(&c).await {
            Ok(d) => d.list_names().await.unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        for n in names.iter().map(|n| n.to_string()).filter(|n| followed(n)) {
            if let Some(k) = learn(&c, &n).await {
                known.insert(n, k);
            }
        }
        // Signals that come together (a new track: title, art, length) are shown together.
        const QUIET: std::time::Duration = std::time::Duration::from_millis(60);
        let mut last: Option<State> = None;
        // A browser drops its player and makes a new one on every switch (the next Short, the
        // next video); shown as is, the music bubble vanished and came back each time. A
        // player that leaves is kept for a moment: if one comes back, it simply updates.
        const GRACE: std::time::Duration = std::time::Duration::from_millis(1500);
        let mut hold_until: Option<tokio::time::Instant> = None;
        loop {
            let s = build(&known);
            let lost = last
                .as_ref()
                .is_some_and(|l| !l.players.is_empty() && s.players.len() < l.players.len());
            let mut first = None;
            if lost {
                let deadline =
                    *hold_until.get_or_insert_with(|| tokio::time::Instant::now() + GRACE);
                if tokio::time::Instant::now() < deadline {
                    match tokio::time::timeout_at(deadline, events.next()).await {
                        Ok(None) => return,
                        Ok(Some(m)) => first = Some(m),
                        // The moment passed: look again.
                        Err(_) => continue,
                    }
                }
            }
            if first.is_none() {
                hold_until = None;
                if last.as_ref() != Some(&s) {
                    last = Some(s.clone());
                    let _ = out.send(s).await;
                }
                match events.next().await {
                    Some(m) => first = Some(m),
                    None => return,
                }
            }
            let Some(m) = first else { continue };
            let mut dirty = HashSet::new();
            apply(&c, &mut known, &mut dirty, &m).await;
            while let Ok(m) = tokio::time::timeout(QUIET, events.next()).await {
                let Some(m) = m else { return };
                apply(&c, &mut known, &mut dirty, &m).await;
            }
            // Each player that signalled, read again whole (once, however many signals).
            let stale: Vec<String> = known
                .iter()
                .filter(|(_, k)| dirty.contains(&k.owner))
                .map(|(bus, _)| bus.clone())
                .collect();
            for bus in stale {
                match learn(&c, &bus).await {
                    Some(k) => known.insert(bus, k),
                    None => known.remove(&bus),
                };
            }
        }
    })
}

#[cfg(test)]
mod join_tests {
    use super::*;

    #[test]
    fn a_helper_without_a_picture_borrows_the_browsers() {
        let native = Player {
            bus: "org.mpris.MediaPlayer2.chromium.instance1".into(),
            title: "Video".into(),
            playing: true,
            pid: Some(1),
            art: Some("/tmp/thumb".into()),
            ..Player::default()
        };
        let helper = Player {
            bus: "org.mpris.MediaPlayer2.kestrel_browser.instance1".into(),
            title: "Video, exact title".into(),
            playing: true,
            pid: Some(1),
            ..Player::default()
        };
        let mut v = vec![native, helper];
        join_browser_players(&mut v);
        // The browser's player leads (its controls and position), with the helper's exact title.
        assert_eq!(v.len(), 1);
        assert!(v[0].bus.contains(".chromium."));
        assert_eq!(v[0].title, "Video, exact title");
        assert_eq!(v[0].art.as_deref(), Some("/tmp/thumb"));
    }

    #[test]
    fn the_browsers_own_player_leads() {
        let native = Player {
            bus: "org.mpris.MediaPlayer2.chromium.instance1".into(),
            title: "I LOVE the herdr desktop app".into(),
            playing: true,
            pid: Some(1),
            ..Player::default()
        };
        let helper = Player {
            bus: "org.mpris.MediaPlayer2.kestrel_browser.instance1".into(),
            title: "So I've Been Using Omarchy for 357 Days".into(),
            playing: true,
            pid: Some(1),
            ..Player::default()
        };
        let mut v = vec![native, helper];
        join_browser_players(&mut v);
        // Chromium's own player is the truth for its browser; the helper's other tab goes.
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title, "I LOVE the herdr desktop app");
    }

    #[test]
    fn a_stale_helper_loses_to_the_browser_playing() {
        let native = Player {
            bus: "org.mpris.MediaPlayer2.chromium.instance1".into(),
            title: "A YouTube video".into(),
            playing: true,
            pid: Some(1),
            ..Player::default()
        };
        let helper = Player {
            bus: "org.mpris.MediaPlayer2.kestrel_browser.instance1".into(),
            title: "Monday".into(),
            playing: false,
            pid: Some(1),
            ..Player::default()
        };
        let mut v = vec![native, helper];
        join_browser_players(&mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title, "A YouTube video");
        assert!(v[0].playing);
    }

    #[test]
    fn a_tab_reported_twice_becomes_one_player() {
        let native = Player {
            bus: "org.mpris.MediaPlayer2.chromium.instance132820".into(),
            identity: "Chromium".into(),
            title: "Apple Music - Web Player".into(),
            playing: true,
            length: 620_000_000,
            pid: Some(132_820),
            ..Player::default()
        };
        let rich = Player {
            bus: "org.mpris.MediaPlayer2.plasma-browser-integration".into(),
            identity: "Chromium".into(),
            title: "Cry".into(),
            artist: "Charlie Puth & Kenny G".into(),
            art: Some("/tmp/a.jpg".into()),
            playing: false,
            pid: Some(132_820),
            ..Player::default()
        };
        let mut v = vec![native, rich];
        join_browser_players(&mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title, "Cry");
        assert!(v[0].playing, "playback state comes from the browser");
        assert_eq!(v[0].bus, "org.mpris.MediaPlayer2.chromium.instance132820");
        assert_eq!(v[0].art.as_deref(), Some("/tmp/a.jpg"));
    }
}
