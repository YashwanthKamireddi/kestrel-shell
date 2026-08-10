//! The control centre's power page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn power_page(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let header = subpage_header(
        s,
        "Power mode",
        msg(Message::Page(Page::Main)),
        Space::new().into(),
    );
    let current = app.power.profile;
    let mut list = column![].spacing(tok::XXS);
    for (p, i, note) in [
        (
            power::Profile::PowerSaver,
            Icon::Leaf,
            "Longest battery, slower",
        ),
        (power::Profile::Balanced, Icon::Gauge, "Speed when needed"),
        (
            power::Profile::Performance,
            Icon::Rocket,
            "Fastest, uses more power",
        ),
    ] {
        let active = current == Some(p);
        list = list.push(list_row(
            s,
            icon(i, tok::ICON, if active { s.accent } else { s.ink }).into(),
            p.label().to_string(),
            Some(note.to_string()),
            if active {
                icon(Icon::Check, tok::ICON, s.accent).into()
            } else {
                Space::new().into()
            },
            active,
            Some(msg(Message::SetProfile(p))),
        ));
    }
    column![header, list].spacing(tok::M).into()
}
