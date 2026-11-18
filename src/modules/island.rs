//! The island: the bar's centre pill. It always shows the time; when something is live it grows
//! to show that too (a game in progress, music playing), springing to its new width. A click
//! opens the dashboard: today, weather, what's playing, games, system, and the month.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::panels::media;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
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
/// The module's view (the island's items).
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    items(app, o, s, open)
}

pub fn items<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    vec![bar(app, o, s, open)]
}

/// The now-playing bars: four rounded bars in the artwork's colour, growing from their middle
/// with the music (never in game mode).
pub fn visualiser<'a>(app: &App, s: Scheme) -> Option<Element<'a, Message>> {
    let p = app.media.main()?;
    if crate::motion::still() || !p.playing || !app.config.bar.visualiser {
        return None;
    }
    // The artwork's colour, made bright enough to read on black; else the accent.
    let tint = media::art_colour(p).map_or(s.accent, |c| {
        let mut o = crate::theme::color::Oklch::from_srgb(c.r, c.g, c.b);
        o.l = o.l.clamp(0.72, 0.86);
        o.c = o.c.max(0.08);
        o.to_color()
    });
    // Whole pixels only: 3.5 px bars with 2.5 px gaps sat on half pixels, and smoothing split
    // each bar into a solid stripe and a faint one beside it (a "line" in every bar, the
    // report 2026-11-14). Even heights keep the centred bars on whole pixels too.
    let bars = app.levels.iter().map(|v| {
        let h = (2.0 * (2.0 + v.clamp(0.0, 1.0) * 6.5)).round();
        iced::widget::container(iced::widget::Space::new())
            .width(4)
            .height(h)
            .style(move |_| iced::widget::container::Style {
                background: Some(tint.into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..iced::widget::container::Style::default()
            })
            .into()
    });
    Some(
        ui::snap::snap(
            iced::widget::row(bars)
                .spacing(tok::XS)
                .height(18)
                .align_y(Alignment::Center),
        )
        .into(),
    )
}

/// What a round bubble's content leaves of its circle (the cover, the AirPods ring).
pub const BUBBLE_INSET: f32 = 6.0;

/// Diameter of the music bubble: the same height as the pills, so they line up.
pub const BUBBLE: f32 = tok::BAR - 6.0;

/// The music bubble: a black pill circle with the cover inside, dimmed while paused. Opens the
/// media panel. None when there is no player.
pub fn bubble<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Option<Element<'a, Message>> {
    // Stays while paused: the bubble is the player, not just the sound. A browser playing
    // something with no picture (a hover preview) gets none.
    let p = app.media.main().filter(|p| !media::is_bare_browser(p))?;
    let id = o.id;
    // The same inset as the AirPods circle's ring: the two bubbles are one design.
    let art = BUBBLE - BUBBLE_INSET;
    // Snapped as one piece: the circle and the cover land on the same pixels, so the cover
    // sits exactly in the middle.
    Some(
        ui::snap::snap(
            ui::control(
                iced::widget::container(media::art_plain(
                    s.faded(if p.playing { 1.0 } else { 0.5 }),
                    p,
                    art,
                    art / 2.0,
                ))
                .center(BUBBLE),
            )
            .width(BUBBLE)
            .height(BUBBLE)
            .radius(BUBBLE / 2.0)
            // The pills' own surface and states (it had a darker fill of its own).
            .bar(s, false)
            .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r)),
        )
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
    let time = label_w(app.time_text(), text::BODY, s.ink, theme::SEMIBOLD);
    let date = label(app.now.format("%a %-d").to_string(), text::SMALL, s.ink);
    let mut content = row![].spacing(tok::L).align_y(Alignment::Center);
    // A pop-up (volume, brightness, a device) takes the island while it lasts.
    if hosts_osd(app)
        && let Some(osd) = app
            .osd
            .as_ref()
            .filter(|o| o.until > std::time::Instant::now())
    {
        content = content.push(osd_row(
            osd,
            s,
            (app.config.sound.max as f32 / 100.0).max(1.0),
        ));
        // A click acts on what it shows (the level's controls, say).
        return match osd.action.clone() {
            Some(a) => finish_with(content, o, s, active, a),
            None => finish(content, o, s, active),
        };
    }
    // A flash (something copied) takes the whole island for a moment.
    if let Some(f) = &app.flash {
        let lead: Element<'a, Message> = match &f.thumb {
            Some(h) => ui::cover(h.clone(), 34.0, 20.0, 4.0),
            None => icon(f.icon, tok::ICON_S, s.accent).into(),
        };
        let mut words = row![label_w(
            f.title.clone(),
            text::SMALL,
            s.ink,
            theme::SEMIBOLD
        )]
        .spacing(tok::M)
        .align_y(Alignment::Center);
        if !f.detail.is_empty() {
            words = words.push(label(
                ui::fit(&f.detail, text::SMALL, 240.0),
                text::SMALL,
                s.ink_dim,
            ));
        }
        content = content.push(row![lead, words].spacing(tok::M).align_y(Alignment::Center));
        // A click acts on it: opens the screenshot or recording, the clipboard, stops a
        // recording.
        return match f.action.clone() {
            Some(a) => finish_with(content, o, s, active, a),
            None => finish(content, o, s, active),
        };
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
            .spacing(tok::M)
            .align_y(Alignment::Center),
        );
    }
    content = content.push(if app.config.clock.date {
        row![time, date].spacing(tok::M).align_y(Alignment::Center)
    } else {
        row![time].align_y(Alignment::Center)
    });
    // While music plays: the visualiser, inside the time pill.
    if let Some(v) = visualiser(app, s) {
        content = content.push(v);
    }
    // Notifications live in the island: their count at its end (accent while unseen, a quiet
    // bell once seen, crossed out in Do not disturb); a click opens them under the island, a
    // right click turns Do not disturb on or off. Nothing when there are none.
    if let Some(n) = notifications_mark(app, o, s, open) {
        content = content.push(n);
    }
    if !app.config.bar.contains("weather")
        && let Some(w) = &app.weather
    {
        content = content.push(
            row![
                icon(ui::weather_icon(w), tok::ICON, ui::weather_tint(w)),
                label_w(
                    app.temp(w.temp),
                    text::SMALL,
                    ui::temperature_tint(w.temp),
                    theme::MEDIUM
                ),
            ]
            .spacing(tok::XS)
            .align_y(Alignment::Center),
        );
    }
    finish(content, o, s, active)
}

/// The island's notifications mark (see `bar`).
fn notifications_mark<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    _open: Option<PanelKind>,
) -> Option<Element<'a, Message>> {
    if app.notifications.is_empty() && !app.dnd {
        return None;
    }
    let id = o.id;
    let w = o.size.width;
    // One mark, always the same size (it changed shape as they were read, and the island
    // jumped under the click): the number in a small round badge, the accent while unread,
    // quiet once seen; a crossed bell in Do not disturb.
    let content: Element<'a, Message> = if app.dnd {
        icon(crate::ui::Icon::BellOff, tok::ICON_S, s.ink_dim).into()
    } else {
        let n = app.notifications.len();
        let unread = app.unseen > 0;
        let (bg, fg) = if unread {
            (s.accent, s.on_accent)
        } else {
            (s.ink.scale_alpha(tok::FILL_STRONG * s.opacity), s.ink_dim)
        };
        const BADGE: f32 = 20.0;
        iced::widget::container(label_w(
            if n > 9 {
                "9+".to_string()
            } else {
                n.to_string()
            },
            text::CAPTION,
            fg,
            theme::SEMIBOLD,
        ))
        // A fixed circle: a new number never changes its size, so the island does not move
        // when one arrives (it jiggled as 3 became 4, 9 became 9+).
        .center(BADGE)
        .style(move |_| iced::widget::container::Style {
            background: Some(bg.into()),
            border: iced::Border::default().rounded(tok::FULL),
            ..iced::widget::container::Style::default()
        })
        .into()
    };
    Some(
        // Part of the island, not a pill on it: no fill or hover of its own.
        // A soft press: it dips under the click (no fill flashing behind it, which read as a
        // glitch; nothing at all felt dead).
        ui::control(content)
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Still)
            .press_scale()
            .on_press(Message::Toggle(
                PanelKind::Notifications,
                id,
                iced::Rectangle::new(
                    iced::Point::new(w / 2.0, 0.0),
                    iced::Size::new(0.0, tok::BAR),
                ),
            ))
            .on_right_press(Message::Change(crate::app::Change::Dnd))
            .into(),
    )
}

/// A level (icon, bar, value) or an event (icon, title, line under it), sized for the island.
fn osd_row<'a>(osd: &crate::app::Osd, s: Scheme, top_volume: f32) -> Element<'a, Message> {
    if let Some(title) = &osd.text {
        let mut r = row![
            icon(osd.icon, tok::ICON, s.accent),
            label_w(
                ui::fit(title, text::SMALL, 200.0),
                text::SMALL,
                s.ink,
                theme::SEMIBOLD
            ),
        ]
        .spacing(tok::M)
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
    let top = if crate::shell::bar::is_volume(osd.icon) {
        top_volume
    } else {
        1.0
    };
    let level = osd.value.max(0.0);
    row![
        icon(
            osd.icon,
            15.0,
            if level > 0.0 { s.accent } else { s.ink_dim }
        ),
        ui::level_bar(s, level, top, TRACK, 16.0),
        label_w(
            format!("{:.0}", level * 100.0),
            text::SMALL,
            s.ink,
            theme::SEMIBOLD
        )
        .width(26)
        .align_x(Alignment::End),
    ]
    .spacing(tok::L)
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

/// `finish`, with a click doing `action` instead of opening a panel.
fn finish_with<'a>(
    content: iced::widget::Row<'a, Message>,
    o: &'a Output,
    s: Scheme,
    active: bool,
    action: Message,
) -> Element<'a, Message> {
    let id = o.id;
    let inner = ui::morph::morph(content, o.island_w.value, tok::BAR_ITEM)
        .on_measure(move |w| Message::IslandMeasured(id, w));
    let h = tok::BAR - 6.0;
    ui::control(inner)
        .padding([tok::NONE, tok::L])
        .height(h)
        .radius(h / 2.0)
        .bar(s, active)
        .on_press(action)
        .into()
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
    let inner = ui::morph::morph(content, o.island_w.value, tok::BAR_ITEM)
        .on_measure(move |w| Message::IslandMeasured(id, w));
    // As tall as a pill, so in the pill style it is the pill.
    let h = tok::BAR - 6.0;
    ui::control(inner)
        .padding([tok::NONE, tok::L])
        .height(h)
        .radius(h / 2.0)
        // The same states as every other pill.
        .bar(s, active)
        .on_press_at(move |r| Message::Toggle(kind, id, r))
        .into()
}
