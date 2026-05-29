//! Settings › displays: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A display's size on the desktop (after scale and rotation).
pub(super) fn logical(d: &crate::services::displays::Display) -> (i32, i32) {
    let (w, h) = (
        (d.width as f32 / d.scale).round() as i32,
        (d.height as f32 / d.scale).round() as i32,
    );
    if d.transform % 2 == 1 { (h, w) } else { (w, h) }
}

/// Where the second display is now, next to the first.
pub(super) fn place_of(all: &[crate::services::displays::Display]) -> Place {
    let (Some(a), Some(b)) = (all.first(), all.get(1)) else {
        return Place::Right;
    };
    let (aw, ah) = logical(a);
    if b.x >= a.x + aw {
        Place::Right
    } else if b.y >= a.y + ah {
        Place::Below
    } else if b.y < a.y && b.x < a.x + aw && b.x + logical(b).0 > a.x {
        Place::Above
    } else {
        Place::Left
    }
}

/// Puts the second display on `place` of the first, touching, the first at the origin.
pub(super) fn arrange(all: &mut [crate::services::displays::Display], place: Place) {
    if all.len() < 2 {
        return;
    }
    let (aw, ah) = logical(&all[0]);
    let (bw, bh) = logical(&all[1]);
    let (ax, ay, bx, by) = match place {
        Place::Right => (0, 0, aw, 0),
        Place::Left => (bw, 0, 0, 0),
        Place::Below => (0, 0, 0, ah),
        Place::Above => (0, bh, 0, 0),
    };
    all[0].x = ax;
    all[0].y = ay;
    all[1].x = bx;
    all[1].y = by;
}

pub(super) fn change_display(app: &mut App, i: usize, change: DisplayChange) -> Task<AppMessage> {
    let st = &mut app.panels.settings;
    if i >= st.displays.len() {
        return Task::none();
    }
    // The last screen that is on stays on.
    if matches!(change, DisplayChange::On(false))
        && st.displays.iter().filter(|d| d.enabled).count() <= 1
    {
        return Task::none();
    }
    let place = place_of(&st.displays);
    st.mode_open = None;
    let d = &mut st.displays[i];
    let mut new_place = place;
    match change {
        DisplayChange::On(on) => d.enabled = on,
        DisplayChange::Mode(w, h, hz) => {
            d.width = w;
            d.height = h;
            d.refresh = hz;
        }
        DisplayChange::Scale(sc) => d.scale = sc,
        DisplayChange::Rotate(t) => d.transform = t,
        DisplayChange::Place(p) => new_place = p,
        DisplayChange::Mirror(m) => {
            if m.is_some() {
                d.enabled = true;
            } else if d.mirror.is_some() {
                // While mirrored both screens sit in one place, which says nothing about where
                // this one was: it comes back on the right, as a new screen would.
                new_place = Place::Right;
            }
            d.mirror = m;
        }
    }
    // Two screens, both on, side by side: kept touching in their order after any change of
    // size. One on (the other off), a mirror, or more than two: positions stay as they are.
    if st.displays.len() == 2 && st.displays.iter().all(|d| d.enabled && d.mirror.is_none()) {
        arrange(&mut st.displays, new_place);
    }
    // Applied now; kept only when confirmed, else it all goes back by itself.
    let n = st.pending.map_or(1, |(n, _)| n + 1);
    st.pending = Some((n, std::time::Instant::now()));
    let all = st.displays.clone();
    Task::batch([
        Task::future(async move { crate::services::displays::apply(&all).await }).discard(),
        Task::perform(
            tokio::time::sleep(std::time::Duration::from_secs(KEEP_SECS)),
            move |()| msg(Message::DisplayTimeout(n)),
        ),
    ])
}

pub(super) fn revert_displays(app: &mut App) -> Task<AppMessage> {
    let st = &mut app.panels.settings;
    st.pending = None;
    st.displays = st.kept.clone();
    let all = st.kept.clone();
    Task::future(async move { crate::services::displays::apply(&all).await }).discard()
}

pub(super) fn displays_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    if let Some(left) = display_countdown(st) {
        out.push(
            container(
                row![
                    column![
                        label_w(
                            "Keep these display settings?",
                            text::BODY,
                            s.ink,
                            theme::SEMIBOLD
                        ),
                        label(format!("Going back in {left} s"), text::SMALL, s.ink_dim),
                    ]
                    .spacing(tok::XXS)
                    .width(Length::Fill),
                    control(label_w("Go back", text::SMALL, s.ink, theme::MEDIUM))
                        .pill(s, crate::ui::control::Pill::Plain)
                        .on_press(msg(Message::DisplayRevert)),
                    control(label_w("Keep", text::SMALL, s.on_accent, theme::SEMIBOLD))
                        .pill(s, crate::ui::control::Pill::Primary)
                        .on_press(msg(Message::DisplayKeep)),
                ]
                .spacing(tok::M)
                .align_y(Alignment::Center),
            )
            .padding([tok::L, tok::XL])
            .style(move |_| container::Style {
                background: Some(s.accent.scale_alpha(0.14 * s.opacity).into()),
                border: iced::Border::default().rounded(size::CARD_RADIUS),
                ..container::Style::default()
            })
            .into(),
        );
    }
    let on_count = st.displays.iter().filter(|d| d.enabled).count();
    for (i, d) in st.displays.iter().enumerate() {
        let mut rows = vec![row_of(
            s,
            "Use this display",
            None,
            if d.enabled && on_count <= 1 {
                // The only one on: it cannot be turned off.
                ui::switch(s, true, msg(Message::DisplayTick))
            } else {
                ui::switch(
                    s,
                    d.enabled,
                    msg(Message::Display(i, DisplayChange::On(!d.enabled))),
                )
            },
        )];
        if d.enabled {
            let open = st.mode_open == Some(i);
            rows.push(row_of(
                s,
                "Resolution",
                None,
                control(
                    row![
                        label(format!("{} × {}", d.width, d.height), text::SMALL, s.ink),
                        icon(Icon::ChevronDown, 14.0, s.ink_dim)
                    ]
                    .spacing(tok::S)
                    .align_y(Alignment::Center),
                )
                .padding([tok::XS, tok::L])
                .radius(14.0)
                .kind(s, crate::ui::control::Kind::Ghost)
                .on_press(msg(Message::ModeOpen((!open).then_some(i))))
                .into(),
            ));
            if open {
                for (w, h) in d.sizes() {
                    let on = w == d.width && h == d.height;
                    // A new size keeps the refresh rate when it has it, else its highest.
                    let rates = d.rates(w, h);
                    let hz = rates
                        .iter()
                        .copied()
                        .find(|r| (r - d.refresh).abs() < 0.5)
                        .or_else(|| rates.first().copied())
                        .unwrap_or(d.refresh);
                    rows.push(Some(
                        control(
                            row![
                                label(
                                    format!("{w} × {h}"),
                                    text::BODY,
                                    if on { s.accent } else { s.ink }
                                )
                                .width(Length::Fill),
                                if on {
                                    Element::from(icon(Icon::Check, 16.0, s.accent))
                                } else {
                                    iced::widget::Space::new().into()
                                }
                            ]
                            .align_y(Alignment::Center),
                        )
                        .padding([0, 28])
                        .height(34)
                        .width(Length::Fill)
                        .kind(s, crate::ui::control::Kind::Ghost)
                        .no_press_scale()
                        .on_press(msg(Message::Display(i, DisplayChange::Mode(w, h, hz))))
                        .into(),
                    ));
                }
            }
            // Refresh rates at this size, when there is a choice.
            let rates = d.rates(d.width, d.height);
            if rates.len() > 1 {
                let opts: Vec<(&str, bool, AppMessage)> = rates
                    .iter()
                    .map(|r| {
                        (
                            hz_name(*r),
                            (r - d.refresh).abs() < 0.5,
                            msg(Message::Display(
                                i,
                                DisplayChange::Mode(d.width, d.height, *r),
                            )),
                        )
                    })
                    .collect();
                rows.push(row_of(s, "Refresh rate", None, segments(s, &opts)));
            }
            let scales: Vec<(&str, bool, AppMessage)> =
                [("100%", 1.0), ("125%", 1.25), ("150%", 1.5), ("200%", 2.0)]
                    .into_iter()
                    .map(|(n, v)| {
                        (
                            n,
                            (d.scale - v).abs() < 0.01,
                            msg(Message::Display(i, DisplayChange::Scale(v))),
                        )
                    })
                    .collect();
            rows.push(row_of(s, "Scale", None, segments(s, &scales)));
            let turns: Vec<(&str, bool, AppMessage)> =
                [("Normal", 0u8), ("90°", 1), ("180°", 2), ("270°", 3)]
                    .into_iter()
                    .map(|(n, t)| {
                        (
                            n,
                            d.transform == t,
                            msg(Message::Display(i, DisplayChange::Rotate(t))),
                        )
                    })
                    .collect();
            rows.push(row_of(s, "Rotation", None, segments(s, &turns)));
            // Show another screen's picture (presenting), or its own space.
            let others: Vec<&crate::services::displays::Display> = st
                .displays
                .iter()
                .filter(|o| o.name != d.name && o.enabled && o.mirror.is_none())
                .collect();
            if !others.is_empty() {
                let mut opts: Vec<(&str, bool, AppMessage)> = vec![(
                    "Extend",
                    d.mirror.is_none(),
                    msg(Message::Display(i, DisplayChange::Mirror(None))),
                )];
                for o in others {
                    opts.push((
                        if st.displays.len() == 2 {
                            "Mirror"
                        } else {
                            o.label.as_str()
                        },
                        d.mirror.as_deref() == Some(o.name.as_str()),
                        msg(Message::Display(
                            i,
                            DisplayChange::Mirror(Some(o.name.clone())),
                        )),
                    ));
                }
                rows.push(row_of(s, "Show", None, segments(s, &opts)));
            }
            // The second of two screens: where it sits next to the first.
            if i == 1
                && st.displays.len() == 2
                && st.displays[0].enabled
                && st.displays.iter().all(|d| d.mirror.is_none())
            {
                let now = place_of(&st.displays);
                let places: Vec<(&str, bool, AppMessage)> = [
                    ("Left", Place::Left),
                    ("Right", Place::Right),
                    ("Above", Place::Above),
                    ("Below", Place::Below),
                ]
                .into_iter()
                .map(|(n, p)| {
                    (
                        n,
                        now == p,
                        msg(Message::Display(i, DisplayChange::Place(p))),
                    )
                })
                .collect();
                rows.push(row_of(s, "Position", None, segments(s, &places)));
            }
        }
        out.extend(group(s, Some(d.label.as_str()), rows));
    }
    // Night light: on or off, how warm, and from sunset to sunrise by itself (the same as the
    // control centre's).
    {
        use crate::panels::control::{COOLEST, Message as C, WARMEST};
        let k = app
            .panels
            .control
            .kelvin
            .unwrap_or(app.config.night_light_temperature);
        let v = ((k as f32 - WARMEST) / (COOLEST - WARMEST)).clamp(0.0, 1.0);
        let warm = Color::from_rgb8(0xFF, 0xA8, 0x55);
        let bar = crate::ui::slider::slider(
            iced::widget::Space::new().width(Length::Fill),
            v,
            22.0,
            (s.ink.scale_alpha(0.08 * s.opacity), s.fade(warm)),
            |v| snd(C::Warmth(v)),
        )
        .on_release(snd(C::WarmthDone));
        let mut rows = vec![
            row_of(
                s,
                "Night light",
                None,
                ui::switch(s, app.night_light, snd(C::NightLight)),
            ),
            row_of(
                s,
                "Warmth",
                None,
                row![
                    container(bar).width(200),
                    container(label(format!("{k} K"), text::SMALL, s.ink_dim))
                        .width(56)
                        .align_right(56),
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center)
                .into(),
            ),
        ];
        if let Some(w) = app.weather.as_ref().filter(|w| !w.sunset.is_empty()) {
            let auto = app.config.night_light_schedule;
            rows.push(row_of(
                s,
                "Sunset to sunrise",
                None,
                row![
                    label(
                        format!("{} – {}", w.sunset, w.sunrise),
                        text::SMALL,
                        s.ink_dim
                    ),
                    ui::switch(s, auto, snd(C::Schedule(!auto))),
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        out.extend(group(s, Some("Night light"), rows));
    }
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
