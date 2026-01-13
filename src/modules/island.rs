//! The island: the bar's centre pill. It always shows the time; when something is live it grows
//! to show that too (a game in progress, music playing), springing to its new width. A click
//! opens the dashboard: today, weather, what's playing, games, system, and the month.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::panels::media;
use crate::services::web::Phase;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, icon, label, label_w};
use iced::widget::row;
use iced::{Alignment, Element};

/// What the island shows next to the time, most important first.
enum Activity<'a> {
    Game(&'a crate::services::web::Game),
    Music(&'a crate::services::mpris::Player),
    None,
}

fn activity(app: &App) -> Activity<'_> {
    if let Some(g) = app.nba.iter().find(|g| g.phase == Phase::Live) {
        return Activity::Game(g);
    }
    match app.media.main() {
        Some(p) if p.playing => Activity::Music(p),
        _ => Activity::None,
    }
}

pub fn bar<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let active = open == Some(PanelKind::Island);
    let id = o.id;
    let time = label_w(
        app.now.format("%H:%M").to_string(),
        text::BODY,
        if active { s.accent } else { s.ink },
        theme::SEMIBOLD,
    );
    let date = label(app.now.format("%a %-d").to_string(), text::SMALL, s.ink_dim);
    let mut content = row![].spacing(10).align_y(Alignment::Center);
    match activity(app) {
        Activity::Game(g) => {
            content = content.push(
                row![
                    icon(Icon::Trophy, 14.0, s.accent),
                    label(
                        format!("{} {}–{} {}", g.away, g.away_score, g.home_score, g.home),
                        text::SMALL,
                        s.ink
                    ),
                    label(g.detail.clone(), text::CAPTION, s.ink_dim),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        Activity::Music(p) => {
            content = content.push(
                row![
                    media::art(s, p, 18.0, 5.0),
                    label(ui::fit(&p.title, text::SMALL, 220.0), text::SMALL, s.ink),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        Activity::None => {}
    }
    content = content.push(row![time, date].spacing(8).align_y(Alignment::Center));
    if let Some(w) = &app.weather {
        content = content.push(
            row![
                icon(ui::weather_icon(w), 14.0, s.ink_dim),
                label(format!("{:.0}°", w.temp), text::SMALL, s.ink_dim),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    let inner = ui::morph::morph(content, o.island_w.value, size::CONTROL)
        .on_measure(move |w| Message::IslandMeasured(id, w));
    ui::control(inner)
        .padding([0, 12])
        .height(size::CONTROL)
        .radius(size::CONTROL / 2.0)
        .colors(
            if active { s.accent_soft } else { s.raised },
            if active { s.accent_soft } else { s.raised_hi },
            s.raised_press,
        )
        .on_press_at(move |r| Message::Toggle(PanelKind::Island, id, r))
        .into()
}
