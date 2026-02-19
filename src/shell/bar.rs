//! The bar: the top edge of the kestrel surface, and the panels hanging from it.

use crate::app::{App, Message, OSD_WIDTH, Output, TOAST_WIDTH};
use crate::panels::{self, PanelKind};
use crate::services::hyprland;
use crate::theme::{Scheme, size, text};
use crate::ui::shape::{BarShape, Drop};
use crate::ui::{self, label, label_w};
use iced::widget::{Space, canvas, container, mouse_area, pin, row, stack};
use iced::{Alignment, Element, Length, Point};

/// Workspace slot size and spacing in the bar.
pub const SLOT: f32 = 28.0;
/// Where the floating pills end (they are `BAR - 6` tall, centred in the bar).
pub const PILL_BOTTOM: f32 = size::BAR - 3.0;
/// The floating pills' distance from the screen's sides.
pub const PILL_MARGIN: f32 = 8.0;
pub const SLOT_GAP: f32 = 2.0;

/// Left edge and width of the highlight behind the active workspace.
pub fn pill_geometry(_h: &hyprland::State, _monitor: &str, active: i32, _min: u32) -> (f32, f32) {
    ((active.max(1) - 1) as f32 * (SLOT + SLOT_GAP), SLOT)
}

pub fn slot_count(h: &hyprland::State, min: u32) -> usize {
    let max_id = h.workspaces.iter().map(|w| w.id).max().unwrap_or(1).max(1) as u32;
    max_id.max(min) as usize
}

/// The bar surface: the bar itself (its band in the full style, the pills) and the frame's top
/// corners under it. Only as tall as that, so the bar's own motion (visualiser, workspaces,
/// clock) repaints a strip, not the screen.
pub fn view<'a>(app: &'a App, o: &'a Output) -> Element<'a, Message> {
    build(app, o, false)
}

/// The overlay surface: panels, toasts and pop-ups over the whole output, and a click catcher
/// outside the bar while a panel is open. Draws nothing (and repaints nothing) when idle.
pub fn overlay<'a>(app: &'a App, o: &'a Output) -> Element<'a, Message> {
    build(app, o, true)
}

/// Whether the overlay has anything to show on `o`.
pub fn overlay_busy(app: &App, o: &Output) -> bool {
    let focused = app.focused_output() == Some(o.id) && app.panel.is_none();
    let island = crate::modules::island::hosts_osd(app);
    app.panel.as_ref().is_some_and(|p| p.output == o.id)
        || (focused && !island && (app.toast_sheet.visible() || !app.toasts.is_empty()))
        || (focused && !island && app.osd.is_some())
}

fn build<'a>(app: &'a App, o: &'a Output, overlay: bool) -> Element<'a, Message> {
    let s = app.scheme;
    let t = app.frame().thickness;
    let pills = app.config.bar.style == crate::config::BarStyle::Pills;
    let panel = app.panel.as_ref().filter(|p| p.output == o.id);
    let focused = app.focused_output() == Some(o.id) && panel.is_none();
    let drop = |x: f32, w: f32, h: f32| Drop {
        x,
        width: w,
        height: h,
        radius: size::PANEL_RADIUS,
        join: size::JOIN,
        flush_left: !pills && x <= t + 0.5,
        flush_right: !pills && x + w >= o.size.width - t - 0.5,
        floating: pills,
        top: size::BAR + crate::ui::shape::FLOAT_GAP,
    };
    // Pills float with a margin; the full bar runs to the frame.
    let edge = if pills { PILL_MARGIN } else { t };
    let top_gap = if pills {
        crate::ui::shape::FLOAT_GAP
    } else {
        0.0
    };
    let toast_x = o.size.width - edge - TOAST_WIDTH;
    let osd_x = ((o.size.width - OSD_WIDTH) / 2.0).round();
    let mut drops = Vec::new();
    // The panel opens by fading in at its full size (and closes the same way, faster); it does
    // not unroll or move.
    let mut panel_drop = None;
    if let Some(p) = panel {
        let natural = app.natural.get(&p.kind).map_or(0.0, |s| s.height);
        let k = p.progress(natural);
        let h = if p.h.value > 0.0 { p.h.value } else { natural };
        // It fades in and out where it stands: moving it would put text and icons between
        // pixels for a few frames (soft, shaky).
        panel_drop = Some((drop(p.x.value, p.w.value, h), k, h));
    }
    // With the island in the bar, notifications arrive in it instead of a box of their own.
    let show_toasts = focused
        && (app.toast_sheet.visible() || !app.toasts.is_empty())
        && !crate::modules::island::hosts_osd(app);
    // With the island in the bar, pop-ups live inside it instead of dropping a box.
    let show_osd = focused && app.osd.is_some() && !crate::modules::island::hosts_osd(app);
    if show_toasts {
        drops.push(drop(toast_x, TOAST_WIDTH, app.toast_sheet.h.value));
    }
    if show_osd {
        drops.push(drop(osd_x, OSD_WIDTH, app.osd_sheet.h.value));
    }

    // The bar surface draws the band (and the frame's top corners); the overlay draws the
    // toasts' and pop-ups' boxes, joined to the band where it is.
    let shape = canvas(BarShape {
        color: s.base,
        bar: size::BAR,
        frame: t,
        corner: if overlay { 0.0 } else { app.frame().radius },
        band: !pills && !overlay,
        drops: if overlay { drops } else { Vec::new() },
    })
    .width(Length::Fill)
    .height(Length::Fill);

    // The bar's contents: built for the bar surface only.
    let bar: Option<Element<'a, Message>> = (!overlay).then(|| {
        let open = panel.filter(|p| p.open).map(|p| p.kind);
        // The time sits at the exact centre of the screen, always: the centre items listed before it
        // line up to its left, the ones after to its right, so switching items on or off never
        // moves it (the island, which is the time pill; else a clock).
        let center = &app.config.bar.center;
        let anchor = center
            .iter()
            .position(|m| m == "island")
            .or_else(|| center.iter().position(|m| m == "clock"));
        let sides = row![
            container(section(&app.config.bar.left, app, o, s, open, pills)).width(Length::Fill),
            container(section(&app.config.bar.right, app, o, s, open, pills))
                .width(Length::Fill)
                .align_x(Alignment::End),
        ]
        .align_y(Alignment::Center);
        let middle: Element<'a, Message> = match anchor {
            Some(i) => {
                let before = section(&center[..i], app, o, s, open, pills);
                row![
                    container(before)
                        .width(Length::Fill)
                        .align_x(Alignment::End),
                    section(&center[i..=i], app, o, s, open, pills),
                    container(section(&center[i + 1..], app, o, s, open, pills))
                        .width(Length::Fill)
                        .align_x(Alignment::Start),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
            }
            None => container(section(center, app, o, s, open, pills))
                .center_x(Length::Fill)
                .into(),
        };
        // The side margin goes on each layer: the stack does not pass a container's padding on
        // (the pills ended up touching the screen's edges).
        let margin = if pills { PILL_MARGIN } else { t + 6.0 };
        let layer = |e: Element<'a, Message>| {
            container(e)
                .padding([0.0, margin])
                .width(Length::Fill)
                .height(size::BAR)
                .center_y(size::BAR)
        };
        stack![layer(sides.into()), layer(middle)]
            .width(Length::Fill)
            .height(size::BAR)
            .into()
    });

    if let Some(bar) = bar {
        return stack![shape, bar]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    let mut layers: Vec<Element<'a, Message>> = Vec::new();
    let panel_shape = panel_drop.map(|(d, k, _)| {
        canvas(BarShape {
            // Closing, the box goes with its content instead of lingering faintly after it.
            color: s.base.scale_alpha(if panel.is_some_and(|p| p.open) {
                k
            } else {
                crate::motion::range(k, 0.4, 1.0)
            }),
            bar: size::BAR,
            frame: t,
            corner: 0.0,
            band: false,
            drops: vec![d],
        })
        .width(Length::Fill)
        .height(Length::Fill)
    });
    if panel.is_some() {
        // Clicking anywhere outside the panel closes it.
        layers.push(
            mouse_area(shape)
                .on_press(Message::Close)
                .on_right_press(Message::Close)
                .into(),
        );
    } else {
        layers.push(shape.into());
    }
    if let Some(ps) = panel_shape {
        layers.push(ps.into());
    }
    if let Some(p) = panel {
        let natural = app.natural.get(&p.kind).map_or(0.0, |s| s.height);
        let alpha = p.alpha(natural);
        let width = p.kind.width();
        let content = panels::view(app, p.kind, s.faded(alpha));
        let x = p.x.value + (p.w.value - width) / 2.0;
        let kind = p.kind;
        layers.push(
            pin(
                ui::reveal(content, width, panel_drop.map_or(p.h.value, |d| d.2))
                    .on_measure(move |sz| Message::Measured(kind, sz)),
            )
            .position(Point::new(x, size::BAR + top_gap))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        );
    }
    if show_toasts {
        let a = app.toast_sheet.alpha();
        let fs = s.faded(a);
        let cards = iced::widget::column(app.toasts.iter().rev().filter_map(|t| {
            app.notifications
                .iter()
                .find(|n| n.id == t.id)
                .map(|n| panels::notifications::card(fs, n))
        }))
        .spacing(8)
        .padding(10);
        layers.push(
            pin(ui::reveal(cards, TOAST_WIDTH, app.toast_sheet.h.value)
                .offset((1.0 - a) * 6.0)
                .on_measure(Message::MeasuredToasts))
            .position(Point::new(toast_x, size::BAR + top_gap))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        );
    }
    if show_osd && let Some(osd) = &app.osd {
        let a = app.osd_sheet.alpha();
        let fs = s.faded(a);
        layers.push(
            pin(
                ui::reveal(osd_view(fs, osd), OSD_WIDTH, app.osd_sheet.h.value)
                    .offset((1.0 - a) * 4.0)
                    .on_measure(Message::MeasuredOsd),
            )
            .position(Point::new(osd_x, size::BAR + top_gap))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        );
    }
    stack(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Icon, level bar, value.
fn osd_view<'a>(s: Scheme, osd: &crate::app::Osd) -> Element<'a, Message> {
    const PAD: f32 = 16.0;
    if let Some(t) = &osd.text {
        let mut words = iced::widget::column![label_w(
            ui::fit(t, text::BODY, OSD_WIDTH - 2.0 * PAD - 30.0),
            text::BODY,
            s.ink,
            crate::theme::MEDIUM
        )]
        .spacing(2);
        if let Some(sub) = &osd.sub {
            words = words.push(label(sub.clone(), text::SMALL, s.ink_dim));
        }
        return container(
            row![ui::icon(osd.icon, 18.0, s.ink), words]
                .spacing(12)
                .align_y(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding([14.0, PAD])
        .into();
    }
    const VALUE_W: f32 = 30.0;
    let track_w = OSD_WIDTH - 2.0 * PAD - 18.0 - VALUE_W - 2.0 * 12.0;
    let bar = |w: f32, c: iced::Color| {
        container(Space::new())
            .width(w)
            .height(6)
            .style(move |_| container::Style {
                background: Some(c.into()),
                border: iced::Border::default().rounded(3),
                ..container::Style::default()
            })
    };
    let track = stack![
        bar(track_w, s.raised_hi),
        bar((track_w * osd.value).max(6.0), s.accent),
    ];
    row![
        ui::icon(osd.icon, 18.0, s.ink),
        container(track).center_y(18),
        container(label(
            format!("{:.0}", osd.value * 100.0),
            text::SMALL,
            s.ink_dim
        ))
        .width(VALUE_W)
        .align_x(Alignment::End),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .padding([14.0, PAD])
    .into()
}

/// One bar section: its modules' items in order. In the pill style the section sits on its
/// own floating pill (none when the section is empty).
/// One bar section: its modules' items in order. In the pill style every module sits on its
/// own floating pill, except neighbours of one `[bar] groups` entry, which share one (none
/// when empty).
fn section<'a>(
    ids: &'a [String],
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
    pills: bool,
) -> Element<'a, Message> {
    if !pills {
        let items: Vec<Element<'a, Message>> = ids
            .iter()
            .flat_map(|id| crate::modules::items(id, app, o, s, open))
            .collect();
        return row(items).spacing(4).align_y(Alignment::Center).into();
    }
    // Neighbours of the same group share a pill; every other module is a pill alone.
    let mut groups: Vec<Vec<Element<'a, Message>>> = Vec::new();
    let mut last: Option<usize> = None;
    for id in ids {
        let items = crate::modules::items(id, app, o, s, open);
        if items.is_empty() {
            continue;
        }
        let group = app.config.bar.group_of(id);
        if group.is_some() && group == last {
            if let Some(g) = groups.last_mut() {
                g.extend(items);
            }
        } else {
            groups.push(items);
        }
        last = group;
    }
    let pills_row = groups.into_iter().map(|items| {
        // A pill with one item (the island) is that item: it fills the pill edge to edge.
        let single = items.len() == 1;
        container(row(items).spacing(4).align_y(Alignment::Center))
            .padding([0, if single { 0 } else { 4 }])
            .height(size::BAR - 6.0)
            .center_y(size::BAR - 6.0)
            .style(move |_| container::Style {
                background: Some(s.base.into()),
                border: iced::Border::default().rounded((size::BAR - 6.0) / 2.0),
                ..container::Style::default()
            })
            .into()
    });
    row(pills_row).spacing(6).align_y(Alignment::Center).into()
}
