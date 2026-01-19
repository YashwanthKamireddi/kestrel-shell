//! The island: the bar's centre pill. It always shows the time; when something is live it grows
//! to show that too (a game in progress, music playing), springing to its new width. A click
//! opens the dashboard: today, weather, what's playing, games, system, and the month.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::panels::media;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, icon, label, label_w};
use iced::widget::row;
use iced::{Alignment, Element};

/// Whether volume, brightness and other pop-ups show inside the island (pill style with the
/// island in the bar) rather than as a separate drop.
pub fn hosts_osd(app: &App) -> bool {
    app.config.bar.style == crate::config::BarStyle::Pills && app.config.bar.contains("island")
}

/// The island. What is playing floats beside it as its own bubble (`bubble`), drawn by the
/// bar outside this row so the island stays centred.
pub fn items<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    vec![bar(app, o, s, open)]
}

/// Diameter of the music bubble.
pub const BUBBLE: f32 = 30.0;

/// The music bubble: the cover in a circle, ringed in the accent while it plays and quietly
/// while paused. Opens the media panel. None when there is no player.
pub fn bubble<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Option<Element<'a, Message>> {
    // Stays while paused: the bubble is the player, not just the sound.
    let p = app.media.main()?;
    let id = o.id;
    let open = app
        .panel
        .as_ref()
        .is_some_and(|q| q.open && q.kind == PanelKind::Media);
    let ring = if open {
        s.ink
    } else if p.playing {
        s.accent
    } else {
        s.ink_faint
    };
    Some(
        ui::control(
            iced::widget::container(media::art(s, p, BUBBLE - 6.0, (BUBBLE - 6.0) / 2.0))
                .center(BUBBLE)
                .style(move |_| iced::widget::container::Style {
                    background: Some(s.base.into()),
                    border: iced::Border {
                        color: ring,
                        width: 2.0,
                        radius: (BUBBLE / 2.0).into(),
                    },
                    ..iced::widget::container::Style::default()
                }),
        )
        .width(BUBBLE)
        .height(BUBBLE)
        .radius(BUBBLE / 2.0)
        .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r))
        .into(),
    )
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
        s.ink,
        theme::SEMIBOLD,
    );
    let date = label(app.now.format("%a %-d").to_string(), text::SMALL, s.ink);
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
    content = content.push(row![time, date].spacing(8).align_y(Alignment::Center));
    if let Some(w) = &app.weather {
        content = content.push(
            row![
                icon(ui::weather_icon(w), 15.0, ui::weather_tint(w)),
                label_w(
                    format!("{:.0}°", w.temp),
                    text::SMALL,
                    ui::temperature_tint(w.temp),
                    theme::MEDIUM
                ),
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
    // As tall as a pill, so in the pill style it is the pill.
    let h = size::BAR - 6.0;
    ui::control(inner)
        .padding([0, 14])
        .height(h)
        .radius(h / 2.0)
        .colors(
            if active { s.raised_press } else { s.raised },
            if active { s.raised_press } else { s.raised_hi },
            s.raised_press,
        )
        .on_press_at(move |r| Message::Toggle(PanelKind::Island, id, r))
        .into()
}
