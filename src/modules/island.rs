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

/// Whether volume, brightness and other pop-ups show inside the island (pill style with the
/// island in the bar) rather than as a separate drop.
pub fn hosts_osd(app: &App) -> bool {
    app.config.bar.style == crate::config::BarStyle::Pills && app.config.bar.contains("island")
}

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

/// The island, with what is playing as its own pill beside it.
pub fn items<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let mut v = Vec::new();
    if let Activity::Music(p) = activity(app) {
        v.push(media_pill(p, o, s, open == Some(PanelKind::Media)));
    }
    v.push(bar(app, o, s, open));
    v
}

/// Now playing: artwork, title, and the play state; opens the media panel.
fn media_pill<'a>(
    p: &'a crate::services::mpris::Player,
    o: &'a Output,
    s: Scheme,
    active: bool,
) -> Element<'a, Message> {
    let id = o.id;
    ui::control(
        row![
            media::art(s, p, 20.0, 5.0),
            label(ui::fit(&p.title, text::SMALL, 200.0), text::SMALL, s.ink),
            icon(Icon::AudioLines, 14.0, s.accent),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 10])
    .height(size::CONTROL)
    .radius(size::CONTROL / 2.0)
    .colors(
        if active { s.accent_soft } else { s.raised },
        if active { s.accent_soft } else { s.raised_hi },
        s.raised_press,
    )
    .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r))
    .into()
}

pub fn bar<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let active = open == Some(PanelKind::Island);
    let time = label_w(
        app.now.format("%H:%M").to_string(),
        text::BODY,
        if active { s.accent } else { s.ink },
        theme::SEMIBOLD,
    );
    let date = label(app.now.format("%a %-d").to_string(), text::SMALL, s.ink_dim);
    let mut content = row![].spacing(10).align_y(Alignment::Center);
    // A pop-up (volume, brightness, a device) takes the island while it lasts.
    if hosts_osd(app)
        && let Some(osd) = app
            .osd
            .as_ref()
            .filter(|o| o.until > std::time::Instant::now())
    {
        content = content.push(osd_row(osd, s));
        return finish(content, o, s, active);
    }
    // A flash (something copied) takes the whole island for a moment.
    if let Some(f) = &app.flash {
        let lead: Element<'a, Message> = match &f.thumb {
            Some(h) => ui::cover(h.clone(), 34.0, 20.0, 4.0),
            None => icon(f.icon, 14.0, s.accent).into(),
        };
        let mut words = row![label_w(
            f.title.clone(),
            text::SMALL,
            s.ink,
            theme::SEMIBOLD
        )]
        .spacing(8)
        .align_y(Alignment::Center);
        if !f.detail.is_empty() {
            words = words.push(label(
                ui::fit(&f.detail, text::SMALL, 240.0),
                text::SMALL,
                s.ink_dim,
            ));
        }
        content = content.push(row![lead, words].spacing(8).align_y(Alignment::Center));
        return finish(content, o, s, active);
    }
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
        // Music has its own pill beside the island (see `items`).
        Activity::Music(_) | Activity::None => {}
    }
    content = content.push(row![time, date].spacing(8).align_y(Alignment::Center));
    if let Some(w) = &app.weather {
        content = content.push(
            row![
                icon(ui::weather_icon(w), 15.0, ui::weather_tint(w)),
                label(format!("{:.0}°", w.temp), text::SMALL, s.ink),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    finish(content, o, s, active)
}

/// A level (icon, bar, value) or an event (icon, title, line under it), sized for the island.
fn osd_row<'a>(osd: &crate::app::Osd, s: Scheme) -> Element<'a, Message> {
    if let Some(title) = &osd.text {
        let mut r = row![
            icon(osd.icon, 15.0, s.accent),
            label_w(
                ui::fit(title, text::SMALL, 200.0),
                text::SMALL,
                s.ink,
                theme::SEMIBOLD
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if let Some(sub) = &osd.sub {
            r = r.push(label(
                ui::fit(sub, text::SMALL, 160.0),
                text::SMALL,
                s.ink_dim,
            ));
        }
        return r.into();
    }
    const TRACK: f32 = 150.0;
    let bar = |w: f32, c: iced::Color| {
        iced::widget::container(iced::widget::Space::new())
            .width(w)
            .height(5)
            .style(move |_| iced::widget::container::Style {
                background: Some(c.into()),
                border: iced::Border::default().rounded(3),
                ..iced::widget::container::Style::default()
            })
    };
    let level = osd.value.clamp(0.0, 1.0);
    row![
        icon(
            osd.icon,
            15.0,
            if level > 0.0 { s.accent } else { s.ink_dim }
        ),
        iced::widget::stack![
            bar(TRACK, s.raised_hi),
            bar(
                (TRACK * level).max(5.0),
                if level > 0.0 { s.accent } else { s.ink_faint }
            ),
        ],
        label_w(
            format!("{:.0}", level * 100.0),
            text::SMALL,
            s.ink,
            theme::SEMIBOLD
        )
        .width(26)
        .align_x(Alignment::End),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// The island's pill around whatever it shows, springing to its width.
fn finish<'a>(
    content: iced::widget::Row<'a, Message>,
    o: &'a Output,
    s: Scheme,
    active: bool,
) -> Element<'a, Message> {
    let id = o.id;
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
