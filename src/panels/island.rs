//! The dashboard the island opens: today and the weather, what's playing, games and the month
//! (the system rings went, design call 2026-11-16). Each section appears only when it has something to say.

use super::{calendar, media};
use crate::app::{App, Message as AppMessage};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, card, control, icon, label, label_w};
use iced::widget::{column, container, row};
use iced::{Alignment, Color, Element, Length};

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    // Weather and music with pills of their own have their own panels: not repeated here.
    let mut col = column![today(app, s)].spacing(tok::L);
    if !app.config.bar.contains("media")
        && let Some(p) = media::current(app)
    {
        col = col.push(playing(s, p));
    }
    col = col.push(card(s, calendar::month(app, s)));
    container(col).padding(tok::L).into()
}

/// Time, date and the weather, on a sky that matches the weather and the time of day.
fn today(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let Some(w) = app
        .weather
        .as_ref()
        .filter(|_| !app.config.bar.contains("weather"))
    else {
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

/// What's playing, with the essential controls.
fn playing<'a>(s: Scheme, p: &'a crate::services::mpris::Player) -> Element<'a, AppMessage> {
    let bus = p.bus.clone();
    let msg = |m: media::Message| AppMessage::Panel(super::Message::Media(m));
    let button = |i: Icon, m: Option<AppMessage>, primary: bool| {
        let fg = if primary {
            s.on_accent
        } else if m.is_some() {
            s.ink
        } else {
            s.ink_faint
        };
        control(container(icon(i, 16.0, fg)).center(32))
            .radius(tok::FULL)
            .kind(
                s,
                if primary {
                    crate::ui::control::Kind::Primary
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .on_press_maybe(m)
    };
    let mut meta = column![label_w(
        ui::two_lines(&p.title, text::BODY, 210.0),
        text::BODY,
        s.ink,
        theme::MEDIUM
    )]
    .spacing(tok::XXS)
    .width(Length::Fill);
    if !p.artist.is_empty() {
        meta = meta.push(label(
            ui::fit(&p.artist, text::SMALL, 210.0),
            text::SMALL,
            s.ink_dim,
        ));
    }
    card(
        s,
        row![
            media::art(s, p, 48.0, 10.0),
            meta,
            row![
                button(
                    Icon::SkipBack,
                    p.can_prev
                        .then(|| msg(media::Message::Previous(bus.clone()))),
                    false
                ),
                button(
                    if p.playing { Icon::Pause } else { Icon::Play },
                    Some(msg(media::Message::PlayPause(bus.clone()))),
                    true
                ),
                button(
                    Icon::SkipForward,
                    p.can_next.then(|| msg(media::Message::Next(bus.clone()))),
                    false
                ),
            ]
            .spacing(tok::XXS)
            .align_y(Alignment::Center),
        ]
        .spacing(tok::L)
        .align_y(Alignment::Center),
    )
}
