//! Settings › sound: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A level: a slider of a fixed width, its percentage, and a mute button.
pub(super) fn level<'a>(
    s: Scheme,
    value: f32,
    muted: bool,
    on_change: impl Fn(f32) -> AppMessage + 'a,
    on_mute: AppMessage,
    mute_icon: (Icon, Icon),
) -> Element<'a, AppMessage> {
    let v = if muted { 0.0 } else { value.clamp(0.0, 1.0) };
    let bar = crate::ui::slider::slider(
        iced::widget::Space::new().width(Length::Fill),
        v,
        22.0,
        (s.ink.scale_alpha(tok::FILL * s.opacity), s.accent),
        on_change,
    );
    row![
        control(icon(
            if muted { mute_icon.1 } else { mute_icon.0 },
            16.0,
            if muted { s.bad } else { s.ink_dim }
        ))
        .padding(tok::S)
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(on_mute),
        container(bar).width(220),
        container(label(
            format!("{:.0}%", value * 100.0),
            text::SMALL,
            s.ink_dim
        ))
        .width(44)
        .align_right(44),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center)
    .into()
}

/// A thin live level, the slider's width.
pub(super) fn level_meter<'a>(s: Scheme, value: f32) -> Element<'a, AppMessage> {
    let v = value.clamp(0.0, 1.0);
    container(row![
        container(iced::widget::Space::new())
            .width(Length::FillPortion(((v * 1000.0) as u16).max(1)))
            .height(4)
            .style(move |_| container::Style {
                background: Some(s.accent.into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            }),
        container(iced::widget::Space::new())
            .width(Length::FillPortion((((1.0 - v) * 1000.0) as u16).max(1)))
            .height(4)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            }),
    ])
    // Exactly under the slider: its width, then the percentage's room (8 + 44) to its right.
    .width(220.0 + 8.0 + 44.0)
    .padding(iced::Padding {
        right: tok::M + 44.0,
        ..iced::Padding::ZERO
    })
    .into()
}

/// A chooser: what is chosen now and an arrow; a click lists the choices under its row.
pub(super) fn chooser<'a>(
    s: Scheme,
    now: String,
    key: &str,
    open: bool,
) -> Element<'a, AppMessage> {
    control(
        row![
            label(now, text::SMALL, s.ink),
            icon(
                if open {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                },
                14.0,
                s.ink_dim
            )
        ]
        .spacing(tok::S)
        .align_y(Alignment::Center),
    )
    .padding([tok::XS, tok::L])
    .radius(tok::FULL)
    .kind(s, crate::ui::control::Kind::Ghost)
    .on_press(msg(Message::SoundPick((!open).then(|| key.to_string()))))
    .into()
}

/// One choice in an open chooser, a check on the one in use.
pub(super) fn choice_row<'a>(
    s: Scheme,
    name: String,
    on: bool,
    m: AppMessage,
) -> Option<Element<'a, AppMessage>> {
    Some(
        control(
            row![
                label(name, text::BODY, if on { s.accent } else { s.ink }).width(Length::Fill),
                if on {
                    Element::from(icon(Icon::Check, tok::ICON, s.accent))
                } else {
                    iced::widget::Space::new().into()
                }
            ]
            .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::INDENT])
        .height(36)
        .width(Length::Fill)
        .kind(s, crate::ui::control::Kind::Ghost)
        .no_press_scale()
        .on_press(AppMessage::Batch(vec![m, msg(Message::SoundPick(None))]))
        .into(),
    )
}

/// Settings > Sound, laid out like the other pages (and like Windows' Sound settings): Output,
/// Input, each app's volume, and apps recording; every row a name and its control.
pub(super) fn sound_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    use crate::panels::control::{Message as C, device_name, mic_name, mode_name, short_names};
    let au = &app.audio;
    let pick = app.panels.settings.sound_pick.as_deref();
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();

    // Output.
    let sinks: Vec<&crate::services::audio::Device> = au
        .sinks
        .iter()
        .filter(|d| d.plugged || d.name == au.default_sink)
        .collect();
    let sink_names = short_names(sinks.iter().map(|d| device_name(&d.description)));
    let mut rows = Vec::new();
    if let Some(d) = au.sink() {
        let now = sinks
            .iter()
            .zip(&sink_names)
            .find(|(x, _)| x.name == d.name)
            .map_or_else(
                || device_name(&d.description).to_string(),
                |(_, n)| n.clone(),
            );
        let open = pick == Some("out");
        rows.push(row_of(
            s,
            "Output device",
            None,
            chooser(s, now, "out", open),
        ));
        if open {
            for (x, n) in sinks.iter().zip(&sink_names) {
                rows.push(choice_row(
                    s,
                    n.clone(),
                    x.name == d.name,
                    snd(C::Sink(x.name.clone())),
                ));
            }
        }
        rows.push(row_of(
            s,
            "Volume",
            None,
            level(
                s,
                d.volume,
                d.muted,
                |v| snd(C::SinkVolume(v)),
                snd(C::SinkMute),
                (Icon::Volume2, Icon::VolumeX),
            ),
        ));
        rows.push(row_of(
            s,
            "Level",
            None,
            level_meter(s, if d.muted { 0.0 } else { app.meters.output }),
        ));
        // Bluetooth: the codec and mode (music quality or calls with the microphone).
        if let Some(card) = Some(d)
            .filter(|d| d.bluetooth)
            .and_then(|d| d.card)
            .and_then(|c| au.cards.iter().find(|k| k.index == c))
            .filter(|k| k.profiles.len() > 1)
        {
            let now = card
                .profiles
                .iter()
                .find(|(n, _)| *n == card.active)
                .map_or_else(String::new, |(_, desc)| mode_name(desc));
            let open = pick == Some("mode");
            rows.push(row_of(s, "Quality", None, chooser(s, now, "mode", open)));
            if open {
                for (name, desc) in &card.profiles {
                    rows.push(choice_row(
                        s,
                        mode_name(desc),
                        *name == card.active,
                        snd(C::CardMode(card.index, name.clone())),
                    ));
                }
            }
        }
        // The volume keys: how much a press changes, and whether they may go past 100 %.
        let cfg = app.config.sound;
        let steps: Vec<(&str, bool, AppMessage)> = [("2%", 2u32), ("5%", 5), ("10%", 10)]
            .into_iter()
            .map(|(l, v)| (l, cfg.step == v, msg(Message::SoundStep(v))))
            .collect();
        rows.push(row_of(s, "Volume keys step", None, segments(s, &steps)));
        rows.push(row_of(
            s,
            "Allow above 100%",
            None,
            ui::switch(
                s,
                cfg.max > 100,
                msg(Message::SoundMax(if cfg.max > 100 { 100 } else { 150 })),
            ),
        ));
        if d.channels == 2 {
            let v = ((d.balance + 1.0) / 2.0).clamp(0.0, 1.0);
            let bar = crate::ui::slider::slider(
                row![
                    container(label("L", text::CAPTION, s.ink_dim)).padding([tok::NONE, tok::M]),
                    iced::widget::Space::new().width(Length::Fill),
                    container(label("R", text::CAPTION, s.ink_dim)).padding([tok::NONE, tok::M]),
                ]
                .align_y(Alignment::Center),
                v,
                22.0,
                (
                    s.ink.scale_alpha(tok::FILL * s.opacity),
                    s.accent.scale_alpha(0.35 * s.opacity),
                ),
                |v| snd(C::Balance(((v * 2.0 - 1.0) * 20.0).round() / 20.0)),
            );
            let even = d.balance.abs() < 0.03;
            rows.push(row_of(
                s,
                "Balance",
                None,
                row![
                    container(bar).width(220),
                    container(label(
                        if even {
                            "Even".to_string()
                        } else {
                            format!("{:+.0}", d.balance * 100.0)
                        },
                        text::SMALL,
                        s.ink_dim
                    ))
                    .width(44)
                    .align_right(44)
                ]
                .spacing(tok::M)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
    }
    // Both speakers, one after the other.
    let busy = app.panels.settings.speaker_test;
    let mut test = control(label(
        if busy { "Playing…" } else { "Play" },
        text::SMALL,
        s.ink,
    ))
    .pill(s, crate::ui::control::Pill::Plain);
    if !busy {
        test = test.on_press(msg(Message::SpeakerTest));
    }
    rows.push(row_of(
        s,
        "Test speakers, left then right",
        None,
        test.into(),
    ));
    out.extend(group(s, Some("Output"), rows));

    // Input.
    let mut rows = Vec::new();
    if let Some(d) = au.source() {
        let sources: Vec<&crate::services::audio::Device> = au
            .sources
            .iter()
            .filter(|d| d.plugged || d.name == au.default_source)
            .collect();
        let names = short_names(
            sources
                .iter()
                .map(|d| mic_name(device_name(&d.description))),
        );
        let now = sources
            .iter()
            .zip(&names)
            .find(|(x, _)| x.name == d.name)
            .map_or_else(
                || mic_name(device_name(&d.description)).to_string(),
                |(_, n)| n.clone(),
            );
        let open = pick == Some("in");
        rows.push(row_of(s, "Input device", None, chooser(s, now, "in", open)));
        if open {
            for (x, n) in sources.iter().zip(&names) {
                rows.push(choice_row(
                    s,
                    n.clone(),
                    x.name == d.name,
                    snd(C::Source(x.name.clone())),
                ));
            }
        }
        rows.push(row_of(
            s,
            "Volume",
            None,
            level(
                s,
                d.volume,
                d.muted,
                |v| snd(C::SourceVolume(v)),
                snd(C::SourceMute),
                (Icon::Mic, Icon::MicOff),
            ),
        ));
        rows.push(row_of(
            s,
            "Level",
            None,
            level_meter(s, if d.muted { 0.0 } else { app.meters.input }),
        ));
        // Hear yourself: three seconds recorded, then played back.
        let step = app.panels.settings.mic_test;
        let mut test = control(label(
            match step {
                1 => "Recording… speak now",
                2 => "Playing back…",
                _ => "Record 3 s",
            },
            text::SMALL,
            if step == 1 { s.bad } else { s.ink },
        ))
        .pill(s, crate::ui::control::Pill::Plain);
        if step == 0 {
            test = test.on_press(msg(Message::MicTest));
        }
        rows.push(row_of(s, "Test your microphone", None, test.into()));
    }
    out.extend(group(s, Some("Input"), rows));

    // Each app: its icon and name; its volume, mute and output.
    let app_rows = |list: &'_ [crate::services::audio::App], recording: bool| {
        let mut rows = Vec::new();
        for a in list {
            let ic: Element<'_, AppMessage> = match a.icon_path() {
                Some(p) => ui::image_file_faded(&p, 20.0, s.opacity),
                None => icon(Icon::AppWindow, tok::ICON_M, s.ink_dim).into(),
            };
            let key = format!("{}{}", if recording { "rec:" } else { "app:" }, a.name);
            let open = pick == Some(key.as_str());
            let devices: Vec<&crate::services::audio::Device> = if recording {
                au.sources.iter().filter(|d| d.plugged).collect()
            } else {
                au.sinks.iter().filter(|d| d.plugged).collect()
            };
            let names = short_names(devices.iter().map(|d| {
                if recording {
                    mic_name(device_name(&d.description))
                } else {
                    device_name(&d.description)
                }
            }));
            let current = if a.sink.is_empty() {
                if recording {
                    au.default_source.clone()
                } else {
                    au.default_sink.clone()
                }
            } else {
                a.sink.clone()
            };
            let now = devices
                .iter()
                .zip(&names)
                .find(|(d, _)| d.name == current)
                .map_or_else(String::new, |(_, n)| n.clone());
            let ids = a.streams.clone();
            let ids2 = a.streams.clone();
            let lvl = if recording {
                level(
                    s,
                    a.volume,
                    a.muted,
                    move |v| snd(C::RecVolume(ids.clone(), v)),
                    snd(C::RecMute(ids2)),
                    (Icon::Mic, Icon::MicOff),
                )
            } else {
                level(
                    s,
                    a.volume,
                    a.muted,
                    move |v| snd(C::AppVolume(ids.clone(), v)),
                    snd(C::AppMute(ids2)),
                    (Icon::Volume2, Icon::VolumeX),
                )
            };
            rows.push(Some(
                container(
                    column![
                        row![
                            ic,
                            label(a.name.clone(), text::BODY, s.ink).width(Length::Fill),
                            chooser(s, now, &key, open),
                        ]
                        .spacing(tok::L)
                        .align_y(Alignment::Center),
                        row![iced::widget::Space::new().width(Length::Fill), lvl]
                            .align_y(Alignment::Center),
                    ]
                    .spacing(tok::S),
                )
                .padding([tok::L, tok::XL])
                .into(),
            ));
            if open {
                for (d, n) in devices.iter().zip(&names) {
                    let ids = a.streams.clone();
                    rows.push(choice_row(
                        s,
                        n.clone(),
                        d.name == current,
                        if recording {
                            snd(C::RecTo(ids, d.name.clone()))
                        } else {
                            snd(C::AppTo(ids, d.name.clone()))
                        },
                    ));
                }
            }
        }
        rows
    };
    out.extend(group(s, Some("Apps"), app_rows(&au.apps, false)));
    out.extend(group(
        s,
        Some("Using the microphone"),
        app_rows(&au.recorders, true),
    ));
    // Sounds the system makes: notifications (switched in Notifications), the volume keys'
    // click, the screenshot shutter; one volume for all of them.
    let cfg = app.config.sound;
    let loud: Vec<(&str, bool, AppMessage)> = [("Quiet", 40i64), ("Normal", 70), ("Loud", 100)]
        .into_iter()
        .map(|(l, v)| {
            (
                l,
                i64::from(cfg.alerts) == v,
                msg(Message::SoundOpt("alerts", v.into())),
            )
        })
        .collect();
    out.extend(group(
        s,
        Some("System sounds"),
        vec![
            row_of(
                s,
                "Alert volume",
                None,
                row![
                    segments(s, &loud),
                    control(label("Play", text::SMALL, s.ink))
                        .pill(s, crate::ui::control::Pill::Plain)
                        .on_press(msg(Message::AlertPlay)),
                ]
                .spacing(tok::M)
                .align_y(Alignment::Center)
                .into(),
            ),
            row_of(
                s,
                "Click on volume change",
                None,
                ui::switch(
                    s,
                    cfg.feedback,
                    msg(Message::SoundOpt("feedback", (!cfg.feedback).into())),
                ),
            ),
            row_of(
                s,
                "Screenshot sound",
                None,
                ui::switch(
                    s,
                    cfg.screenshot,
                    msg(Message::SoundOpt("screenshot", (!cfg.screenshot).into())),
                ),
            ),
        ],
    ));
    out
}
