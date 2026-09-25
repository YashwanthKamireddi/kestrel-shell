//! Settings › shortcuts: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// Keys ("SUPER + SHIFT + T") as chips.
pub(super) fn key_chips<'a>(s: Scheme, keys: &str) -> Element<'a, Message> {
    let mut r = row![].spacing(tok::XS).align_y(Alignment::Center);
    for k in keys.split('+').map(str::trim) {
        let shown = match k {
            "SUPER" => "Super".to_string(),
            "CTRL" => "Ctrl".to_string(),
            "ALT" => "Alt".to_string(),
            "SHIFT" => "Shift".to_string(),
            k => k.to_string(),
        };
        r = r.push(crate::ui::keycap(s, shown));
    }
    r.into()
}

/// The bind using these keys already, if any ("Window: Close").
pub(super) fn taken_by<'a>(st: &'a State, keys: &str) -> Option<&'a str> {
    // Both sides lowercased, the cheatsheet's friendly names turned back into key names.
    let norm = |k: &str| -> String {
        match k.trim().to_lowercase().as_str() {
            "/" => "slash".into(),
            "enter" => "return".into(),
            "prtsc" => "print".into(),
            o => o.into(),
        }
    };
    let mut want: Vec<String> = keys.split('+').map(norm).collect();
    want.sort();
    st.binds.iter().flat_map(|(_, v)| v).find_map(|b| {
        let mut have: Vec<String> = b.keys.iter().map(|k| norm(k)).collect();
        have.sort();
        (have == want).then_some(b.what.as_str())
    })
}

pub(super) fn shortcuts_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let mut out: Vec<Element<'_, Message>> = Vec::new();
    // Making one: record the keys, name it, say what it runs.
    let rec: Element<'_, Message> = if st.recording {
        control(label("Press the keys…", text::SMALL, s.on_accent))
            .pill(s, crate::ui::control::Pill::Primary)
            .on_press(Message::Record(false))
            .into()
    } else {
        match &st.new_keys {
            Some(k) => control(key_chips(s, k))
                .padding([tok::XS, tok::M])
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Ghost)
                .on_press(Message::Record(true))
                .into(),
            None => control(label("Record keys", text::SMALL, s.ink))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(Message::Record(true))
                .into(),
        }
    };
    let field = |id: &'static str, ph: &'static str, v: &str, on: fn(String) -> Message| {
        container(
            text_input(ph, v)
                .id(iced::widget::Id::new(id))
                .on_input(on)
                .on_submit(Message::AddShortcut)
                .padding([tok::S, tok::L])
                .size(text::BODY)
                .font(theme::FONT)
                .width(320)
                .style(crate::ui::input_style(s)),
        )
        .style(move |_| ui::field_style(s))
    };
    let conflict = st.new_keys.as_deref().and_then(|k| taken_by(st, k));
    let can_add = st.new_keys.is_some() && !st.new_command.trim().is_empty() && conflict.is_none();
    let mut make = vec![
        row_of(s, "Keys", conflict.map(|_| "Already used"), rec),
        row_of(
            s,
            "Name",
            None,
            field("sc-name", "Name", &st.new_name, Message::NewName).into(),
        ),
        row_of(
            s,
            "Command",
            None,
            field(
                "sc-command",
                "Command",
                &st.new_command,
                Message::NewCommand,
            )
            .into(),
        ),
    ];
    if let Some(c) = conflict {
        make.push(Some(
            container(label(format!("Used by {c}"), text::SMALL, s.bad))
                .padding([tok::NONE, tok::XL])
                .into(),
        ));
    }
    make.push(row_of(
        s,
        "",
        None,
        ui::button(
            s,
            "Add",
            crate::ui::control::Pill::Primary,
            can_add.then(|| Message::AddShortcut),
        )
        .into(),
    ));
    out.extend(group(s, Some("New shortcut"), make));
    // Added here: each removable.
    let mine = st
        .custom
        .iter()
        .map(|c| {
            Some(
                container(
                    row![
                        column![
                            label(c.name.clone(), text::BODY, s.ink),
                            label(c.command.clone(), text::CAPTION, s.ink_dim)
                        ]
                        .spacing(tok::HAIR)
                        .width(Length::Fill),
                        key_chips(s, &c.keys),
                        crate::ui::remove_button(s, Message::RemoveShortcut(c.keys.clone())),
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                )
                .padding([tok::M, tok::XL])
                .into(),
            )
        })
        .collect();
    out.extend(group(s, Some("Yours"), mine));
    // Every shortcut there is, by category.
    for (cat, binds) in &st.binds {
        if cat == "Custom" {
            continue;
        }
        let rows = binds
            .iter()
            .filter(|b| matches(&b.what) || matches(cat))
            .map(|b| {
                let keys = b.keys.join(" + ");
                // A click on the keys records new ones for it.
                let waiting = st.recording && st.rebinding.as_deref() == Some(keys.as_str());
                let keys_el: Element<'_, Message> = if waiting {
                    control(label("Press the new keys…", text::SMALL, s.on_accent))
                        .pill(s, crate::ui::control::Pill::Primary)
                        .on_press(Message::Record(false))
                        .into()
                } else {
                    control(key_chips(s, &keys))
                        .padding([tok::XXS, tok::S])
                        .radius(tok::FULL)
                        .kind(s, crate::ui::control::Kind::Ghost)
                        .on_press(Message::Rebind(keys.clone()))
                        .into()
                };
                Some(
                    container(
                        row![
                            label(b.what.clone(), text::BODY, s.ink).width(Length::Fill),
                            keys_el,
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::XL])
                    .height(40)
                    .align_y(Alignment::Center)
                    .into(),
                )
            })
            .collect();
        out.extend(group(s, Some(cat.as_str()), rows));
    }
    out
}
