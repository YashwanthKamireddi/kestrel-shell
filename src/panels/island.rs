//! The dashboard the island opens: today and the weather, what's playing, games, the system,
//! and the month. Each section appears only when it has something to say.

use super::{calendar, media};
use crate::app::{App, Message as AppMessage};
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, card, control, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Color, Element, Length};

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let mut col = column![today(app, s)].spacing(12);
    if let Some(p) = media::current(app) {
        col = col.push(playing(s, p));
    }
    col = col.push(system(app, s));
    col = col.push(card(s, calendar::month(app, s)));
    container(col).padding(12).into()
}

/// Time, date and the weather, on a sky that matches the weather and the time of day.
fn today(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let Some(w) = &app.weather else {
        return container(clock(app, s.ink, s.ink_dim))
            .padding([4, 6])
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
                    format!("{:.0}°", w.temp),
                    theme::text::DISPLAY,
                    white,
                    theme::SEMIBOLD
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            label(w.describe(), text::SMALL, soft),
            label(
                format!("H {:.0}°  L {:.0}°", w.high, w.low),
                text::CAPTION,
                soft
            ),
        ]
        .spacing(2)
        .align_x(Alignment::End),
    ]
    .align_y(Alignment::Center);
    let (top, bottom) = ui::weather_sky(w);
    let (top, bottom) = (s.fade(top), s.fade(bottom));
    container(r)
        .padding([16, 18])
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
        label_w(
            app.now.format("%H:%M").to_string(),
            theme::text::DISPLAY,
            ink,
            theme::SEMIBOLD
        ),
        label(app.now.format("%A, %-d %B").to_string(), text::BODY, dim),
    ]
    .spacing(2)
    .width(Length::Fill)
    .into()
}

/// What's playing, with the essential controls.
fn playing<'a>(s: Scheme, p: &'a crate::services::mpris::Player) -> Element<'a, AppMessage> {
    let bus = p.bus.clone();
    let msg = |m: media::Message| AppMessage::Panel(super::Message::Media(m));
    let button = |i: Icon, m: Option<AppMessage>, primary: bool| {
        let (fg, bg) = if primary {
            (s.on_accent, s.accent)
        } else {
            (
                if m.is_some() { s.ink } else { s.ink_faint },
                Color::TRANSPARENT,
            )
        };
        control(container(icon(i, 16.0, fg)).center(32))
            .radius(16.0)
            .colors(bg, if primary { bg } else { s.raised_hi }, s.raised_press)
            .on_press_maybe(m)
    };
    let mut meta = column![label_w(
        ui::fit(&p.title, text::BODY, 210.0),
        text::BODY,
        s.ink,
        theme::MEDIUM
    )]
    .spacing(2)
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
            .spacing(2)
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
}

/// CPU, memory and temperature as quiet level bars.
fn system(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let sys = app.system;
    let meter = |i: Icon, name: &'static str, value: f32, shown: String| {
        let hot = value >= 0.85;
        let width = 120.0;
        let bar = iced::widget::stack![
            container(Space::new())
                .width(width)
                .height(4)
                .style(move |_| container::Style {
                    background: Some(s.raised_hi.into()),
                    border: iced::Border::default().rounded(2),
                    ..container::Style::default()
                }),
            container(Space::new())
                .width((width * value.clamp(0.0, 1.0)).max(4.0))
                .height(4)
                .style(move |_| container::Style {
                    background: Some(if hot { s.warn } else { s.accent }.into()),
                    border: iced::Border::default().rounded(2),
                    ..container::Style::default()
                }),
        ];
        row![
            icon(i, 14.0, s.ink_dim),
            label(name, text::SMALL, s.ink_dim).width(Length::Fixed(60.0)),
            container(bar).center_y(16).width(Length::Fill),
            label(shown, text::SMALL, if hot { s.warn } else { s.ink }).width(Length::Fixed(44.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    };
    let mut col = column![
        meter(
            Icon::Cpu,
            "CPU",
            f32::from(sys.cpu) / 100.0,
            format!("{}%", sys.cpu)
        ),
        meter(
            Icon::MemoryStick,
            "Memory",
            f32::from(sys.mem) / 100.0,
            format!("{}%", sys.mem)
        ),
    ]
    .spacing(8);
    if let Some(t) = sys.temp {
        col = col.push(meter(
            Icon::Thermometer,
            "Temp",
            f32::from(t) / 100.0,
            format!("{t}°"),
        ));
    }
    card(s, col)
}
