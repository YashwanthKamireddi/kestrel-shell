//! The control centre's quiet page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A time when Do not disturb ends, as people say it ("14:30", "tomorrow 07:00").
pub fn until_text(t: u64) -> String {
    use chrono::TimeZone;
    let Some(when) = chrono::Local.timestamp_opt(t as i64, 0).single() else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    if when.date_naive() == today {
        when.format("%H:%M").to_string()
    } else {
        when.format("tomorrow %H:%M").to_string()
    }
}

/// Do not disturb: on for an hour, a few hours, until the morning, or until turned off.
pub(super) fn quiet_page(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let header = subpage_header(
        s,
        "Do not disturb",
        msg(Message::Page(Page::Main)),
        switch(s, app.dnd, msg(Message::Dnd)),
    );
    let until = crate::state::get(|st| st.toggles.dnd_until);
    let mut list = column![].spacing(tok::XXS);
    for (name, minutes) in [
        ("For 1 hour", 60u64),
        ("For 3 hours", 180),
        ("Until tomorrow morning", u64::MAX),
        ("Until I turn it off", 0),
    ] {
        let on = app.dnd && minutes == 0 && until.is_none();
        list = list.push(list_row(
            s,
            icon(
                Icon::BellOff,
                tok::ICON,
                if on { s.accent } else { s.ink_dim },
            )
            .into(),
            name,
            None,
            if on {
                icon(Icon::Check, tok::ICON, s.accent).into()
            } else {
                Space::new().into()
            },
            on,
            Some(msg(Message::DndFor(minutes))),
        ));
    }
    if app.dnd
        && let Some(t) = until
    {
        list = list.push(
            container(label(
                format!("On until {}", until_text(t)),
                text::SMALL,
                s.ink_dim,
            ))
            .padding([tok::S, tok::L]),
        );
    }
    column![header, list].spacing(tok::M).into()
}
