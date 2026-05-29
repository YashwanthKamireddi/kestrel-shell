//! Settings › updates: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn updates_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let mut out = package_updates(app, s);
    // Firmware (fwupd, from the LVFS): each update a row; Update runs fwupd's own updater in
    // the terminal (it asks before anything, and says what it needs).
    let fw = &app.panels.settings.firmware;
    if !fw.is_empty() {
        let rows = fw
            .iter()
            .map(|f| {
                let mut note = Vec::new();
                if f.needs_ac {
                    note.push("plug in first");
                }
                if f.needs_reboot {
                    note.push("restarts to install");
                }
                Some(
                    container(
                        row![
                            column![
                                label(f.device.clone(), text::BODY, s.ink),
                                label(
                                    if note.is_empty() {
                                        format!("{} → {}", f.from, f.to)
                                    } else {
                                        format!("{} → {} · {}", f.from, f.to, note.join(", "))
                                    },
                                    text::CAPTION,
                                    s.ink_dim
                                ),
                            ]
                            .spacing(tok::XXS)
                            .width(Length::Fill),
                            control(label("Update…", text::SMALL, s.ink))
                                .pill(s, crate::ui::control::Pill::Plain)
                                .on_press(msg(Message::FirmwareUpdate(f.id.clone()))),
                        ]
                        .spacing(tok::L)
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::XL])
                    .height(56)
                    .align_y(Alignment::Center)
                    .into(),
                )
            })
            .collect();
        out.extend(group(s, Some("Firmware"), rows));
    }
    out
}

pub(super) fn package_updates(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let Some(list) = &st.updates else {
        return vec![crate::ui::waiting(
            s,
            Icon::Package,
            "Checking for updates…",
        )];
    };
    if list.is_empty() {
        return group(s, None, vec![value_row(s, "Up to date", String::new())])
            .into_iter()
            .collect();
    }
    let n = list.len();
    let head = group(
        s,
        None,
        vec![row_of(
            s,
            if n == 1 {
                "1 update"
            } else {
                "Updates available"
            },
            None,
            row![
                label(format!("{n}"), text::BODY, s.ink_dim),
                control(label_w(
                    "Update all",
                    text::SMALL,
                    s.on_accent,
                    theme::SEMIBOLD
                ))
                .pill(s, crate::ui::control::Pill::Primary)
                .on_press(AppMessage::Batch(vec![
                    msg(Message::InTerminal(
                        "sh -c 'yay -Syu; echo; read -p \"Done. Press Enter to close.\" _'",
                        "Updates"
                    )),
                    AppMessage::Close,
                ])),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center)
            .into(),
        )],
    );
    let rows = list
        .iter()
        .map(|p| {
            Some(
                container(
                    row![
                        label(p.name.clone(), text::BODY, s.ink).width(Length::Fill),
                        label(format!("{}  →  {}", p.from, p.to), text::SMALL, s.ink_dim),
                        if p.aur {
                            Element::from(
                                container(label("AUR", text::CAPTION, s.ink_dim)).padding([1, 6]),
                            )
                        } else {
                            iced::widget::Space::new().into()
                        },
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                )
                .padding([tok::NONE, tok::XL])
                .height(38)
                .align_y(Alignment::Center)
                .into(),
            )
        })
        .collect();
    [head, group(s, Some("Packages"), rows)]
        .into_iter()
        .flatten()
        .collect()
}
