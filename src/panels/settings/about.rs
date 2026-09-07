//! Settings › about: its page.

use super::*;
use crate::theme::tok;

/// The About page as text, one line each (the computer's name left out).
pub(super) fn system_report(a: &crate::services::about::About) -> String {
    let mut lines = vec![
        format!("Device: {}", a.device),
        format!("System: {}", a.os),
        format!("Kernel: {}", a.kernel),
        format!("Processor: {}", a.processor),
        format!("Memory: {}", a.memory),
    ];
    for g in &a.graphics {
        lines.push(format!("Graphics: {g}"));
    }
    lines.push(format!("Disk: {}", a.disk));
    lines.push(format!("Compositor: {}", a.compositor));
    lines.push(format!("Shell: kestrel {}", env!("CARGO_PKG_VERSION")));
    lines.join("\n")
}

pub(super) fn about_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let Some(a) = &st.about else {
        return Vec::new();
    };
    let mut hardware = vec![
        value_row(s, "Processor", a.processor.clone()),
        value_row(s, "Memory", a.memory.clone()),
    ];
    for g in &a.graphics {
        hardware.push(value_row(s, "Graphics", g.clone()));
    }
    hardware.push(value_row(s, "Disk", a.disk.clone()));
    [
        group(
            s,
            None,
            vec![
                value_row(s, "Device", a.device.clone()),
                row_of(
                    s,
                    "Name",
                    None,
                    match &st.rename {
                        Some(t) => container(
                            text_input("Name", t)
                                .id(iced::widget::Id::new("rename"))
                                .on_input(|t| Message::Rename(Some(t)))
                                .on_submit(Message::RenameSubmit)
                                .padding([tok::S, tok::L])
                                .size(text::BODY)
                                .font(theme::FONT)
                                .width(220)
                                .style(crate::ui::input_style(s)),
                        )
                        .style(move |_| ui::field_style(s))
                        .into(),
                        None => control(label(a.hostname.clone(), text::SMALL, s.ink))
                            .pill(s, crate::ui::control::Pill::Plain)
                            .on_press(Message::Rename(Some(a.hostname.clone())))
                            .into(),
                    },
                ),
            ],
        ),
        group(s, Some("Hardware"), hardware),
        group(
            s,
            Some("Software"),
            vec![
                value_row(s, "System", a.os.clone()),
                value_row(s, "Kernel", a.kernel.clone()),
                value_row(s, "Compositor", a.compositor.clone()),
                value_row(s, "Shell", format!("kestrel {}", env!("CARGO_PKG_VERSION"))),
                value_row(s, "Up for", a.uptime.clone()),
            ],
        ),
        // Everything above as plain text, for a bug report or a forum post (no computer name).
        group(
            s,
            None,
            vec![row_of(
                s,
                "System report",
                None,
                control(label("Copy", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(Message::Copy(system_report(a)))
                    .into(),
            )],
        ),
    ]
    .into_iter()
    .flatten()
    .chain(
        st.error
            .clone()
            .map(|e| label(e, text::SMALL, s.bad).into()),
    )
    .collect()
}
