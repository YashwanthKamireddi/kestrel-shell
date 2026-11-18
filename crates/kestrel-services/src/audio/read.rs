//! One refresh: everything the sound server knows, read in six lists at once. Each list's
//! reply fills its part of the round; the last one to finish sends the whole state.

use super::*;
use pulse::context::introspect::Introspector;

#[derive(Default)]
pub(super) struct Partial {
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
    pub(super) sink_volumes:
        std::collections::HashMap<String, (ChannelVolumes, pulse::channelmap::Map)>,
}

/// Starts a refresh round; its replies fill the returned round (a newer round has its own, so
/// late replies of an older one cannot miscount it).
pub(super) fn refresh(
    intro: &Introspector,
    out: &tokio::sync::mpsc::UnboundedSender<State>,
) -> Arc<Mutex<Partial>> {
    let partial = Arc::new(Mutex::new(Partial {
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
            p.pending = p.pending.saturating_sub(1);
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
    server(intro, partial.clone(), finish.clone());
    outputs(intro, partial.clone(), finish.clone());
    recorders(intro, partial.clone(), finish.clone());
    apps(intro, partial.clone(), finish.clone());
    cards(intro, partial.clone(), finish.clone());
    inputs(intro, partial.clone(), finish);
    partial
}

/// The defaults.
fn server(intro: &Introspector, p1: Arc<Mutex<Partial>>, f1: impl Fn() + 'static) {
    intro.get_server_info(move |info| {
        let mut p = p1.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.state.default_sink = info.default_sink_name.as_deref().unwrap_or("").into();
        p.state.default_source = info.default_source_name.as_deref().unwrap_or("").into();
        drop(p);
        f1();
    });
}

/// Outputs (sinks), with their balance and where they play.
fn outputs(intro: &Introspector, p2: Arc<Mutex<Partial>>, f2: impl Fn() + 'static) {
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
                    headphones: port.contains("headphone") || name.starts_with("bluez_"),
                    description: i.description.as_deref().unwrap_or(&name).to_string(),
                    name,
                    volume: level(&i.volume),
                    muted: i.mute,
                    channels: i.volume.len(),
                });
        }
        ListResult::End | ListResult::Error => f2(),
    });
}

/// Apps recording (source outputs); kestrel's own meters are not.
fn recorders(intro: &Introspector, p4: Arc<Mutex<Partial>>, f4: impl Fn() + 'static) {
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
}

/// Apps playing (sink inputs); system sounds and meters are not.
fn apps(intro: &Introspector, p5: Arc<Mutex<Partial>>, f5: impl Fn() + 'static) {
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
                .or_else(|| Some(prop("application.process.binary")).filter(|n| !n.is_empty()))
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
}

/// Sound cards and their profiles.
fn cards(intro: &Introspector, p6: Arc<Mutex<Partial>>, f6: impl Fn() + 'static) {
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
                        .map(|p| (name(p), p.description.as_deref().unwrap_or("").to_string()))
                        .collect(),
                });
        }
        ListResult::End | ListResult::Error => f6(),
    });
}

/// Microphones (sources that are not an output's monitor).
fn inputs(intro: &Introspector, p3: Arc<Mutex<Partial>>, f3: impl Fn() + 'static) {
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
