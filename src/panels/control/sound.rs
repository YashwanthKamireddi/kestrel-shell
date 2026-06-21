//! The control centre's sound page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A thin live level under a slider (Windows' mixer shows the sound moving the same way).
pub(super) fn meter<'a>(s: Scheme, value: f32) -> Element<'a, AppMessage> {
    let v = value.clamp(0.0, 1.0);
    let fill = container(Space::new())
        .width(Length::FillPortion(((v * 1000.0) as u16).max(1)))
        .height(3)
        .style(move |_| container::Style {
            background: Some(s.accent.scale_alpha(0.85 * s.opacity).into()),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        });
    let rest = Space::new()
        .width(Length::FillPortion((((1.0 - v) * 1000.0) as u16).max(1)))
        .height(3);
    container(row![fill, rest])
        .padding([tok::NONE, tok::L])
        .width(Length::Fill)
        .height(3)
        .into()
}

/// A mode's name as people say it (Bluetooth's long profile names made short).
pub fn mode_name(description: &str) -> String {
    // The codec, when the server names one ("..., codec AAC)"): a headset offers each mode
    // with several, which read as the same line three times without it.
    let codec = description
        .split_once("codec ")
        .map(|(_, c)| {
            let c = c.trim_end_matches(')').trim();
            // A sample rate after the name ("LC3-24kHz") is dropped; "SBC-XQ" stays whole.
            match c.rsplit_once('-') {
                Some((name, rate)) if rate.ends_with("Hz") => name.to_string(),
                _ => c.to_string(),
            }
        })
        .filter(|c| !c.is_empty());
    let base = if description.contains("A2DP") {
        "High quality"
    } else if description.contains("HSP") || description.contains("HFP") {
        "Calls"
    } else {
        return description.to_string();
    };
    match codec {
        Some(c) => format!("{base} · {c}"),
        None if base == "Calls" => "Calls (with microphone)".into(),
        None => base.into(),
    }
}

/// Sound, like Windows' volume mixer: the output (level, live meter, device, mode, balance),
/// the microphone (level, live meter, device), every app playing sound (its own level, live
/// meter, mute, output) and every app recording (its level, mute, microphone).
pub(super) fn output_page(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    // The control centre scrolls as a whole: no scroll of its own.
    sound(app, s, true).into()
}

/// The whole mixer (output, microphone, apps, recording): in the control centre's Sound page
/// (`header`: with its back row) and in Settings > Sound (one implementation).
pub fn sound(app: &App, s: Scheme, header: bool) -> iced::widget::Column<'_, AppMessage> {
    let head = subpage_header(
        s,
        "Sound",
        msg(Message::Page(Page::Main)),
        Space::new().into(),
    );
    let heading = |t: &'static str| {
        container(label_w(t, text::SMALL, s.ink_dim, theme::SEMIBOLD)).padding([tok::XS, tok::S])
    };
    let check = |on: bool| -> Element<'_, AppMessage> {
        if on {
            icon(Icon::Check, size::ICON, s.accent).into()
        } else {
            Space::new().into()
        }
    };
    let mut col = column![].spacing(tok::L);
    if header {
        col = col.push(head);
    }
    let st = &app.panels.control;

    // Output: its level and the sound moving, the devices, its card's modes, the balance.
    let mut out = column![heading("Output")].spacing(tok::S);
    let sink = app.audio.sink();
    if let Some(d) = sink {
        let top = boost_top(app);
        out = out.push(level_row_to(
            s,
            level_icon(Some(d)),
            if d.muted { 0.0 } else { d.volume.min(top) },
            top,
            Message::SinkVolume,
            Message::SinkMute,
            Trail::Bare,
        ));
        out = out.push(meter(s, if d.muted { 0.0 } else { app.meters.output }));
    }
    let sinks: Vec<&audio::Device> = app
        .audio
        .sinks
        .iter()
        .filter(|d| d.plugged || d.name == app.audio.default_sink)
        .collect();
    let names = short_names(sinks.iter().map(|d| device_name(&d.description)));
    let mut list = column![].spacing(tok::XXS);
    for (d, name) in sinks.into_iter().zip(names) {
        let active = d.name == app.audio.default_sink;
        let i = if d.bluetooth || d.headphones {
            Icon::Headphones
        } else {
            Icon::Speaker
        };
        list = list.push(list_row(
            s,
            icon(i, size::ICON, if active { s.accent } else { s.ink }).into(),
            ui::fit(&name.replace(" (Stereo)", ""), text::BODY, 250.0),
            None,
            check(active),
            active,
            Some(msg(Message::Sink(d.name.clone()))),
        ));
    }
    out = out.push(list);
    // The output's card modes (Bluetooth: high quality or calls), when there is a choice.
    // Only Bluetooth outputs: their modes mean something (music quality or calls with the
    // microphone); a laptop's card lists technical routings with near-identical names.
    if let Some(card) = sink
        .filter(|d| d.bluetooth)
        .and_then(|d| d.card)
        .and_then(|c| app.audio.cards.iter().find(|k| k.index == c))
        .filter(|k| k.profiles.len() > 1)
    {
        let mut modes = column![heading("Mode")].spacing(tok::XXS);
        for (name, description) in &card.profiles {
            let active = *name == card.active;
            modes = modes.push(list_row(
                s,
                icon(
                    Icon::AudioLines,
                    size::ICON,
                    if active { s.accent } else { s.ink },
                )
                .into(),
                ui::fit(&mode_name(description), text::BODY, 250.0),
                None,
                check(active),
                active,
                Some(msg(Message::CardMode(card.index, name.clone()))),
            ));
        }
        out = out.push(modes);
    }
    // Balance, for stereo outputs: a slider like the others, with "Left" and "Right" at its
    // ends and the even middle marked (the marker-on-a-track looked unfinished).
    if let Some(d) = sink.filter(|d| d.channels == 2) {
        let v = ((d.balance + 1.0) / 2.0).clamp(0.0, 1.0);
        let even = (d.balance.abs() < 0.03).then_some(());
        let bar = slider(
            row![
                container(label("Left", text::CAPTION, s.ink_dim)).padding([tok::NONE, tok::L]),
                Space::new().width(Length::Fill),
                label(
                    if even.is_some() {
                        "Even".to_string()
                    } else {
                        format!("{:+.0}", d.balance * 100.0)
                    },
                    text::CAPTION,
                    s.ink_dim
                ),
                Space::new().width(Length::Fill),
                container(label("Right", text::CAPTION, s.ink_dim)).padding([tok::NONE, tok::L]),
            ]
            .align_y(Alignment::Center),
            v,
            28.0,
            (
                s.ink.scale_alpha(tok::TONAL * s.opacity),
                s.accent.scale_alpha(0.35 * s.opacity),
            ),
            |v| msg(Message::Balance(((v * 2.0 - 1.0) * 20.0).round() / 20.0)),
        );
        out = out.push(column![heading("Balance"), bar].spacing(tok::S));
    }
    col = col.push(out);

    // Input: the microphone's level and the sound it hears, then the microphones.
    if let Some(d) = app.audio.source() {
        let mut input = column![heading("Input")].spacing(tok::S).push(level_row(
            s,
            if d.muted { Icon::MicOff } else { Icon::Mic },
            if d.muted { 0.0 } else { d.volume.min(1.0) },
            Message::SourceVolume,
            Message::SourceMute,
            Trail::Bare,
        ));
        input = input.push(meter(s, if d.muted { 0.0 } else { app.meters.input }));
        let sources: Vec<&audio::Device> = app
            .audio
            .sources
            .iter()
            .filter(|d| d.plugged || d.name == app.audio.default_source)
            .collect();
        if sources.len() > 1 {
            let names = short_names(
                sources
                    .iter()
                    .map(|d| mic_name(device_name(&d.description))),
            );
            let mut mics = column![].spacing(tok::XXS);
            for (d, name) in sources.into_iter().zip(names) {
                let active = d.name == app.audio.default_source;
                mics = mics.push(list_row(
                    s,
                    icon(Icon::Mic, size::ICON, if active { s.accent } else { s.ink }).into(),
                    ui::fit(&name, text::BODY, 250.0),
                    None,
                    check(active),
                    active,
                    Some(msg(Message::Source(d.name.clone()))),
                ));
            }
            input = input.push(mics);
        }
        col = col.push(input);
    }

    // Apps playing sound.
    if !app.audio.apps.is_empty() {
        let mut apps = column![heading("Apps")].spacing(tok::L);
        for a in &app.audio.apps {
            let level = app
                .meters
                .apps
                .iter()
                .filter(|(i, _)| a.streams.contains(i))
                .map(|(_, v)| *v)
                .fold(0.0, f32::max);
            apps = apps.push(app_row(app, s, a, level, st.sound_open.as_deref(), false));
        }
        col = col.push(apps);
    }
    // Apps recording (a microphone).
    if !app.audio.recorders.is_empty() {
        let mut rec = column![heading("Recording")].spacing(tok::L);
        for a in &app.audio.recorders {
            rec = rec.push(app_row(app, s, a, 0.0, st.sound_open.as_deref(), true));
        }
        col = col.push(rec);
    }
    col
}

/// One app in the mixer: its icon (a click mutes it), its name and the device it uses (a click
/// opens the list to choose another), its level and, for playback, the sound moving.
/// `recording`: an app using a microphone (its device is a microphone).
pub(super) fn app_row<'a>(
    app: &'a App,
    s: Scheme,
    a: &'a audio::App,
    level: f32,
    open: Option<&str>,
    recording: bool,
) -> Element<'a, AppMessage> {
    let ids = a.streams.clone();
    let lead: Element<'a, AppMessage> = match a.icon_path() {
        Some(p) => ui::image_file_faded(&p, 24.0, if a.muted { 0.4 } else { 1.0 } * s.opacity),
        None => icon(
            if a.muted {
                Icon::VolumeX
            } else if recording {
                Icon::Mic
            } else {
                Icon::Music
            },
            18.0,
            if a.muted { s.ink_faint } else { s.ink },
        )
        .into(),
    };
    let mute = ui::control(container(lead).center(32))
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(msg(if recording {
            Message::RecMute(ids.clone())
        } else {
            Message::AppMute(ids.clone())
        }));
    // The devices it could use, and the one it does.
    let (devices, default) = if recording {
        (&app.audio.sources, app.audio.default_source.as_str())
    } else {
        (&app.audio.sinks, app.audio.default_sink.as_str())
    };
    let usable: Vec<&audio::Device> = devices
        .iter()
        .filter(|d| d.plugged || d.name == default)
        .collect();
    let on = if a.sink.is_empty() {
        default
    } else {
        a.sink.as_str()
    };
    let names: Vec<String> = short_names(usable.iter().map(|d| device_name(&d.description)))
        .into_iter()
        .map(|n| mic_name(&n).replace(" (Stereo)", ""))
        .collect();
    let key = format!("{}{}", if recording { "rec:" } else { "" }, a.name);
    let expanded = open == Some(key.as_str());
    let mut title = row![
        container(label_w(
            ui::fit(&a.name, text::BODY, 170.0),
            text::BODY,
            if a.muted { s.ink_dim } else { s.ink },
            theme::MEDIUM
        ))
        .width(Length::Fill)
    ]
    .align_y(Alignment::Center);
    if usable.len() > 1 {
        let current = usable
            .iter()
            .zip(&names)
            .find(|(d, _)| d.name == on)
            .map_or(String::new(), |(_, n)| n.clone());
        title = title.push(
            ui::control(
                row![
                    label(
                        ui::fit(&current, text::CAPTION, 120.0),
                        text::CAPTION,
                        s.ink_dim
                    ),
                    icon(
                        if expanded {
                            Icon::ChevronUp
                        } else {
                            Icon::ChevronDown
                        },
                        12.0,
                        s.ink_dim
                    ),
                ]
                .spacing(tok::XS)
                .align_y(Alignment::Center),
            )
            .pill(s, crate::ui::control::Pill::Quiet)
            .on_press(msg(Message::AppPick(if expanded {
                None
            } else {
                Some(key)
            }))),
        );
    }
    // An app playing may be boosted past 100 % like the output (the real level shown, the boost
    // past 100 % in red); a recording stays at 100 %.
    let most = if recording { 1.0 } else { boost_top(app) };
    let value = if a.muted { 0.0 } else { a.volume.min(most) };
    let top = if value > 1.0 { most } else { 1.0 };
    let on_track = value / top;
    let ids2 = ids.clone();
    let bar = slider(
        row![
            Space::new().width(Length::Fill),
            label(
                format!("{:.0}%", value * 100.0),
                text::CAPTION,
                if on_track > 0.86 {
                    s.on_accent
                } else {
                    s.ink_dim
                }
            ),
            Space::new().width(tok::L),
        ]
        .align_y(Alignment::Center),
        on_track,
        22.0,
        (s.ink.scale_alpha(tok::TONAL * s.opacity), s.accent),
        move |v| {
            msg(if recording {
                Message::RecVolume(ids2.clone(), v * top)
            } else {
                Message::AppVolume(ids2.clone(), v * top)
            })
        },
    )
    .boost(1.0 / top, s.bad);
    let mut body = column![title, bar].spacing(tok::XS).width(Length::Fill);
    if !recording {
        body = body.push(meter(s, if a.muted { 0.0 } else { level }));
    }
    // Its devices to choose from, right under it, while open.
    if expanded {
        let mut list = column![].spacing(tok::XXS);
        for (d, name) in usable.iter().zip(&names) {
            let active = d.name == on;
            list = list.push(list_row(
                s,
                icon(
                    if recording { Icon::Mic } else { Icon::Speaker },
                    size::ICON,
                    if active { s.accent } else { s.ink },
                )
                .into(),
                ui::fit(name, text::BODY, 220.0),
                None,
                if active {
                    icon(Icon::Check, size::ICON, s.accent).into()
                } else {
                    Space::new().into()
                },
                active,
                Some(msg(if recording {
                    Message::RecTo(ids.clone(), d.name.clone())
                } else {
                    Message::AppTo(ids.clone(), d.name.clone())
                })),
            ));
        }
        body = body.push(list);
    }
    row![mute, body]
        .spacing(tok::M)
        .align_y(Alignment::Start)
        .into()
}

/// Drops the words every name starts with, when there are several names that share them.
/// A device's name without its sound card's ("Alder Lake PCH-P High Definition Audio
/// Controller Speaker" is "Speaker").
pub fn device_name(description: &str) -> &str {
    description
        .split_once("Audio Controller ")
        .map_or(description, |(_, rest)| rest)
}

/// A microphone's name as people say it: the laptop's own is "Built-in microphone".
pub fn mic_name(description: &str) -> &str {
    if description.contains("Digital Microphone") || description.ends_with("Microphone Array") {
        "Built-in microphone"
    } else {
        description
    }
}

pub fn short_names<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let all: Vec<Vec<&str>> = names.map(|n| n.split_whitespace().collect()).collect();
    // Only names that share their first word with another name are shortened.
    all.iter()
        .map(|w| {
            let peers: Vec<&Vec<&str>> = all
                .iter()
                .filter(|o| !std::ptr::eq(*o, w) && o.first() == w.first())
                .collect();
            if peers.is_empty() {
                return w.join(" ");
            }
            let mut common = 0;
            while common < w.len() && peers.iter().all(|o| o.get(common) == w.get(common)) {
                common += 1;
            }
            let rest = w[common..].join(" ");
            if rest.is_empty() { w.join(" ") } else { rest }
        })
        .collect()
}
