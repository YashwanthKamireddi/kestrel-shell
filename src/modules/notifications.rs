//! Notifications: a bell, with the count of unseen ones (in the accent) or crossed out in Do
//! not disturb; opens the notifications at the island. Right click: Do not disturb on
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
    // With the island in the bar, notifications are part of it (inside its pill): no bell of
    // its own beside it.
    if app.config.bar.contains("island") {
        return Vec::new();
    }
    let id = o.id;
    let active = open == Some(PanelKind::Notifications);
    // Opens at the island, the centre of the bar, wherever the bell sits.
    let w = o.size.width;
    let press = move |_r: iced::Rectangle| {
        Message::Toggle(
            PanelKind::Notifications,
            id,
            iced::Rectangle::new(
                iced::Point::new(w / 2.0, 0.0),
                iced::Size::new(0.0, size::BAR),
            ),
        )
    };
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
