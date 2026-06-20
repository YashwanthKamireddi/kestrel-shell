//! Notifications: a bell, with the count of unseen ones (in the accent) or crossed out in Do
//! not disturb; opens the control centre (the list is there). Right click: Do not disturb on
//! or off.

use crate::app::{App, Message, Output};
use crate::panels::{Message as PanelMessage, PanelKind, control};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{Icon, icon, label_w};
use iced::widget::row;
use iced::{Alignment, Element};

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let active = super::control_on(app, open, control::Page::Main);
    let press = move |r| Message::ControlAt(control::Page::Main, id, r);
    let dnd = Message::Panel(PanelMessage::Control(control::Message::Dnd));
    if app.dnd {
        return vec![
            super::icon_chip(s, Icon::BellOff, s.ink_dim, active)
                .on_press_at(press)
                .on_right_press(dnd)
                .into(),
        ];
    }
    if app.unseen == 0 {
        return vec![
            super::icon_chip(s, Icon::Bell, s.ink, active)
                .on_press_at(press)
                .on_right_press(dnd)
                .into(),
        ];
    }
    let count = if app.unseen > 9 {
        "9+".to_string()
    } else {
        app.unseen.to_string()
    };
    let content = row![
        icon(Icon::BellDot, size::ICON, s.accent),
        label_w(count, text::SMALL, s.accent, theme::SEMIBOLD),
    ]
    .spacing(tok::XS)
    .align_y(Alignment::Center);
    vec![
        super::chip(s, content, active)
            .on_press_at(press)
            .on_right_press(dnd)
            .into(),
    ]
}
