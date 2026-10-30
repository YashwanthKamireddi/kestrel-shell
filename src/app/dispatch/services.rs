//! Readings from system services (sound, network, Bluetooth, power, media, Hyprland…).

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Hypr(s) => {
            // A recording starting or ending says so at once in the island (where it is
            // saved, when it ends).
            // Started: the recorder is running and Hyprland says the screen is being captured.
            // Ended: the recorder process is gone (Hyprland's screencast flag flickers during a
            // recording, which announced "saved" again on every flicker), once per file.
            let started = match &s.recording {
                Some(f) if s.screencast && app.started_recording.as_deref() != Some(f.as_str()) => {
                    app.started_recording = Some(f.clone());
                    true
                }
                _ => false,
            };
            let ended = match (&app.hypr.recording, &s.recording) {
                (Some(f), None) if app.saved_recording.as_deref() != Some(f.as_str()) => {
                    Some(f.clone())
                }
                _ => None,
            };
            if started {
                app.flash = Some(Flash {
                    icon: Icon::ScreenShare,
                    title: "Recording".into(),
                    detail: String::new(),
                    thumb: None,
                    until: Instant::now() + Duration::from_millis(FLASH_MS),
                    // A click stops it.
                    action: s.recording_pid.map(|_| Message::StopRecording),
                });
                app.clock.reset();
            } else if let Some(file) = &ended {
                app.saved_recording = Some(file.clone());
                let name = std::path::Path::new(file)
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                app.flash = Some(Flash {
                    icon: Icon::Check,
                    title: "Recording saved".into(),
                    detail: name.clone(),
                    thumb: None,
                    until: Instant::now() + Duration::from_millis(FLASH_MS + 1800),
                    // A click plays it.
                    action: Some(open_file(file)),
                });
                let folder = std::path::Path::new(file)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                app.remember(
                    "media-record",
                    "Recording saved".into(),
                    name,
                    None,
                    vec![
                        (format!("kestrel-open:{file}"), "Open".into()),
                        (format!("kestrel-copy:{file}"), "Copy".into()),
                        (format!("kestrel-open:{folder}"), "Show in folder".into()),
                    ],
                );
                app.clock.reset();
            }
            // Scripts hear about a new workspace or a new focused app.
            use crate::lua::{Val, event};
            let ws = |h: &hyprland::State| h.active_on(&h.focused_monitor);
            if ws(&s) != ws(&app.hypr)
                && let Some(id) = ws(&s)
            {
                event("workspace", vec![("id", Val::Num(f64::from(id)))]);
            }
            if s.window_class != app.hypr.window_class || s.window_title != app.hypr.window_title {
                event(
                    "window",
                    vec![
                        ("class", Val::Str(s.window_class.clone())),
                        ("title", Val::Str(s.window_title.clone())),
                    ],
                );
            }
            // A window opened or closed: take the overview's pictures now, in the background,
            // so it never opens on icons.
            let changed = {
                let mut a: Vec<&str> = app
                    .hypr
                    .windows
                    .iter()
                    .map(|w| w.address.as_str())
                    .collect();
                let mut b: Vec<&str> = s.windows.iter().map(|w| w.address.as_str()).collect();
                a.sort_unstable();
                b.sort_unstable();
                a != b
            };
            // The keyboard layout: an update that does not say keeps the one known; a new
            // one (Super+Space) is shown, as macOS and GNOME show the input source.
            let mut s = s;
            if s.layout.is_empty() {
                s.layout.clone_from(&app.hypr.layout);
            } else if !app.hypr.layout.is_empty() && s.layout != app.hypr.layout {
                log::info!("keyboard layout: {} -> {}", app.hypr.layout, s.layout);
                app.show_event(Icon::Keyboard, s.layout.clone(), "Keyboard layout".into());
            }
            // The overview is for choosing a window; switching workspace from the keyboard while
            // it is open (Super+5) means you went somewhere else: it closes (its cards were laid
            // out for where you were, and broke).
            let moved =
                app.hypr.active_on(&app.hypr.focused_monitor) != s.active_on(&s.focused_monitor);
            let overview_open = app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Overview);
            app.hypr = s;
            app.update_pills(false);
            if moved && overview_open {
                let a = panels::overview::refresh_missing(&app.panels.overview, &app.hypr);
                let refresh = panels::action::perform(app, a, |m| {
                    Message::Panel(panels::Message::Overview(m))
                });
                return Task::batch([app.close(), refresh]);
            }
            if changed {
                let a = panels::overview::refresh_missing(&app.panels.overview, &app.hypr);
                return panels::action::perform(app, a, |m| {
                    Message::Panel(panels::Message::Overview(m))
                });
            }
            Task::none()
        }
        Message::Audio(s) => {
            app.audio_changed(s);
            Task::none()
        }
        Message::Net(s) => {
            let before = std::mem::replace(&mut app.net, *s);
            app.panels.lists_changed(&app.net, &app.bt);
            // Settings' Network page reads its connection again when it moved (only while
            // its window is open).
            if app.settings_window.is_none() {
                return Task::none();
            }
            let a = panels::settings::network_changed(&app.panels.settings, &app.net, &before);
            panels::action::perform(app, a, |m| Message::Panel(panels::Message::Settings(m)))
        }
        Message::Bt(s) => {
            // A device connecting or leaving gets an OSD (not on the first snapshot, nor when
            // the adapter itself goes off and takes every device with it).
            if !app.bt.adapter.is_empty() && app.bt.powered && s.powered {
                for d in &s.devices {
                    let was = app
                        .bt
                        .devices
                        .iter()
                        .find(|o| o.path == d.path)
                        .is_some_and(|o| o.connected);
                    if was != d.connected {
                        crate::lua::event(
                            "device",
                            vec![
                                ("name", crate::lua::Val::Str(d.name.clone())),
                                ("connected", crate::lua::Val::Bool(d.connected)),
                            ],
                        );
                        let mut sub = if d.connected {
                            "Connected"
                        } else {
                            "Disconnected"
                        }
                        .to_string();
                        if let (true, Some(b)) = (d.connected, d.battery) {
                            sub.push_str(&format!(" · {b}%"));
                        }
                        // AirPods report their charge a moment later, over their own channel.
                        if services::airpods::is_airpods(&d.modalias, &d.icon) {
                            app.pods_fresh = d.connected;
                        }
                        app.show_event(device_icon(&d.icon), d.name.clone(), sub);
                    }
                }
            }
            app.bt = s;
            app.panels.lists_changed(&app.net, &app.bt);
            if crate::panels::pods::device(&app.bt).is_none() {
                app.pods = None;
                // Their panel goes with them.
                if app
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.open && p.kind == PanelKind::Pods)
                {
                    return app.close();
                }
            }
            Task::none()
        }
        Message::Pods(p) => {
            if let Some(p) = &p
                && app.pods_fresh
                && (p.left.is_some() || p.right.is_some() || p.headset.is_some())
            {
                app.pods_fresh = false;
                let name = crate::panels::pods::device(&app.bt)
                    .map_or_else(|| "AirPods".to_string(), |d| d.name.clone());
                let pct = |c: Option<services::airpods::Charge>| {
                    c.map_or_else(|| "–".to_string(), |c| format!("{}%", c.level))
                };
                let sub = match p.headset {
                    Some(h) => format!("Connected · {}%", h.level),
                    None => format!("L {}  ·  R {}", pct(p.left), pct(p.right)),
                };
                app.show_event(Icon::Earbuds, name, sub);
            }
            app.pods = p;
            Task::none()
        }
        Message::Power(s) => {
            // Power mode follows the charger: balanced on battery, performance on AC.
            let plugged_changed = app.power.has_battery && app.power.on_battery != s.on_battery;
            let on_battery = s.on_battery;
            if app.power.percent.round() != s.percent.round() {
                crate::lua::event(
                    "battery",
                    vec![(
                        "percent",
                        crate::lua::Val::Num(f64::from(s.percent.round())),
                    )],
                );
            }
            if plugged_changed {
                crate::lua::event(
                    "power",
                    vec![
                        ("charging", crate::lua::Val::Bool(!s.on_battery)),
                        (
                            "percent",
                            crate::lua::Val::Num(f64::from(s.percent.round())),
                        ),
                    ],
                );
                let pct = format!("{:.0}%", s.percent);
                if on_battery {
                    app.show_event(Icon::Battery, "On battery".into(), pct);
                } else {
                    app.show_event(Icon::PlugZap, "Charging".into(), pct);
                }
            }
            app.battery_warning(&s);
            app.power = s;
            if plugged_changed && app.power.profile.is_some() {
                let want = if on_battery {
                    power::Profile::Balanced
                } else {
                    power::Profile::Performance
                };
                app.power.profile = Some(want);
                return Task::future(power::set_profile(want)).discard();
            }
            Task::none()
        }
        Message::Media(s) => {
            let key = |m: &mpris::State| m.main().map(|p| (p.title.clone(), p.playing));
            if key(&s) != key(&app.media)
                && let Some(p) = s.main()
            {
                use crate::lua::Val;
                crate::lua::event(
                    "media",
                    vec![
                        ("title", Val::Str(p.title.clone())),
                        ("artist", Val::Str(p.artist.clone())),
                        ("playing", Val::Bool(p.playing)),
                    ],
                );
            }
            for p in &s.players {
                if let Some(old) = app.media.players.iter().find(|o| o.bus == p.bus)
                    && old.title == p.title
                    && p.length > old.length + 1_000_000
                    && old.length > 0
                {
                    app.length_unreliable.insert(p.bus.clone());
                }
            }
            // A new track starts with a clean slate.
            app.length_unreliable.retain(|b| {
                s.players.iter().any(|p| {
                    &p.bus == b
                        && app
                            .media
                            .players
                            .iter()
                            .any(|o| &o.bus == b && o.title == p.title)
                })
            });
            // Artwork settles before it is shown: a browser points at a placeholder (its own
            // logo) for a moment while the next video loads, which flashed in the bubble. A
            // player keeps its last settled cover until the new one has stayed for a moment.
            let mut s = s;
            let mut wait = Vec::new();
            for p in &mut s.players {
                let real = p.art.clone();
                app.reported_art.insert(p.bus.clone(), real.clone());
                match app.settled_art.get(&p.bus) {
                    // First sight of this player: nothing to protect.
                    None => {
                        app.settled_art.insert(p.bus.clone(), real);
                    }
                    Some(shown) if *shown != real => {
                        wait.push((p.bus.clone(), real));
                        p.art = shown.clone();
                    }
                    Some(_) => {}
                }
            }
            app.settled_art
                .retain(|b, _| s.players.iter().any(|p| &p.bus == b));
            app.media = s;
            let mut tasks: Vec<Task<Message>> = wait
                .into_iter()
                .map(|(bus, art)| {
                    Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                            (bus, art)
                        },
                        |(bus, art)| Message::ArtSettled(bus, art),
                    )
                })
                .collect();
            // A pause, a play or a new track while the media panel shows: its position is read
            // again at once (it kept the one read on opening, 0:00 on a video at 1:58).
            if app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Media)
            {
                tasks.push(Task::done(Message::Panel(panels::Message::Media(
                    panels::media::Message::Poll,
                ))));
            }
            Task::batch(tasks)
        }
        Message::Camera(apps) => {
            app.camera_apps = apps;
            Task::none()
        }
        Message::Ddc(found) => {
            if !found.is_empty() {
                log::info!(
                    "brightness: DDC/CI on {:?}",
                    found.iter().map(|d| &d.connector).collect::<Vec<_>>()
                );
            }
            let reads = found.iter().map(|d| {
                let bus = d.bus;
                Task::perform(services::ddc::get(bus), move |v| Message::DdcLevel(bus, v))
            });
            let t = Task::batch(reads.collect::<Vec<_>>());
            app.ddc = found;
            t
        }
        Message::DdcLevel(bus, v) => {
            match v {
                Some(v) => {
                    app.ddc_level.insert(bus, v);
                }
                // Stopped answering: dimming takes over.
                None => app.ddc.retain(|d| d.bus != bus),
            }
            Task::none()
        }
        Message::ArtSettled(bus, art) => {
            // Adopted only if the player still points at it (it stayed put).
            if app.pending_art(&bus) == Some(art.clone()) {
                app.settled_art.insert(bus.clone(), art.clone());
                if let Some(p) = app.media.players.iter_mut().find(|p| p.bus == bus) {
                    p.art = art;
                }
            }
            Task::none()
        }
        Message::Tray(s) => {
            app.tray = s;
            Task::none()
        }
        Message::Notification(e) => app.notify(e),
        Message::VolumeStep(lines) => {
            if let Some(d) = app.audio.sink().cloned() {
                // 5 % a notch, up to Settings > Sound's maximum like the volume keys (capped at
                // 100 %, scrolling up from a boosted 130 % dropped it to 100 %).
                let v = stepped(d.volume, lines.signum() * 5.0, app.volume_max());
                audio::command(audio::Command::SinkVolume(d.name, d.channels, v));
            }
            Task::none()
        }
        Message::Weather(w) => {
            if app.weather.as_ref() != Some(&w) {
                services::web::keep_weather(&w);
            }
            app.weather = Some(w);
            Task::none()
        }
        // The level stream yields nothing now (its readings are read each frame).
        Message::Level(_) => Task::none(),
        Message::VisTick(now) => {
            app.vis_step(now);
            Task::none()
        }
        Message::Meters(m) => {
            app.meters = m;
            Task::none()
        }
        Message::Copied(c) => {
            app.copied(c);
            // The history recorded the copy before "Copied" arrived: read again in the
            // background, so Super+V opens on a list that is already there, never "Loading…".
            Task::perform(panels::clipboard::clip_list(), |c| {
                Message::Panel(panels::Message::Clipboard(
                    panels::clipboard::Message::Loaded(c),
                ))
            })
        }
        Message::Updates(s) => {
            app.updates = s;
            Task::none()
        }
        Message::System(s) => {
            app.system = s;
            Task::none()
        }
        Message::Locks(caps, num, is_caps) => {
            let (name, on) = if is_caps {
                ("Caps Lock", caps)
            } else {
                ("Num Lock", num)
            };
            app.show_osd(Icon::Keyboard, 0.0);
            if let Some(o) = &mut app.osd {
                o.text = Some(format!("{name} {}", if on { "on" } else { "off" }));
            }
            Task::none()
        }
        Message::Vpn(s) => {
            app.vpn = s;
            Task::none()
        }
        _ => unreachable!("routed here by app::dispatch"),
    }
}
