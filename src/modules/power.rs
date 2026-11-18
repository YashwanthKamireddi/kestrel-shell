//! The power button: opens the power menu (which asks before anything destructive).

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::Scheme;
use crate::ui::Icon;
use iced::Element;

pub fn view<'a>(
    _app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let active = open == Some(PanelKind::Power);
    vec![
        super::icon_chip(s, Icon::Power, s.ink, active)
            .on_press_at(move |b| Message::Toggle(PanelKind::Power, id, b))
            .into(),
    ]
}
