//! The bar: the top edge of the kestrel surface, and the panels hanging from it.

use crate::app::{App, Message, OSD_WIDTH, Output, TOAST_WIDTH};
use crate::panels::{self, PanelKind};
use crate::services::hyprland;
use crate::theme::tok;
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
pub fn pill_geometry(_h: &hyprland::State, _monitor: &str, active: i32, n: u32) -> (f32, f32) {
    let (first, _) = group(active, n);
    ((active.max(1) - first) as f32 * (SLOT + SLOT_GAP), SLOT)
}

/// The workspaces the bar shows: the group of `n` holding `active` (1–5, 6–10, …), as ii does.
/// It used to show every number up to the highest workspace, so one stray workspace 28 put 28
/// slots in the bar.
pub fn group(active: i32, n: u32) -> (i32, usize) {
    let n = n.max(1) as i32;
    (((active.max(1) - 1) / n) * n + 1, n as usize)
}

/// Where scrolling over the workspaces goes from `active` (`step` ±1): through the workspaces
/// with windows and the shown group's slots, wrapping. Never past them: scrolling used to walk on
/// into new empty workspaces without end.
pub fn scroll_target(h: &hyprland::State, active: i32, n: u32, step: i32) -> i32 {
    let (first, count) = group(active, n);
    let mut ids: Vec<i32> = h
        .workspaces
        .iter()
        .filter(|w| w.id > 0 && w.windows > 0)
        .map(|w| w.id)
        .chain(first..first + count as i32)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let at = ids.iter().position(|&i| i == active).unwrap_or(0) as i32;
    let len = ids.len() as i32;
    ids[(at + step).rem_euclid(len) as usize]
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
        || (focused && (app.toast_sheet.visible() || !app.toasts.is_empty()))
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
        // Known before drawing (the overview): that, so frame and content change together.
        let h = match p.kind.height_for(app) {
            Some(h) => h,
            None if p.h.value > 0.0 => p.h.value,
            None => natural,
        };
        // Its size follows its content while open (the overview gains a card when you switch
        // to an empty workspace): the frame and the content share one live width, centred
        // where the panel opened and kept on screen.
        let (x, w) = panel_span(app, p, o.size.width, edge);
        panel_drop = Some((drop(x, w, h), k, h));
    }
    // Notifications always arrive as cards under the bar (design call 2026-11-13: squeezed into
    // one line in the island they were hard to read); the island keeps the short pop-ups.
    // With the island in the bar, notifications arrive in it instead (modules::island).
    let show_toasts = focused
        && !crate::modules::island::hosts_osd(app)
        && (app.toast_sheet.visible() || !app.toasts.is_empty());
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
        color: s.base.scale_alpha(panel_alpha(app)),
        bar: size::BAR,
        frame: t,
        corner: if overlay { 0.0 } else { app.frame().radius },
        band: !pills && !overlay,
        drops: if overlay { drops.clone() } else { Vec::new() },
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
                .spacing(tok::S)
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
        // While a panel or menu is open, a click on the bar anywhere but a button closes it
        // (the overlay's catcher covers the rest of the screen).
        let shape: Element<'a, Message> = if panel.is_some_and(|p| p.open) {
            mouse_area(shape)
                .on_press(Message::Close)
                .on_right_press(Message::Close)
                .into()
        } else {
            shape.into()
        };
        return stack![shape, bar]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    let mut layers: Vec<Element<'a, Message>> = Vec::new();
    let panel_shape = panel_drop.map(|(d, k, _)| {
        canvas(BarShape {
            // Closing, the box goes with its content instead of lingering faintly after it.
            color: s.base.scale_alpha(panel_alpha(app)).scale_alpha(
                if panel.is_some_and(|p| p.open) {
                    k
                } else {
                    crate::motion::range(k, 0.4, 1.0)
                },
            ),
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
    // Frosted: real blur (and its tint) fills the boxes of the panel, banners and pop-ups,
    // under their content.
    if app.config.material == crate::config::Material::Frosted {
        let mut boxes: Vec<(crate::ui::shape::Drop, f32, f32)> = drops
            .iter()
            .filter(|d| d.height > 0.5)
            .map(|d| (*d, d.height, 1.0))
            .collect();
        if let Some((d, k, h)) = panel_drop {
            let a = if panel.is_some_and(|p| p.open) {
                k
            } else {
                crate::motion::range(k, 0.4, 1.0)
            };
            boxes.push((d, h, a));
        }
        for (d, h, a) in boxes {
            layers.push(
                pin(frosted(
                    app,
                    o,
                    d.radius,
                    a * s.opacity,
                    Space::new().width(d.width).height(h),
                ))
                .position(Point::new(d.x, d.top))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            );
        }
    }
    if let Some(p) = panel {
        let natural = app.natural.get(&p.kind).map_or(0.0, |s| s.height);
        let alpha = p.alpha(natural);
        let (x, width) = panel_span(app, p, o.size.width, edge);
        let content = panels::view(app, p.kind, s.faded(alpha));
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
        // Each pop-up apart from the next by a hairline (two ran together as one card).
        let mut cards = iced::widget::column![].spacing(tok::XXS).padding(tok::XS);
        let shown = app
            .toasts
            .iter()
            .rev()
            .filter_map(|t| app.notifications.iter().find(|n| n.id == t.id));
        for (i, n) in shown.enumerate() {
            if i > 0 {
                cards = cards
                    .push(iced::widget::container(ui::divider(fs)).padding([tok::NONE, tok::L]));
            }
            cards = cards.push(panels::notifications::popup(
                app,
                fs,
                n,
                TOAST_WIDTH - 2.0 * tok::XS,
            ));
        }
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
            pin(ui::reveal(
                osd_view(fs, osd, (app.config.sound.max as f32 / 100.0).max(1.0)),
                OSD_WIDTH,
                app.osd_sheet.h.value,
            )
            .offset((1.0 - a) * 4.0)
            .on_measure(Message::MeasuredOsd))
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

/// Where an open panel stands and how wide it is now: its content's current width, centred on
/// where it opened, never past the screen's edges.
fn panel_span(app: &App, p: &crate::app::Panel, screen_w: f32, edge: f32) -> (f32, f32) {
    let w = p.kind.width_for(app).min(screen_w - 2.0 * edge);
    let centre = p.x.value + p.w.value / 2.0;
    let x = (centre - w / 2.0).clamp(edge, (screen_w - edge - w).max(edge));
    (x.round(), w)
}

/// Icon, level bar, value.
/// Whether an OSD is the output volume's.
pub fn is_volume(i: crate::ui::Icon) -> bool {
    use crate::ui::Icon;
    matches!(
        i,
        Icon::Volume | Icon::Volume1 | Icon::Volume2 | Icon::VolumeX
    )
}

fn osd_view<'a>(s: Scheme, osd: &crate::app::Osd, top_volume: f32) -> Element<'a, Message> {
    const PAD: f32 = 16.0;
    if let Some(t) = &osd.text {
        let mut words = iced::widget::column![label_w(
            ui::fit(t, text::BODY, OSD_WIDTH - 2.0 * PAD - 30.0),
            text::BODY,
            s.ink,
            crate::theme::MEDIUM
        )]
        .spacing(tok::XXS);
        if let Some(sub) = &osd.sub {
            words = words.push(label(sub.clone(), text::SMALL, s.ink_dim));
        }
        return container(
            row![ui::icon(osd.icon, 18.0, s.ink), words]
                .spacing(tok::L)
                .align_y(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding([14.0, PAD])
        .into();
    }
    const VALUE_W: f32 = 30.0;
    let track_w = OSD_WIDTH - 2.0 * PAD - 18.0 - VALUE_W - 2.0 * 12.0;
    // The track a tint of the ink; the number says the real level (a boosted volume reads 120).
    // A volume that may go past 100 %: the track runs to the maximum and the part
    // past 100 % is red (as the control centre's slider).
    let top = if is_volume(osd.icon) { top_volume } else { 1.0 };
    let track = ui::level_bar(s, osd.value, top, track_w, 6.0);
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
    .spacing(tok::L)
    .align_y(Alignment::Center)
    .padding([14.0, PAD])
    .into()
}

/// One bar section: its modules' items in order. In the pill style the section sits on its
/// own floating pill (none when the section is empty).
/// The message for a configured action: kestrel's own command, or a program to run.
fn action(cmd: &str) -> Message {
    match cmd.strip_prefix("kestrel:") {
        Some(c) => Message::Ipc(c.trim().to_string()),
        None => Message::Dispatch(crate::services::compositor::Action::Exec(String::from(cmd))),
    }
}

/// A module's items with its configured mouse actions (`[modules.<id>]`) around them.
fn with_actions<'a>(
    id: &str,
    app: &'a App,
    items: Vec<Element<'a, Message>>,
) -> Vec<Element<'a, Message>> {
    let Some(m) = app.config.modules.get(id) else {
        return items;
    };
    if items.is_empty() {
        return items;
    }
    let mut area = mouse_area(row(items).spacing(tok::XS).align_y(Alignment::Center));
    if let Some(c) = &m.right {
        area = area.on_right_press(action(c));
    }
    if let Some(c) = &m.middle {
        area = area.on_middle_press(action(c));
    }
    if m.scroll_up.is_some() || m.scroll_down.is_some() {
        let (up, down) = (m.scroll_up.clone(), m.scroll_down.clone());
        area = area.on_scroll(move |d| {
            let y = match d {
                iced::mouse::ScrollDelta::Lines { y, .. }
                | iced::mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            match if y > 0.0 { &up } else { &down } {
                Some(c) => action(c),
                None => Message::Batch(Vec::new()),
            }
        });
    }
    vec![area.into()]
}

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
            .flat_map(|id| with_actions(id, app, crate::modules::items(id, app, o, s, open)))
            .collect();
        return row(items)
            .spacing(tok::XS)
            .align_y(Alignment::Center)
            .into();
    }
    let material = app.config.material;
    // Neighbours of the same group share a pill; every other module is a pill alone.
    let mut groups: Vec<Vec<Element<'a, Message>>> = Vec::new();
    let mut last: Option<usize> = None;
    for id in ids {
        let items = with_actions(id, app, crate::modules::items(id, app, o, s, open));
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
        let pill = container(row(items).spacing(tok::XS).align_y(Alignment::Center))
            .padding([0, if single { 0 } else { 4 }])
            .height(size::BAR - 6.0)
            .center_y(size::BAR - 6.0)
            .style(move |_| pill_style(material, s, (size::BAR - 6.0) / 2.0));
        frosted(app, o, (size::BAR - 6.0) / 2.0, s.opacity, pill)
    });
    row(pills_row)
        .spacing(tok::S)
        .align_y(Alignment::Center)
        .into()
}

/// A pill's box in each material. Frosted has no fill of its own: the frost (`ui::frost`) is
/// drawn under it, and a faint rim keeps its edge on a dark wallpaper.
pub fn pill_style(m: crate::config::Material, s: Scheme, radius: f32) -> container::Style {
    use crate::config::Material;
    container::Style {
        background: match m {
            Material::Black => Some(s.base.into()),
            Material::Frosted => None,
        },
        border: iced::Border {
            color: match m {
                Material::Black => iced::Color::TRANSPARENT,
                Material::Frosted => s.ink.scale_alpha(tok::FILL * s.opacity),
            },
            width: if m == Material::Black { 0.0 } else { 1.0 },
            radius: radius.into(),
        },
        ..container::Style::default()
    }
}

/// `content` on frost at `radius` when the look is Frosted and the wallpaper is in; else as is.
pub fn frosted<'a>(
    app: &App,
    _o: &Output,
    radius: f32,
    opacity: f32,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if app.config.material != crate::config::Material::Frosted {
        return content.into();
    }
    // Real frosted glass: the compositor blurs whatever is behind (windows, the wallpaper)
    // in this shape (ext-background-effect-v1), and a dark tint on top keeps white text
    // readable on anything. Not a copy of the wallpaper: it is what is really there.
    let s = app.scheme;
    iced::blur_container(content)
        .style(move |_| iced::widget::container::Style {
            background: Some(s.base.scale_alpha(0.42 * opacity).into()),
            border: iced::Border::default().rounded(radius),
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// How opaque the panels' and banners' own fill is: none when frosted (the compositor's blur
/// and its tint are under them).
pub fn panel_alpha(app: &App) -> f32 {
    match app.config.material {
        crate::config::Material::Frosted => 0.0,
        _ => 1.0,
    }
}
