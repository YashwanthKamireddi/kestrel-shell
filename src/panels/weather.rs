//! Weather, opened from the weather circle: now on a sky that matches it, the details (feels
//! like, humidity, wind, sun), the next hours and the coming days. Only the weather.

use crate::app::{App, Message as AppMessage};
use crate::services::web::Weather;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Color, Element, Length};

pub const WIDTH: f32 = crate::theme::tok::PANEL_M;

/// Where every section's content starts and ends inside the panel: the sky card's text, the
/// headings, the hours and the days all line up on it (they sat at four different insets).
const INSET: f32 = tok::L;

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let Some(w) = &app.weather else {
        // No forecast yet: its icon only (no "No weather yet" or hint lines: the user's rules).
        return crate::ui::empty(s, Icon::Cloud);
    };
    let mut col = column![now(app, w, s), details(app, w, s)].spacing(tok::L);
    if w.hours.len() > 1 {
        col = col.push(hours(app, w, s));
    }
    if w.days.len() > 1 {
        col = col.push(days(app, w, s));
    }
    container(col).padding(tok::L).into()
}

/// Now: place, temperature, condition and today's range, on the sky.
pub fn now<'a>(app: &App, w: &'a Weather, s: Scheme) -> Element<'a, AppMessage> {
    let white = s.fade(Color::from_rgb8(0xF7, 0xF8, 0xFC));
    let soft = s.fade(Color::from_rgb8(0xF7, 0xF8, 0xFC).scale_alpha(0.78));
    let left = column![
        label_w(
            ui::fit(&w.place, text::BODY, 220.0),
            text::BODY,
            white,
            theme::SEMIBOLD
        ),
        label_w(app.temp(w.temp), 48.0, white, theme::MEDIUM),
        label(
            format!("H {}  L {}", app.temp(w.high), app.temp(w.low)),
            text::SMALL,
            soft
        ),
    ]
    .spacing(tok::NONE)
    .width(Length::Fill);
    let right = column![
        icon(
            ui::weather_icon(w),
            tok::ICON_DISPLAY,
            s.fade(ui::weather_tint(w))
        ),
        label_w(w.describe(), text::BODY, white, theme::MEDIUM),
    ]
    .spacing(tok::S)
    .align_x(Alignment::End);
    let (top, bottom) = ui::weather_sky(w);
    let (top, bottom) = (s.fade(top), s.fade(bottom));
    // Its text on the panel's one left and right edge (INSET), like every row under it.
    container(row![left, right].align_y(Alignment::Center))
        .padding([tok::XL, INSET])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                    .add_stop(0.0, top)
                    .add_stop(1.0, bottom)
                    .into(),
            ),
            border: iced::Border::default().rounded(tok::R_CARD),
            ..container::Style::default()
        })
        .into()
}

/// Feels like, humidity, wind, and the sun: four even columns.
fn details<'a>(app: &App, w: &'a Weather, s: Scheme) -> Element<'a, AppMessage> {
    let cell = |i: Icon, tint: Color, value: String, name: &'static str| {
        column![
            icon(i, tok::ICON_M, s.fade(tint)),
            label_w(value, text::BODY, s.ink, theme::SEMIBOLD),
            label(name, text::CAPTION, s.ink_dim),
        ]
        .spacing(tok::XS)
        .align_x(Alignment::Center)
        .width(Length::Fill)
    };
    let rgb = Color::from_rgb8;
    let (sun_icon, sun_time, sun_name) = sun_next(w);
    row![
        cell(
            Icon::Thermometer,
            ui::temperature_tint(w.feels),
            app.temp(w.feels),
            "Feels like"
        ),
        cell(
            Icon::Humidity,
            rgb(0x7C, 0xB8, 0xFF),
            format!("{:.0}%", w.humidity),
            "Humidity"
        ),
        cell(
            Icon::Air,
            rgb(0xB8, 0xE0, 0xD2),
            match app.config.weather_units {
                crate::config::Units::Celsius => format!("{:.0} km/h", w.wind),
                crate::config::Units::Fahrenheit => format!("{:.0} mph", w.wind * 0.621_371),
            },
            "Wind"
        ),
        cell(sun_icon, rgb(0xFF, 0xB0, 0x6B), sun_time, sun_name),
    ]
    .padding([tok::NONE, INSET - tok::M])
    .into()
}

/// The next sunrise or sunset.
fn sun_next(w: &Weather) -> (Icon, String, &'static str) {
    let now = chrono::Local::now().format("%H:%M").to_string();
    if !w.sunrise.is_empty() && now < w.sunrise {
        (Icon::Sunset, w.sunrise.clone(), "Sunrise")
    } else if !w.sunset.is_empty() && now < w.sunset {
        (Icon::Sunset, w.sunset.clone(), "Sunset")
    } else {
        (Icon::Sunset, w.sunrise.clone(), "Sunrise")
    }
}

fn heading<'a>(s: Scheme, title: &'a str) -> Element<'a, AppMessage> {
    crate::ui::section(s, title, INSET)
}

/// The next hours: time, condition, temperature.
fn hours<'a>(app: &App, w: &'a Weather, s: Scheme) -> Element<'a, AppMessage> {
    let cells = w
        .hours
        .iter()
        .take(8)
        .enumerate()
        .map(|(i, &(h, t, c, day))| {
            let when = if i == 0 {
                "Now".to_string()
            } else {
                match app.config.clock.format {
                    crate::config::ClockFormat::H24 => format!("{h:02}"),
                    crate::config::ClockFormat::H12 => {
                        format!("{} {}", (h + 11) % 12 + 1, if h < 12 { "AM" } else { "PM" })
                    }
                }
            };
            column![
                label(when, text::CAPTION, if i == 0 { s.ink } else { s.ink_dim }),
                icon(
                    ui::weather_icon_for(c, day),
                    18.0,
                    s.fade(ui::weather_tint_for(c, day))
                ),
                label_w(app.temp(t), text::SMALL, s.ink, theme::MEDIUM),
            ]
            .spacing(tok::S)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .into()
        });
    column![
        heading(s, "Next hours"),
        row(cells).padding([tok::NONE, INSET - tok::M])
    ]
    .spacing(tok::M)
    .into()
}

/// The coming days: name, condition, and the range as a bar on the week's scale.
fn days<'a>(app: &App, w: &'a Weather, s: Scheme) -> Element<'a, AppMessage> {
    let lo = w.days.iter().map(|d| d.3).fold(f32::INFINITY, f32::min);
    let hi = w.days.iter().map(|d| d.2).fold(f32::NEG_INFINITY, f32::max);
    let span = (hi - lo).max(1.0);
    // Day, icon, low, range, high in tight columns (the icon floated in a wide gap before).
    const TRACK: f32 = 150.0;
    let mut list = column![heading(s, "This week")].spacing(tok::XXS);
    for (i, (date, code, max, min)) in w.days.iter().enumerate() {
        let name = if i == 0 {
            "Today".to_string()
        } else {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map(|d| d.format("%a").to_string())
                .unwrap_or_default()
        };
        let from = (min - lo) / span * TRACK;
        let width = ((max - min) / span * TRACK).max(6.0);
        let (c1, c2) = (ui::temperature_tint(*min), ui::temperature_tint(*max));
        let range = row![
            Space::new().width(from),
            container(Space::new())
                .width(width)
                .height(5)
                .style(move |_| container::Style {
                    background: Some(
                        iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
                            .add_stop(0.0, s.fade(c1))
                            .add_stop(1.0, s.fade(c2))
                            .into()
                    ),
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                }),
        ];
        let track = container(range)
            .width(TRACK)
            .height(5)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            });
        list = list.push(
            container(
                row![
                    label_w(name, text::BODY, s.ink, theme::MEDIUM).width(64),
                    container(icon(
                        ui::weather_icon_for(*code, true),
                        18.0,
                        s.fade(ui::weather_tint_for(*code, true))
                    ))
                    .center_x(28),
                    Space::new().width(Length::Fill),
                    label(app.temp(*min), text::BODY, s.ink_dim)
                        .width(34)
                        .align_x(Alignment::End),
                    track,
                    label_w(app.temp(*max), text::BODY, s.ink, theme::MEDIUM)
                        .width(34)
                        .align_x(Alignment::End),
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center),
            )
            .padding([tok::S, INSET]),
        );
    }
    list.into()
}
