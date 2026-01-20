//! Media players over MPRIS on the session bus.
//!
//! A browser often appears twice (its own player and the browser-integration extension, which
//! carries the artwork and artist). Players without a title are hidden, and players showing
//! the same title are merged into one.

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
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
    /// The process behind it: a browser's own player ends its bus name in `.instance<PID>`,
    /// the browser-integration player carries `kde:pid`. Used to join the two.
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

async fn read_player(c: &Connection, bus: &str) -> zbus::Result<Player> {
    let p = dbus::proxy(c, bus, PATH, PLAYER).await?;
    let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.unwrap_or_default();
    let status: String = p.get_property("PlaybackStatus").await.unwrap_or_default();
    let identity: String = dbus::get(c, bus, PATH, ROOT, "Identity")
        .await
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
    let pid = meta
        .get("kde:pid")
        .and_then(|v| {
            i64::try_from(v.clone())
                .ok()
                .or_else(|| u32::try_from(v.clone()).ok().map(i64::from))
        })
        .and_then(|p| u32::try_from(p).ok())
        .or_else(|| bus.rsplit(".instance").next().and_then(|p| p.parse().ok()));
    Ok(Player {
        bus: bus.to_string(),
        identity,
        // Pages put invisible direction marks and non-breaking spaces in titles.
        title: clean(&s(&meta, "xesam:title").unwrap_or_default()),
        artist,
        album: s(&meta, "xesam:album").unwrap_or_default(),
        art,
        playing: status == "Playing",
        length,
        can_next: p.get_property("CanGoNext").await.unwrap_or(false),
        can_prev: p.get_property("CanGoPrevious").await.unwrap_or(false),
        can_seek: p.get_property("CanSeek").await.unwrap_or(false),
        pid,
    })
}

/// One browser tab can be reported twice: by the browser itself (true playback state,
/// position and controls, but a generic title like "Apple Music - Web Player") and by the
/// browser integration (the song, artist and cover, but a playback state that goes stale).
/// Join them: what is playing from the integration, how it is playing from the browser.
fn join_browser_players(players: &mut Vec<Player>) {
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

async fn read(c: &Connection) -> zbus::Result<State> {
    let names: Vec<String> = dbus::proxy(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?
    .call("ListNames", &())
    .await?;
    let mut all: Vec<Player> = Vec::new();
    for n in names.iter().filter(|n| n.starts_with(PREFIX)) {
        // playerctld mirrors other players.
        if n.ends_with(".playerctld") {
            continue;
        }
        if let Ok(p) = read_player(c, n).await {
            all.push(p);
        }
    }
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
    Ok(State { players })
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
        let c = Connection::session().await?;
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
    let c = Connection::session().await.ok()?;
    dbus::get(&c, &bus, PATH, PLAYER, "Position").await.ok()
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
        let (Ok(a), Ok(b)) = (
            MessageStream::for_match_rule(props, &c, Some(64)).await,
            MessageStream::for_match_rule(owners, &c, Some(16)).await,
        ) else {
            return;
        };
        let merged = futures::stream::select(a.map(|_| ()), b.map(|_| ()));
        let mut changes = Box::pin(dbus::debounce(merged, std::time::Duration::from_millis(60)));
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

#[cfg(test)]
mod join_tests {
    use super::*;

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
