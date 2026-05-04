//! Time and date (without the island); opens the calendar.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::{self, Scheme, text};
use crate::ui::{label, label_w};
use iced::widget::row;
use iced::{Alignment, Element};

/// Time and date; opens the calendar.
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    // The island is the time pill; a clock beside it would show the time twice.
    if app.config.bar.contains("island") {
        return Vec::new();
    }
    vec![clock(app, o, s, open)]
}

fn clock<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let active = open == Some(PanelKind::Calendar);
    let time = app.time_text();
    let date = app.now.format("%a %-d %b").to_string();
    let id = o.id;
    let mut r = row![label_w(time, text::BODY, s.ink, theme::SEMIBOLD)]
        .spacing(8)
        .align_y(Alignment::Center);
    if app.config.clock.date {
        r = r.push(label(date, text::SMALL, s.ink));
    }
    if let Some(v) = super::island::visualiser(app, s) {
        r = r.push(v);
    }
    super::chip(s, r, active)
        .on_press_at(move |b| Message::Toggle(PanelKind::Calendar, id, b))
        .into()
}
