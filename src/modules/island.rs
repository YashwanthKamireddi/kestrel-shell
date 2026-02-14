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

/// The island: the time pill. It always shows the time and date; music (unless it has its
/// own bubble), the visualiser while music plays, and the weather (unless it has its own
/// circle) join it, and pop-ups take it over for a moment.
pub fn items<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    vec![bar(app, o, s, open)]
}

/// Five accent bars moving with the music, while it plays (never in game mode).
pub fn visualiser<'a>(app: &App, s: Scheme) -> Option<Element<'a, Message>> {
    if crate::motion::still() || !app.media.main().is_some_and(|p| p.playing) {
        return None;
    }
    // Heights on whole pixels: a bar ending between pixels looks soft.
    let bars = app.levels.iter().map(|v| {
        let h = (4.0 + v.clamp(0.0, 1.0) * 12.0).round();
        iced::widget::container(iced::widget::Space::new())
            .width(3)
            .height(h)
            .style(move |_| iced::widget::container::Style {
                background: Some(s.accent.into()),
                border: iced::Border::default().rounded(1.5),
                ..iced::widget::container::Style::default()
            })
            .into()
    });
    Some(
        iced::widget::row(bars)
            .spacing(2)
            .height(16)
            .align_y(Alignment::Center)
            .into(),
    )
}

/// Diameter of the music bubble: the same height as the pills, so they line up.
pub const BUBBLE: f32 = size::BAR - 6.0;

/// The music bubble: a black pill circle with the cover inside, dimmed while paused. Opens the
/// media panel. None when there is no player.
pub fn bubble<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Option<Element<'a, Message>> {
    // Stays while paused: the bubble is the player, not just the sound.
    let p = app.media.main()?;
    let id = o.id;
    let art = BUBBLE - 8.0;
    Some(
        ui::control(
            iced::widget::container(media::art(
                s.faded(if p.playing { 1.0 } else { 0.5 }),
                p,
                art,
                art / 2.0,
            ))
            .center(BUBBLE)
            .style(move |_| iced::widget::container::Style {
                background: Some(s.base.into()),
                border: iced::Border::default().rounded(BUBBLE / 2.0),
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
    // A notification, while it is fresh: who, what, and the start of it. A click opens the
    // notification centre.
    if hosts_osd(app)
        && let Some(n) = app
            .toasts
            .last()
            .and_then(|t| app.notifications.iter().find(|n| n.id == t.id))
    {
        let lead: Element<'a, Message> = match crate::services::icons::lookup(&n.app_icon) {
            Some(p) => ui::image_file(&p, 18.0),
            None => icon(ui::Icon::BellRing, 16.0, s.accent).into(),
        };
        let mut words = row![label_w(
            ui::fit(&n.summary, text::SMALL, 220.0),
            text::SMALL,
            s.ink,
            theme::SEMIBOLD
        )]
        .spacing(8)
        .align_y(Alignment::Center);
        let body = n.body.split_whitespace().collect::<Vec<_>>().join(" ");
        if !body.is_empty() {
            words = words.push(label(
                ui::fit(&body, text::SMALL, 260.0),
                text::SMALL,
                s.ink_dim,
            ));
        }
        content = content.push(row![lead, words].spacing(10).align_y(Alignment::Center));
        return finish_to(content, o, s, active, PanelKind::Control);
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
    // Music without a bubble of its own: a small round cover and the title, before the time.
    if !app.config.bar.contains("media")
        && let Some(p) = app.media.main()
    {
        content = content.push(
            row![
                media::art(s.faded(if p.playing { 1.0 } else { 0.5 }), p, 22.0, 11.0),
                label_w(
                    ui::fit(&p.title, text::SMALL, 170.0),
                    text::SMALL,
                    if p.playing { s.ink } else { s.ink_dim },
                    theme::MEDIUM
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    content = content.push(row![time, date].spacing(8).align_y(Alignment::Center));
    // While music plays: the visualiser, inside the time pill.
    if let Some(v) = visualiser(app, s) {
        content = content.push(v);
    }
    if !app.config.bar.contains("weather")
        && let Some(w) = &app.weather
    {
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
    finish_to(content, o, s, active, PanelKind::Island)
}

/// `finish`, with a click opening `kind` (a notification in the island opens the centre).
fn finish_to<'a>(
    content: iced::widget::Row<'a, Message>,
    o: &'a Output,
    s: Scheme,
    active: bool,
    kind: PanelKind,
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
            // Black like the other pills; it lifts on hover and while its panel is open.
            if active { s.raised_press } else { s.base },
            if active { s.raised_press } else { s.raised },
            s.raised_press,
        )
        .on_press_at(move |r| Message::Toggle(kind, id, r))
        .into()
}
