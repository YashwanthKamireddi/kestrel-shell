//! The island: the bar's centre pill. It always shows the time; when something is live it grows
//! to show that too (a game in progress, music playing), springing to its new width. A click
//! opens the dashboard: today, weather, what's playing, games, system, and the month.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::panels::media;
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, icon, label, label_w};
use iced::widget::row;
use iced::{Alignment, Element};

/// The island: the time pill. It always shows the time and date; music (unless it has its
/// own module), the visualiser while music plays, and the weather (unless it has its own
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
    // each bar into a solid stripe and a faint one beside it (a "line" in every bar, the user's
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

/// One thing the island can show: what it is, how much it matters (the highest active one
/// shows), whether it is happening, and its face. Adding one is a function and a line in
/// `ACTIVITIES` (p3dro's `IslandRegistry`: the old way was a chain of ifs).
struct Activity {
    #[allow(dead_code)]
    id: &'static str,
    priority: u8,
    active: fn(&App) -> bool,
    face: for<'a> fn(&'a App, &'a Output, Scheme, bool) -> Element<'a, Message>,
}

/// Everything the island can show, the time last (always active).
const ACTIVITIES: &[Activity] = &[
    Activity {
        id: "osd",
        priority: 30,
        active: |app| {
            crate::policy::island_owns_osd(app)
                && app
                    .osd
                    .as_ref()
                    .is_some_and(|o| o.until > std::time::Instant::now())
        },
        face: osd_face,
    },
    Activity {
        id: "notification",
        priority: 25,
        active: |app| crate::policy::island_owns_notifications(app) && !app.toasts.is_empty(),
        face: notification_face,
    },
    Activity {
        id: "flash",
        priority: 20,
        active: |app| app.flash.is_some(),
        face: flash_face,
    },
    Activity {
        id: "time",
        priority: 0,
        active: |_| true,
        face: time_face,
    },
];

pub fn bar<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let active = open == Some(PanelKind::Island);
    let now = ACTIVITIES
        .iter()
        .filter(|a| (a.active)(app))
        .max_by_key(|a| a.priority)
        .unwrap_or(&ACTIVITIES[ACTIVITIES.len() - 1]);
    (now.face)(app, o, s, active)
}

/// A level (volume, brightness) or a device event, while it lasts; a click acts on it.
fn osd_face<'a>(app: &'a App, o: &'a Output, s: Scheme, active: bool) -> Element<'a, Message> {
    let mut content = row![].spacing(tok::L).align_y(Alignment::Center);
    let Some(osd) = app.osd.as_ref() else {
        return time_face(app, o, s, active);
    };
    content = content.push(osd_row(
        osd,
        s,
        (app.config.sound.max as f32 / 100.0).max(1.0),
    ));
    match osd.action.clone() {
        Some(a) => finish_with(content, o, s, active, a),
        None => finish(content, o, s, active),
    }
}

/// What just came in: the app's icon, the title and a line of the message; a click drops its
/// full card under the island (`panels::notice`).
fn notification_face<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    active: bool,
) -> Element<'a, Message> {
    let Some(n) = app
        .toasts
        .last()
        .and_then(|t| app.notifications.iter().find(|n| n.id == t.id))
    else {
        return time_face(app, o, s, active);
    };
    let title = if n.summary.trim().is_empty() {
        n.app.clone()
    } else {
        n.summary.trim().to_string()
    };
    let body: String = n.body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut content = row![
        ui::app_icon(s, &n.app, 18.0, ui::Icon::Bell),
        label_w(
            ui::fit(&title, text::SMALL, 200.0),
            text::SMALL,
            s.ink,
            theme::SEMIBOLD
        ),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    if !body.is_empty() {
        content = content.push(label(
            ui::fit(&body, text::SMALL, 220.0),
            text::SMALL,
            s.ink_dim,
        ));
    }
    let more = app.toasts.len().saturating_sub(1);
    if more > 0 {
        content = content.push(label(format!("+{more}"), text::SMALL, s.ink_dim));
    }
    finish_to(content, o, s, active || open_notice(app), PanelKind::Notice)
}

fn open_notice(app: &App) -> bool {
    app.panel
        .as_ref()
        .is_some_and(|p| p.open && p.kind == PanelKind::Notice)
}

/// Something copied, a screenshot, a recording saved: for a moment; a click opens it.
fn flash_face<'a>(app: &'a App, o: &'a Output, s: Scheme, active: bool) -> Element<'a, Message> {
    let mut content = row![].spacing(tok::L).align_y(Alignment::Center);
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
    time_face(app, o, s, active)
}

/// The resting face: the time and date, with music, its visualiser and the weather when
/// they have no module of their own.
fn time_face<'a>(app: &'a App, o: &'a Output, s: Scheme, active: bool) -> Element<'a, Message> {
    let time = label_w(app.time_text(), text::BODY, s.ink, theme::SEMIBOLD);
    let date = label(app.now.format("%a %-d").to_string(), text::SMALL, s.ink);
    let mut content = row![].spacing(tok::L).align_y(Alignment::Center);
    // Music without a module of its own: a small round cover and the title, before the time.
    // The song is its own target: a click on it opens the player; anywhere else, the day.
    if crate::policy::island_shows(app, "media")
        && let Some(p) = app.media.main().filter(|p| !media::is_bare_browser(p))
    {
        let id = o.id;
        content = content.push(
            ui::control(
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
            )
            .kind(s, ui::control::Kind::Bare)
            .no_press_scale()
            .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r)),
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
    if crate::policy::island_shows(app, "weather")
        && let Some(w) = &app.weather
    {
        content = content.push(
            row![
                icon(ui::weather_icon(w), 15.0, ui::weather_tint(w)),
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

/// A level (icon, bar, value) or an event (icon, title, line under it), sized for the island.
fn osd_row<'a>(osd: &crate::app::Osd, s: Scheme, top_volume: f32) -> Element<'a, Message> {
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
    // A level that can be set is a slider in the island: drag it where you see it (Caelestia's
    // OSD is a control, not a picture). Anything else (keyboard layout) is a plain bar.
    let settable = crate::shell::bar::is_volume(osd.icon)
        || matches!(osd.icon, ui::Icon::Mic | ui::Icon::MicOff | ui::Icon::Sun);
    let track: Element<'a, Message> = if settable {
        // The plain 0–100 % track until boosted, then up to the maximum with the boost red.
        let span = if level > 1.0 { top.max(1.0) } else { 1.0 };
        iced::widget::container(
            ui::slider::slider(
                iced::widget::Space::new(),
                (level / span).clamp(0.0, 1.0),
                14.0,
                (s.ink.scale_alpha(tok::FILL * s.opacity), s.accent),
                move |v| Message::OsdSet(v * span),
            )
            .boost(1.0 / span, s.bad),
        )
        .width(TRACK)
        .into()
    } else {
        ui::level_bar(s, level, top, TRACK, 5.0)
    };
    row![
        icon(
            osd.icon,
            15.0,
            if level > 0.0 { s.accent } else { s.ink_dim }
        ),
        track,
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
    let inner = ui::morph::morph(content, o.island_w.value, size::CONTROL)
        .on_measure(move |w| Message::IslandMeasured(id, w));
    let h = size::CONTROL;
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
    let inner = ui::morph::morph(content, o.island_w.value, size::CONTROL)
        .on_measure(move |w| Message::IslandMeasured(id, w));
    // As tall as a pill, so in the pill style it is the pill.
    let h = size::CONTROL;
    ui::control(inner)
        .padding([tok::NONE, tok::L])
        .height(h)
        .radius(h / 2.0)
        // The same states as every other pill.
        .bar(s, active)
        .on_press_at(move |r| Message::Toggle(kind, id, r))
        .into()
}
