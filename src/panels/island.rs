//! The dashboard the island opens: today and the weather, what's playing, games, the system,
//! and the month. Each section appears only when it has something to say.

use super::{calendar, media};
use crate::app::{App, Message as AppMessage};
use crate::services::web::Phase;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, card, control, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Color, Element, Length};

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let mut col = column![today(app, s)].spacing(12);
    if let Some(p) = media::current(app) {
        col = col.push(playing(s, p));
    }
    if !app.nba.is_empty() {
        col = col.push(games(app, s));
    }
    col = col.push(system(app, s));
    col = col.push(card(s, calendar::month(app, s)));
    container(col).padding(12).into()
}

/// Time, date and the weather, side by side.
fn today(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let when = column![
        label_w(
            app.now.format("%H:%M").to_string(),
            theme::text::DISPLAY,
            s.ink,
            theme::SEMIBOLD
        ),
        label(
            app.now.format("%A, %-d %B").to_string(),
            text::BODY,
            s.ink_dim
        ),
    ]
    .spacing(2)
    .width(Length::Fill);
    let mut r = row![when].align_y(Alignment::Center);
    if let Some(w) = &app.weather {
        r = r.push(
            column![
                row![
                    icon(ui::weather_icon(w), 20.0, s.ink),
                    label_w(
                        format!("{:.0}°", w.temp),
                        theme::text::HEADING,
                        s.ink,
                        theme::MEDIUM
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                label(w.describe(), text::SMALL, s.ink_dim),
                label(
                    format!("H {:.0}°  L {:.0}°", w.high, w.low),
                    text::CAPTION,
                    s.ink_faint
                ),
            ]
            .spacing(2)
            .align_x(Alignment::End),
        );
    }
    container(r).padding([4, 6]).into()
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

/// NBA: live games first, then today's; at most five.
fn games(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let mut list: Vec<&crate::services::web::Game> = app.nba.iter().collect();
    list.sort_by_key(|g| match g.phase {
        Phase::Live => 0,
        Phase::Upcoming => 1,
        Phase::Final => 2,
    });
    let mut col = column![
        row![
            icon(Icon::Trophy, 14.0, s.ink_dim),
            label_w("NBA", text::SMALL, s.ink_dim, theme::MEDIUM),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    ]
    .spacing(8);
    for g in list.into_iter().take(5) {
        let live = g.phase == Phase::Live;
        let score = if g.phase == Phase::Upcoming {
            String::new()
        } else {
            format!("{}  {}", g.away_score, g.home_score)
        };
        col = col.push(
            row![
                label(format!("{} @ {}", g.away, g.home), text::SMALL, s.ink)
                    .width(Length::Fixed(110.0)),
                label(score, text::SMALL, if live { s.ink } else { s.ink_dim }).width(Length::Fill),
                label(
                    g.detail.clone(),
                    text::CAPTION,
                    if live { s.accent } else { s.ink_faint }
                ),
            ]
            .align_y(Alignment::Center),
        );
    }
    card(s, col)
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
