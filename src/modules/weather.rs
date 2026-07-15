//! Weather now: the condition's icon in its colour and the temperature; opens the weather
//! panel.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::tok;
use crate::theme::{Scheme, size};
use crate::ui::{icon, label};
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
        label(app.temp(w.temp), crate::theme::bar::TEXT, s.ink),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    vec![
        super::chip(s, content, active)
            .on_press_at(move |r| Message::Toggle(PanelKind::Weather, id, r))
            .into(),
    ]
}
