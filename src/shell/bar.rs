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
/// How far a panel glides down into place as it fades in.
const GLIDE: f32 = 8.0;
pub const SLOT_GAP: f32 = 2.0;

/// Left edge and width of the highlight behind the active workspace.
pub fn pill_geometry(_h: &hyprland::State, _monitor: &str, active: i32, _min: u32) -> (f32, f32) {
    ((active.max(1) - 1) as f32 * (SLOT + SLOT_GAP), SLOT)
}

pub fn slot_count(h: &hyprland::State, min: u32) -> usize {
    let max_id = h.workspaces.iter().map(|w| w.id).max().unwrap_or(1).max(1) as u32;
    max_id.max(min) as usize
}

pub fn view<'a>(app: &'a App, o: &'a Output) -> Element<'a, Message> {
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
    // The panel opens by fading in at its full size while gliding the last few pixels down
    // into place (and closes the same way, faster); it does not unroll.
    let mut panel_drop = None;
    if let Some(p) = panel {
        let natural = app.natural.get(&p.kind).map_or(0.0, |s| s.height);
        let k = p.progress(natural);
        let lift = if pills { (1.0 - k) * GLIDE } else { 0.0 };
        let h = if p.h.value > 0.0 { p.h.value } else { natural };
        let mut d = drop(p.x.value, p.w.value, h);
        d.top -= lift;
        panel_drop = Some((d, k, lift, h));
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

    let shape = canvas(BarShape {
        color: s.base,
        bar: size::BAR,
        frame: t,
        corner: app.frame().radius,
        band: !pills,
        drops,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let open = panel.filter(|p| p.open).map(|p| p.kind);
    // The time sits at the exact centre of the screen, always: the centre items listed before it
    // line up to its left, the ones after to its right, so switching items on or off never
    // moves it (the clock, or the island when there is no separate clock).
    let center = &app.config.bar.center;
    let anchor = center
        .iter()
        .position(|m| m == "clock")
        .or_else(|| center.iter().position(|m| m == "island"));
    let sides = row![
        container(section(&app.config.bar.left, app, o, s, open, pills)).width(Length::Fill),
        container(section(&app.config.bar.right, app, o, s, open, pills))
            .width(Length::Fill)
            .align_x(Alignment::End),
    ]
    .align_y(Alignment::Center);
    let middle: Element<'a, Message> = match anchor {
        Some(i) => {
            let mut before = row![section(&center[..i], app, o, s, open, pills)]
                .spacing(6)
                .align_y(Alignment::Center);
            // Music with no media item of its own: its bubble sits just left of the time.
            if pills
                && !app.config.bar.contains("media")
                && center[i] == "island"
                && let Some(b) = crate::modules::island::bubble(app, o, s)
            {
                before = before.push(b);
            }
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
    let bar = container(stack![sides, middle].width(Length::Fill).height(size::BAR))
        .padding([0.0, if pills { PILL_MARGIN } else { t + 6.0 }])
        .height(size::BAR)
        .center_y(size::BAR);

    let mut layers: Vec<Element<'a, Message>> = Vec::new();
    let panel_shape = panel_drop.map(|(d, k, _, _)| {
        canvas(BarShape {
            // Closing, the box goes with its content instead of lingering faintly after it.
            color: s.base.scale_alpha(if panel.is_some_and(|p| p.open) {
                k
            } else {
                crate::motion::range(k, 0.45, 1.0)
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
    layers.push(bar.into());
    if let Some(p) = panel {
        let natural = app.natural.get(&p.kind).map_or(0.0, |s| s.height);
        let alpha = p.alpha(natural);
        let width = p.kind.width();
        let content = panels::view(app, p.kind, s.faded(alpha));
        let x = p.x.value + (p.w.value - width) / 2.0;
        let kind = p.kind;
        layers.push(
            pin(
                ui::reveal(content, width, panel_drop.map_or(p.h.value, |d| d.3))
                    .on_measure(move |sz| Message::Measured(kind, sz)),
            )
            .position(Point::new(
                x,
                size::BAR + top_gap - panel_drop.map_or(0.0, |d| d.2),
            ))
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
/// Items that belong together share one pill (the status cluster, the tray, the bell, power);
/// anything else stands alone in a pill of its own.
const CLUSTER: &[&str] = &["tray", "privacy", "status", "notifications", "power"];

/// One bar section: its modules' items in order. In the pill style every standalone module
/// sits on its own floating pill and the clustered ones share one (none when empty).
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
    // Consecutive clustered modules form one group; every other module is a group alone.
    let mut groups: Vec<Vec<Element<'a, Message>>> = Vec::new();
    let mut in_cluster = false;
    for id in ids {
        let items = crate::modules::items(id, app, o, s, open);
        if items.is_empty() {
            continue;
        }
        let clustered = CLUSTER.contains(&id.as_str());
        if clustered && in_cluster {
            if let Some(g) = groups.last_mut() {
                g.extend(items);
            }
        } else {
            groups.push(items);
        }
        in_cluster = clustered;
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
