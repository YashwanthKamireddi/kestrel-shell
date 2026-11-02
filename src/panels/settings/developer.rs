//! Settings › developer: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// What `kestrel <command>` does (the IPC the keybinds use): (what, the command).
pub(super) const COMMANDS: [(&str, &str); 11] = [
    ("Open or close a panel", "kestrel toggle launcher"),
    ("Close the open panel", "kestrel close"),
    ("The bar from the keyboard", "kestrel focus bar"),
    ("Volume", "kestrel volume up"),
    ("Mute", "kestrel volume mute"),
    ("Microphone", "kestrel mic mute"),
    ("Brightness", "kestrel brightness +5"),
    ("Caps Lock and Num Lock", "kestrel osd caps"),
    ("Game mode", "kestrel game-mode"),
    ("Default app for a kind", "kestrel open-default files"),
    ("Settings", "kestrel toggle settings"),
];

/// Panels `kestrel toggle` opens.
pub(super) const PANELS: &str = "launcher overview control calendar power clipboard emoji wallpaper keys capture monitor weather files media privacy settings";

pub(super) fn developer_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let cfg = crate::config::path();
    let lua = cfg.with_file_name("init.lua");
    let open = |p: std::path::PathBuf| -> Element<'_, Message> {
        control(label("Open", text::SMALL, s.ink))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(Message::OpenFile(p))
            .into()
    };
    let mut out = Vec::new();
    out.extend(group(
        s,
        Some("kestrel"),
        vec![
            row_of(s, "Settings file", None, open(cfg)),
            row_of(s, "Lua (bar, widgets, events)", None, open(lua)),
            row_of(
                s,
                "Log",
                None,
                control(label("Show", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(Message::InTerminal(
                        "journalctl --user -u kestrel -f -n 200",
                        "kestrel log",
                    ))
                    .into(),
            ),
            row_of(
                s,
                "Restart the shell",
                None,
                control(label(
                    if st.restart_armed {
                        "Restart?"
                    } else {
                        "Restart"
                    },
                    text::SMALL,
                    if st.restart_armed { s.bad } else { s.ink },
                ))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(Message::Restart)
                .into(),
            ),
        ],
    ));
    let mut rows: Vec<Option<Element<'_, Message>>> = COMMANDS
        .iter()
        .map(|(what, cmd)| copy_row(s, what, (*cmd).to_string()))
        .collect();
    // The panels' names, wrapped under their title (too long for one row).
    rows.push(Some(
        container(
            column![
                label("Panels for toggle", text::BODY, s.ink),
                iced::widget::text(PANELS)
                    .size(text::SMALL)
                    .color(s.ink_dim)
                    .font(theme::FONT)
                    .wrapping(iced::widget::text::Wrapping::Word),
            ]
            .spacing(tok::XS),
        )
        .padding([tok::L, tok::XL])
        .width(Length::Fill)
        .into(),
    ));
    out.extend(group(s, Some("Commands (a click copies)"), rows));
    out
}
