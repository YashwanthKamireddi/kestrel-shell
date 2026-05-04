//! Weather now: the condition's icon in its colour and the temperature; opens the weather
//! panel.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{icon, label_w};
use iced::widget::row;
use iced::{Alignment, Element};

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let Some(w) = &app.weather else {
        return Vec::new();
    };
    let active = open == Some(PanelKind::Weather);
    let content = row![
        icon(
            crate::ui::weather_icon(w),
            size::ICON,
            crate::ui::weather_tint(w)
        ),
        label_w(app.temp(w.temp), text::BODY, s.ink, theme::SEMIBOLD),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    vec![
        super::chip(s, content, active)
            .on_press_at(move |r| Message::Toggle(PanelKind::Weather, id, r))
            .into(),
    ]
}
