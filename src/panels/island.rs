//! The day, opened from the island's time: the time and the weather on its sky, the weather's
//! details, the next hours and the coming days, and the month. The weather's one home (its
//! own panel is gone, design call 2026-11-15); music opens from the song in the island and the
//! system has its own monitor, so neither repeats here.

use super::calendar;
use crate::app::{App, Message as AppMessage};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, card, icon, label, label_w};
use iced::widget::{column, container, row};
use iced::{Alignment, Color, Element, Length};

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let mut col = column![today(app, s)].spacing(tok::L);
    if let Some(w) = &app.weather {
        col = col.push(super::weather::details(app, w, s));
        if w.hours.len() > 1 {
            col = col.push(super::weather::hours(app, w, s));
        }
        if w.days.len() > 1 {
            col = col.push(super::weather::days(app, w, s));
        }
    }
    col = col.push(card(s, calendar::month(app, s)));
    container(col).padding(tok::L).into()
}

/// Time, date and the weather, on a sky that matches the weather and the time of day.
fn today(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let Some(w) = app.weather.as_ref() else {
        return container(clock(app, s.ink, s.ink_dim))
            .padding([tok::XS, tok::S])
            .into();
    };
    let white = s.fade(Color::from_rgb8(0xF7, 0xF8, 0xFC));
    let soft = s.fade(Color::from_rgb8(0xF7, 0xF8, 0xFC).scale_alpha(0.78));
    let r = row![
        clock(app, white, soft),
        column![
            row![
                icon(ui::weather_icon(w), 28.0, s.fade(ui::weather_tint(w))),
                label_w(
                    app.temp(w.temp),
                    theme::text::DISPLAY,
                    white,
                    theme::SEMIBOLD
                ),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
            label(w.describe(), text::SMALL, soft),
            label(
                format!("H {}  L {}", app.temp(w.high), app.temp(w.low)),
                text::CAPTION,
                soft
            ),
        ]
        .spacing(tok::XXS)
        .align_x(Alignment::End),
    ]
    .align_y(Alignment::Center);
    let (top, bottom) = ui::weather_sky(w);
    let (top, bottom) = (s.fade(top), s.fade(bottom));
    container(r)
        .padding([tok::XL, tok::XL])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                    .add_stop(0.0, top)
                    .add_stop(1.0, bottom)
                    .into(),
            ),
            border: iced::Border::default().rounded(crate::theme::size::CARD_RADIUS),
            ..container::Style::default()
        })
        .into()
}

fn clock(app: &App, ink: Color, dim: Color) -> Element<'_, AppMessage> {
    column![
        label_w(app.time_text(), theme::text::DISPLAY, ink, theme::SEMIBOLD),
        label(app.now.format("%A, %-d %B").to_string(), text::BODY, dim),
    ]
    .spacing(tok::XXS)
    .width(Length::Fill)
    .into()
}
