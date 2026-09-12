//! Sound through the PulseAudio protocol (served by pipewire-pulse). One thread owns the
//! PulseAudio context; changes arrive as events, and kestrel's commands go to that thread
//! through a channel.

use futures::{SinkExt, Stream};
use libpulse_binding as pulse;
use pulse::callbacks::ListResult;
use pulse::context::subscribe::{Facility, InterestMaskSet};
use pulse::context::{Context, FlagSet, State as CtxState};
use pulse::mainloop::threaded::Mainloop;
use pulse::volume::{ChannelVolumes, Volume};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

mod record;
mod samples;
use record::*;
use samples::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub name: String,
    pub description: String,
    /// 0.0..=1.5 (1.0 is 100 %).
    pub volume: f32,
    pub muted: bool,
    pub channels: u8,
    pub bluetooth: bool,
    pub headphones: bool,
    /// Something is plugged into it (an HDMI output with no screen, or a jack with nothing in
    /// it, is not), as far as the card can tell.
    pub plugged: bool,
    /// Its sound card (for the card's modes), if it has one.
    pub card: Option<u32>,
    /// Left/right balance, -1 (left) to 1 (right); 0 for mono.
    pub balance: f32,
}

/// A sound card and the modes it can run in (Bluetooth: high-quality playback or headset with
/// microphone; a laptop: analogue, HDMI…).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub index: u32,
    pub name: String,
    /// The mode in use (its name).
    pub active: String,
    /// The modes it can switch to: (name, description), unavailable ones left out.
    pub profiles: Vec<(String, String)>,
}

/// An app playing sound (all its streams together, like a volume mixer's row).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct App {
    /// Its name as it gives it ("Chromium", "Spotify").
    pub name: String,
    /// Its icon name, or its program's name to look one up by.
    pub icon: String,
    /// Its streams (a browser has one per tab playing).
    pub streams: Vec<u32>,
    /// 0.0..=1.5, the loudest of its streams.
    pub volume: f32,
    /// Every stream muted.
    pub muted: bool,
    pub channels: u8,
    /// The output it plays on (a sink name), when all its streams share one.
    pub sink: String,
}

impl App {
    /// Its icon file: the name it gives, else its desktop entry's icon (Chromium says
    /// "chromium-browser", which themes do not have), else its name.
    pub fn icon_path(&self) -> Option<std::path::PathBuf> {
        crate::icons::lookup(&self.icon)
            .or_else(|| crate::icons::lookup(&crate::apps::icon_for_class(&self.name)))
            .or_else(|| crate::icons::lookup(&crate::apps::icon_for_class(&self.icon)))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// Apps playing sound, in a steady order (by name).
    pub apps: Vec<App>,
    /// Apps recording (a microphone), grouped the same way; `sink` is the source they record.
    pub recorders: Vec<App>,
    pub cards: Vec<Card>,
    pub sinks: Vec<Device>,
    pub sources: Vec<Device>,
    pub default_sink: String,
    pub default_source: String,
    /// Apps currently recording from a microphone.
    pub recording: Vec<String>,
}

impl State {
    pub fn sink(&self) -> Option<&Device> {
        self.sinks.iter().find(|d| d.name == self.default_sink)
    }
    pub fn source(&self) -> Option<&Device> {
        self.sources.iter().find(|d| d.name == self.default_source)
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    SinkVolume(String, u8, f32),
    SinkMute(String, bool),
    SourceVolume(String, u8, f32),
    SourceMute(String, bool),
    DefaultSink(String),
    DefaultSource(String),
    /// An app's streams (their indices), to this level.
    AppVolume(Vec<u32>, u8, f32),
    AppMute(Vec<u32>, bool),
    /// An app's streams moved to this output.
    AppMove(Vec<u32>, String),
    /// A recording app's streams: level, mute, the microphone they record.
    RecorderVolume(Vec<u32>, u8, f32),
    RecorderMute(Vec<u32>, bool),
    RecorderMove(Vec<u32>, String),
    /// An output's left/right balance (-1 to 1).
    SinkBalance(String, f32),
    /// A card's mode (card index, profile name).
    CardProfile(u32, String),
    /// An event sound, at this level (0..1, as heard).
    Play(Arc<crate::sounds::Pcm>, f32),
    /// Records this many seconds from the default microphone, answered with the sound.
    Record(u32, Reply<Result<Arc<crate::sounds::Pcm>, String>>),
    /// The recording's time is up.
    RecordEnd,
    Refresh,
    /// A sound's upload into the server's sample cache ended (its name; whether it is there).
    Uploaded(String, bool),
    /// The server no longer has this sample.
    Forget(String),
}

/// An answer the audio thread sends back once (commands stay cloneable).
pub struct Reply<T>(Arc<Mutex<Option<tokio::sync::oneshot::Sender<T>>>>);

impl<T> Reply<T> {
    pub fn new(tx: tokio::sync::oneshot::Sender<T>) -> Self {
        Self(Arc::new(Mutex::new(Some(tx))))
    }

    fn send(&self, v: T) {
        if let Some(tx) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = tx.send(v);
        }
    }
}

impl<T> Clone for Reply<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> std::fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reply")
    }
}

/// The running connection's commands. Replaced by each new connection: the service is
/// restarted when its connection ends (stream::forever), and commands must reach the new one.
static COMMANDS: Mutex<Option<mpsc::Sender<Command>>> = Mutex::new(None);

pub fn command(c: Command) {
    if let Some(tx) = COMMANDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        let _ = tx.send(c);
    }
}

fn volumes(channels: u8, v: f32) -> ChannelVolumes {
    let mut cv = ChannelVolumes::default();
    let raw = (v.clamp(0.0, 1.5) * Volume::NORMAL.0 as f32) as u32;
    cv.set(channels.max(1), Volume(raw));
    cv
}

fn level(cv: &ChannelVolumes) -> f32 {
    cv.avg().0 as f32 / Volume::NORMAL.0 as f32
}

#[derive(Default)]
struct Partial {
    state: State,
    pending: u8,
    mics: Vec<u32>,
    outputs: Vec<(u32, String)>,
    /// Sink index → name, to say where each stream plays.
    sink_names: Vec<(u32, String)>,
    /// (stream index, app name, icon, volume, muted, channels, sink index)
    streams: Vec<(u32, String, String, f32, bool, u8, u32)>,
    /// The same for recording streams (source index last).
    recordings: Vec<(u32, String, String, f32, bool, u8, u32)>,
    /// Source index → name.
    source_names: Vec<(u32, String)>,
    /// Each output's channel levels and map, for setting its balance.
    sink_volumes: std::collections::HashMap<String, (ChannelVolumes, pulse::channelmap::Map)>,
}

fn run(out: tokio::sync::mpsc::UnboundedSender<State>) -> Option<()> {
    let (tx, rx) = mpsc::channel::<Command>();
    *COMMANDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx.clone());
    let mut ml = Mainloop::new()?;
    let mut ctx = Context::new(&ml, "kestrel")?;
    ctx.connect(None, FlagSet::NOFAIL, None).ok()?;
    ml.lock();
    ml.start().ok()?;
    ml.unlock();
    loop {
        ml.lock();
        let st = ctx.get_state();
        ml.unlock();
        match st {
            CtxState::Ready => break,
            CtxState::Failed | CtxState::Terminated => return None,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    ml.lock();
    let refresh_tx = tx.clone();
    ctx.set_subscribe_callback(Some(Box::new(move |fac, _, _| {
        if matches!(
            fac,
            Some(
                Facility::Sink
                    | Facility::Source
                    | Facility::Server
                    | Facility::SourceOutput
                    | Facility::SinkInput
                    | Facility::Card
            )
        ) {
            let _ = refresh_tx.send(Command::Refresh);
        }
    })));
    ctx.subscribe(
        InterestMaskSet::SINK
            | InterestMaskSet::SOURCE
            | InterestMaskSet::SERVER
            | InterestMaskSet::SOURCE_OUTPUT
            | InterestMaskSet::SINK_INPUT
            | InterestMaskSet::CARD,
        |_| {},
    );
    ml.unlock();
    let _ = tx.send(Command::Refresh);

    // The newest refresh round's replies (each round has its own: replies of an older round
    // that arrive after a new one started fill their own and can't miscount the new one's).
    let mut partial = Arc::new(Mutex::new(Partial::default()));
    let mut samples = Samples::default();
    let mut recording: Option<Recording> = None;
    while let Ok(cmd) = rx.recv() {
        // Collapse a burst of refresh requests into one.
        if matches!(cmd, Command::Refresh) {
            while let Ok(Command::Refresh) = rx.try_recv() {}
        }
        ml.lock();
        let mut intro = ctx.introspect();
        match cmd {
            Command::SinkVolume(n, ch, v) => {
                intro.set_sink_volume_by_name(&n, &volumes(ch, v), None);
            }
            Command::SinkMute(n, m) => {
                intro.set_sink_mute_by_name(&n, m, None);
            }
            Command::SourceVolume(n, ch, v) => {
                intro.set_source_volume_by_name(&n, &volumes(ch, v), None);
            }
            Command::SourceMute(n, m) => {
                intro.set_source_mute_by_name(&n, m, None);
            }
            Command::DefaultSink(n) => {
                ctx.set_default_sink(&n, |_| {});
            }
            Command::DefaultSource(n) => {
                ctx.set_default_source(&n, |_| {});
            }
            Command::AppVolume(ids, ch, v) => {
                for i in ids {
                    intro.set_sink_input_volume(i, &volumes(ch, v), None);
                }
            }
            Command::AppMute(ids, m) => {
                for i in ids {
                    intro.set_sink_input_mute(i, m, None);
                }
            }
            Command::AppMove(ids, sink) => {
                for i in ids {
                    intro.move_sink_input_by_name(i, &sink, None);
                }
            }
            Command::RecorderVolume(ids, ch, v) => {
                for i in ids {
                    intro.set_source_output_volume(i, &volumes(ch, v), None);
                }
            }
            Command::RecorderMute(ids, m) => {
                for i in ids {
                    intro.set_source_output_mute(i, m, None);
                }
            }
            Command::RecorderMove(ids, source) => {
                for i in ids {
                    intro.move_source_output_by_name(i, &source, None);
                }
            }
            Command::SinkBalance(name, b) => {
                let known = partial
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .sink_volumes
                    .get(&name)
                    .copied();
                if let Some((mut cv, map)) = known
                    && cv.set_balance(&map, b.clamp(-1.0, 1.0)).is_some()
                {
                    intro.set_sink_volume_by_name(&name, &cv, None);
                }
            }
            Command::CardProfile(card, profile) => {
                intro.set_card_profile_by_index(card, &profile, None);
            }
            Command::Record(secs, reply) => {
                if recording.is_some() {
                    reply.send(Err("The microphone is already being tested".into()));
                } else {
                    recording = record(&mut ctx, secs, reply, &tx);
                }
            }
            Command::RecordEnd => {
                if let Some(r) = recording.take() {
                    let mut s = r.stream.borrow_mut();
                    s.set_read_callback(None);
                    s.set_state_callback(None);
                    let _ = s.disconnect();
                    drop(s);
                    let mut heard = r.heard.borrow_mut();
                    heard.truncate(r.wanted);
                    let pcm = crate::sounds::Pcm {
                        file: "microphone-test".into(),
                        rate: RECORD_RATE,
                        channels: 1,
                        samples: heard
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| i16::from_le_bytes(*b))
                            .collect(),
                        keep: false,
                    };
                    r.reply.send(if pcm.samples.is_empty() {
                        Err("The microphone could not be recorded".into())
                    } else {
                        Ok(Arc::new(pcm))
                    });
                }
            }
            Command::Play(pcm, volume) => {
                let name = sample_name(&pcm);
                if pcm.keep && samples.cached.contains(&name) {
                    play_sample(&mut ctx, &name, volume, &tx);
                } else if let Some(u) = samples.uploading.get_mut(&name) {
                    u.waiting.push(volume);
                } else if let Some(stream) = upload(&mut ctx, &pcm, &tx) {
                    let u = Upload {
                        stream,
                        waiting: vec![volume],
                        keep: pcm.keep,
                    };
                    samples.uploading.insert(name, u);
                }
            }
            Command::Uploaded(name, ok) => {
                if let Some(u) = samples.uploading.remove(&name) {
                    // Its callbacks hold the stream: cleared, so it is freed.
                    let mut s = u.stream.borrow_mut();
                    s.set_write_callback(None);
                    s.set_state_callback(None);
                    drop(s);
                    if ok {
                        for v in u.waiting {
                            play_sample(&mut ctx, &name, v, &tx);
                        }
                        // A recording is uploaded afresh each time (same name, replaced).
                        if u.keep {
                            samples.cached.insert(name);
                        }
                    } else {
                        log::warn!("sound {name}: the server did not take it");
                    }
                }
            }
            Command::Forget(name) => {
                samples.cached.remove(&name);
            }
            Command::Refresh => {
                partial = Arc::new(Mutex::new(Partial {
                    pending: 6,
                    ..Partial::default()
                }));
                let finish = {
                    let partial = partial.clone();
                    let out = out.clone();
                    move || {
                        let mut p = partial
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        p.pending -= 1;
                        if p.pending == 0 {
                            let mut rec: Vec<String> = p
                                .outputs
                                .iter()
                                .filter(|(src, _)| p.mics.contains(src))
                                .map(|(_, app)| app.clone())
                                .collect();
                            rec.sort();
                            rec.dedup();
                            p.state.recording = rec;
                            p.state.apps = group_apps(&p.streams, &p.sink_names);
                            p.state.recorders = group_apps(&p.recordings, &p.source_names);
                            let _ = out.send(p.state.clone());
                        }
                    }
                };
                let (p1, f1) = (partial.clone(), finish.clone());
                intro.get_server_info(move |info| {
                    let mut p = p1.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    p.state.default_sink = info.default_sink_name.as_deref().unwrap_or("").into();
                    p.state.default_source =
                        info.default_source_name.as_deref().unwrap_or("").into();
                    drop(p);
                    f1();
                });
                let (p2, f2) = (partial.clone(), finish.clone());
                intro.get_sink_info_list(move |r| match r {
                    ListResult::Item(i) => {
                        let name = i.name.as_deref().unwrap_or("").to_string();
                        let port = i
                            .active_port
                            .as_ref()
                            .and_then(|p| p.name.as_deref())
                            .unwrap_or("")
                            .to_lowercase();
                        let plugged = i
                            .active_port
                            .as_ref()
                            .is_none_or(|p| p.available != pulse::def::PortAvailable::No);
                        let mut g = p2.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        g.sink_names.push((i.index, name.clone()));
                        g.sink_volumes
                            .insert(name.clone(), (i.volume, i.channel_map));
                        let balance = if i.volume.len() == 2 {
                            i.volume.get_balance(&i.channel_map)
                        } else {
                            0.0
                        };
                        drop(g);
                        p2.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .state
                            .sinks
                            .push(Device {
                                plugged,
                                card: i.card,
                                balance,
                                bluetooth: name.starts_with("bluez_"),
                                headphones: port.contains("headphone")
                                    || name.starts_with("bluez_"),
                                description: i.description.as_deref().unwrap_or(&name).to_string(),
                                name,
                                volume: level(&i.volume),
                                muted: i.mute,
                                channels: i.volume.len(),
                            });
                    }
                    ListResult::End | ListResult::Error => f2(),
                });
                let (p4, f4) = (partial.clone(), finish.clone());
                intro.get_source_output_info_list(move |r| match r {
                    ListResult::Item(i) => {
                        let prop = |k: &str| i.proplist.get_str(k).unwrap_or_default();
                        let raw = Some(prop("application.name"))
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| "An app".into());
                        // kestrel's own level meters and visualiser are not "recording".
                        if raw.starts_with("kestrel") {
                            return;
                        }
                        // The app's own name: no stream suffix ("Chromium input"), and its
                        // desktop entry's name for a program name ("vesktop").
                        let bare = raw
                            .trim_end_matches(" input")
                            .trim_end_matches(" capture")
                            .to_string();
                        let app = crate::apps::name_for_class(&bare);
                        let icon = Some(prop("application.icon_name"))
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| prop("application.process.binary"));
                        let mut g = p4.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        g.outputs.push((i.source, app.clone()));
                        g.recordings.push((
                            i.index,
                            app,
                            icon,
                            level(&i.volume),
                            i.mute,
                            i.volume.len(),
                            i.source,
                        ));
                    }
                    ListResult::End | ListResult::Error => f4(),
                });
                let (p5, f5) = (partial.clone(), finish.clone());
                intro.get_sink_input_info_list(move |r| match r {
                    ListResult::Item(i) => {
                        let prop = |k: &str| i.proplist.get_str(k).unwrap_or_default();
                        // System sounds (a notification's ding) and level meters are not apps.
                        if prop("media.role") == "event"
                            || prop("application.id") == "org.PulseAudio.pavucontrol"
                        {
                            return;
                        }
                        let name = Some(prop("application.name"))
                            .filter(|n| !n.is_empty())
                            .or_else(|| {
                                Some(prop("application.process.binary")).filter(|n| !n.is_empty())
                            })
                            .unwrap_or_else(|| "An app".into());
                        let icon = Some(prop("application.icon_name"))
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| prop("application.process.binary"));
                        p5.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .streams
                            .push((
                                i.index,
                                name,
                                icon,
                                level(&i.volume),
                                i.mute,
                                i.volume.len(),
                                i.sink,
                            ));
                    }
                    ListResult::End | ListResult::Error => f5(),
                });
                let (p6, f6) = (partial.clone(), finish.clone());
                intro.get_card_info_list(move |r| match r {
                    ListResult::Item(i) => {
                        let name = |p: &pulse::context::introspect::CardProfileInfo| {
                            p.name.as_deref().unwrap_or("").to_string()
                        };
                        p6.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .state
                            .cards
                            .push(Card {
                                index: i.index,
                                name: i.name.as_deref().unwrap_or("").to_string(),
                                active: i.active_profile.as_deref().map(name).unwrap_or_default(),
                                profiles: i
                                    .profiles
                                    .iter()
                                    .filter(|p| p.available && name(p) != "off")
                                    .map(|p| {
                                        (
                                            name(p),
                                            p.description.as_deref().unwrap_or("").to_string(),
                                        )
                                    })
                                    .collect(),
                            });
                    }
                    ListResult::End | ListResult::Error => f6(),
                });
                let (p3, f3) = (partial.clone(), finish);
                intro.get_source_info_list(move |r| match r {
                    ListResult::Item(i) => {
                        // Monitors of outputs are not microphones.
                        if i.monitor_of_sink.is_some() {
                            return;
                        }
                        let name = i.name.as_deref().unwrap_or("").to_string();
                        let plugged = i
                            .active_port
                            .as_ref()
                            .is_none_or(|p| p.available != pulse::def::PortAvailable::No);
                        p3.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .mics
                            .push(i.index);
                        p3.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .source_names
                            .push((i.index, name.clone()));
                        p3.lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .state
                            .sources
                            .push(Device {
                                plugged,
                                card: i.card,
                                balance: 0.0,
                                bluetooth: name.starts_with("bluez_"),
                                headphones: false,
                                description: i.description.as_deref().unwrap_or(&name).to_string(),
                                name,
                                volume: level(&i.volume),
                                muted: i.mute,
                                channels: i.volume.len(),
                            });
                    }
                    ListResult::End | ListResult::Error => f3(),
                });
            }
        }
        ml.unlock();
    }
    Some(())
}

/// Streams gathered into one row per app, in a steady order.
fn group_apps(
    streams: &[(u32, String, String, f32, bool, u8, u32)],
    sinks: &[(u32, String)],
) -> Vec<App> {
    let mut apps: Vec<App> = Vec::new();
    for (index, name, icon, volume, muted, channels, sink) in streams {
        let sink_name = sinks
            .iter()
            .find(|(i, _)| i == sink)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        match apps.iter_mut().find(|a| a.name == *name) {
            Some(a) => {
                a.streams.push(*index);
                a.volume = a.volume.max(*volume);
                a.muted &= *muted;
                if a.sink != sink_name {
                    a.sink.clear();
                }
            }
            None => apps.push(App {
                name: name.clone(),
                icon: icon.clone(),
                streams: vec![*index],
                volume: *volume,
                muted: *muted,
                channels: *channels,
                sink: sink_name,
            }),
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(8, async |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("kestrel-audio".into())
            .spawn(move || {
                if run(tx).is_none() {
                    log::warn!("audio: could not connect to the sound server");
                }
            })
            .ok();
        while let Some(s) = rx.recv().await {
            if out.send(s).await.is_err() {
                return;
            }
        }
    })
}

/// Every output plays in mono (WirePlumber's own setting, kept by it).
pub async fn mono() -> Option<bool> {
    crate::pipewire::setting(MONO)
        .await
        .map(|v| v.trim() == "true")
}

pub async fn set_mono(on: bool) {
    crate::pipewire::set_setting(MONO, if on { "true" } else { "false" });
}

const MONO: &str = "node.features.audio.mono";

#[cfg(test)]
mod tests {
    #[test]
    fn streams_group_into_one_row_per_app() {
        let sinks = vec![(1, "speaker".to_string()), (2, "pods".to_string())];
        let s = |i: u32, n: &str, v: f32, m: bool, sink: u32| {
            (i, n.to_string(), n.to_lowercase(), v, m, 2u8, sink)
        };
        let apps = super::group_apps(
            &[
                s(10, "Chromium", 0.5, false, 1),
                s(11, "Chromium", 0.8, true, 2),
                s(12, "mpv", 1.0, true, 1),
            ],
            &sinks,
        );
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "Chromium");
        assert_eq!(apps[0].streams, vec![10, 11]);
        assert!((apps[0].volume - 0.8).abs() < 1e-6);
        assert!(!apps[0].muted, "muted only when every stream is");
        assert!(apps[0].sink.is_empty(), "two outputs: no single one");
        assert_eq!(apps[1].sink, "speaker");
        assert!(apps[1].muted);
    }
}
