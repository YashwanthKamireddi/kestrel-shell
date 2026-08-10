//! Settings › storage: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// Opens a folder in the file manager and closes Settings.
pub(super) fn open_folder(path: String) -> Task<AppMessage> {
    // The file manager chosen in Default apps (a terminal one too).
    Task::batch([
        Task::done(AppMessage::Ipc(format!("open-default files {path}"))),
        Task::done(AppMessage::Close),
    ])
}

/// "131 GB", "980 MB".
pub(super) fn bytes_text(b: u64) -> String {
    let gb = b as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else if gb >= 1.0 {
        format!("{gb:.0} GB")
    } else {
        format!("{:.0} MB", b as f64 / 1e6)
    }
}

pub(super) fn storage_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let volume = |v: &crate::services::disks::Volume| -> Option<Element<'_, AppMessage>> {
        let used = v.free.map(|f| v.size.saturating_sub(f));
        let share = used.map_or(0.0, |u| u as f32 / v.size.max(1) as f32);
        let line = match (used, v.free) {
            (Some(_), Some(f)) => format!("{} free of {}", bytes_text(f), bytes_text(v.size)),
            _ => bytes_text(v.size),
        };
        let bar = row![
            container(iced::widget::Space::new())
                .width(Length::FillPortion(((share * 1000.0) as u16).max(1)))
                .height(4)
                .style(move |_| container::Style {
                    background: Some((if share > 0.9 { s.bad } else { s.accent }).into()),
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                }),
            container(iced::widget::Space::new())
                .width(Length::FillPortion(
                    (((1.0 - share) * 1000.0) as u16).max(1)
                ))
                .height(4)
                .style(move |_| container::Style {
                    background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                }),
        ];
        let mut actions = row![].spacing(tok::S).align_y(Alignment::Center);
        if v.removable || v.mount.as_deref().is_some_and(|m| m != "/" && m != "/boot") {
            actions = actions.push(
                control(label_w("Open", text::SMALL, s.ink, theme::MEDIUM))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(msg(Message::VolumeOpen(v.path.clone(), v.mount.clone()))),
            );
        }
        if v.removable {
            actions = actions.push(
                control(icon(Icon::Eject, tok::ICON, s.ink))
                    .padding(tok::S)
                    .radius(tok::FULL)
                    .kind(s, crate::ui::control::Kind::Ghost)
                    .on_press(msg(Message::VolumeEject(v.path.clone(), v.drive.clone()))),
            );
        }
        Some(
            container(
                column![
                    row![
                        icon(
                            if v.removable {
                                Icon::Usb
                            } else {
                                Icon::HardDrive
                            },
                            18.0,
                            s.ink_dim
                        ),
                        column![
                            label(v.name.clone(), text::BODY, s.ink),
                            label(line, text::CAPTION, s.ink_dim)
                        ]
                        .spacing(tok::HAIR)
                        .width(Length::Fill),
                        actions,
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                    bar,
                ]
                .spacing(tok::M),
            )
            .padding([tok::L, tok::XL])
            .into(),
        )
    };
    let fixed: Vec<_> = st
        .volumes
        .iter()
        .filter(|v| !v.removable)
        .map(volume)
        .collect();
    let plugged: Vec<_> = st
        .volumes
        .iter()
        .filter(|v| v.removable)
        .map(volume)
        .collect();
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    out.extend(group(s, Some("This computer"), fixed));
    out.extend(group(s, Some("Drives"), plugged));
    // The home folder: what fills it, biggest first; a click opens the folder.
    match &st.usage {
        None => out.push(crate::ui::waiting(
            s,
            Icon::HardDrive,
            "Measuring your home folder…",
        )),
        Some((total, items)) => {
            let rows = items
                .iter()
                .map(|(p, n)| {
                    let name = p
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let share = *n as f32 / (*total).max(1) as f32;
                    Some(
                        control(
                            column![
                                row![
                                    icon(Icon::Folder, tok::ICON, s.ink_dim),
                                    label(name, text::BODY, s.ink).width(Length::Fill),
                                    label(bytes_text(*n), text::SMALL, s.ink_dim),
                                ]
                                .spacing(tok::L)
                                .align_y(Alignment::Center),
                                row![
                                    container(iced::widget::Space::new())
                                        .width(Length::FillPortion(
                                            ((share * 1000.0) as u16).max(1)
                                        ))
                                        .height(3)
                                        .style(move |_| container::Style {
                                            background: Some(s.accent.into()),
                                            border: iced::Border::default().rounded(tok::FULL),
                                            ..container::Style::default()
                                        }),
                                    iced::widget::Space::new()
                                        .width(Length::FillPortion(
                                            (((1.0 - share) * 1000.0) as u16).max(1)
                                        ))
                                        .height(3),
                                ],
                            ]
                            .spacing(tok::S),
                        )
                        .padding([tok::L, tok::XL])
                        .width(Length::Fill)
                        .kind(s, crate::ui::control::Kind::Ghost)
                        .no_press_scale()
                        .on_press(msg(Message::VolumeOpen(
                            String::new(),
                            Some(p.to_string_lossy().into_owned()),
                        )))
                        .into(),
                    )
                })
                .collect();
            out.extend(group(s, Some("Home folder"), rows));
        }
    }
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
