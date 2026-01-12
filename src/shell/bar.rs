//! The bar: the top edge of the kestrel surface, and the panels hanging from it.

use crate::app::{App, Message, OSD_WIDTH, Output, TOAST_WIDTH};
use crate::panels::{self, PanelKind};
use crate::services::hyprland;
use crate::theme::{self, Scheme, size, text};
use crate::ui::shape::{BarShape, Drop};
use crate::ui::{self, Icon, control, icon_button, label, label_w};
use iced::widget::{Space, canvas, container, mouse_area, pin, row, stack};
use iced::{Alignment, Element, Length, Point};

/// Workspace slot size and spacing in the bar.
pub const SLOT: f32 = 28.0;
const SLOT_GAP: f32 = 2.0;

/// Left edge and width of the highlight behind the active workspace.
pub fn pill_geometry(_h: &hyprland::State, _monitor: &str, active: i32, _min: u32) -> (f32, f32) {
    ((active.max(1) - 1) as f32 * (SLOT + SLOT_GAP), SLOT)
}

fn slot_count(h: &hyprland::State, min: u32) -> usize {
    let max_id = h.workspaces.iter().map(|w| w.id).max().unwrap_or(1).max(1) as u32;
    max_id.max(min) as usize
}

pub fn view<'a>(app: &'a App, o: &'a Output) -> Element<'a, Message> {
    let s = app.scheme;
    let t = app.frame().thickness;
    let panel = app.panel.as_ref().filter(|p| p.output == o.id);
    let focused = app.focused_output() == Some(o.id) && panel.is_none();
    let drop = |x: f32, w: f32, h: f32| Drop {
        x,
        width: w,
        height: h,
        radius: size::PANEL_RADIUS,
        join: size::JOIN,
        flush_left: x <= t + 0.5,
        flush_right: x + w >= o.size.width - t - 0.5,
    };
    let toast_x = o.size.width - t - TOAST_WIDTH;
    let osd_x = ((o.size.width - OSD_WIDTH) / 2.0).round();
    let mut drops = Vec::new();
    if let Some(p) = panel {
        drops.push(drop(p.x.value, p.w.value, p.h.value));
    }
    let show_toasts = focused && (app.toast_sheet.visible() || !app.toasts.is_empty());
    let show_osd = focused && app.osd.is_some();
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
        drops,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let open = panel.filter(|p| p.open).map(|p| p.kind);
    let bar = container(
        row![
            container(left(app, o, s)).width(Length::Fill),
            center(app, o, s, open),
            container(right(app, o, s, open))
                .width(Length::Fill)
                .align_x(Alignment::End),
        ]
        .align_y(Alignment::Center),
    )
    .padding([0.0, t + 6.0])
    .height(size::BAR)
    .center_y(size::BAR);

    let mut layers: Vec<Element<'a, Message>> = Vec::new();
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
    layers.push(bar.into());
    if let Some(p) = panel {
        let natural = app.natural.get(&p.kind).map_or(0.0, |s| s.height);
        let alpha = p.alpha(natural);
        let width = p.kind.width();
        let content = panels::view(app, p.kind, s.faded(alpha));
        let x = p.x.value + (p.w.value - width) / 2.0;
        let kind = p.kind;
        layers.push(
            pin(ui::reveal(content, width, p.h.value)
                .offset((1.0 - alpha) * 6.0)
                .on_measure(move |sz| Message::Measured(kind, sz)))
            .position(Point::new(x, size::BAR))
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
            .position(Point::new(toast_x, size::BAR))
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
            .position(Point::new(osd_x, size::BAR))
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
        return container(
            row![
                ui::icon(osd.icon, 18.0, s.ink),
                label(t.clone(), text::BODY, s.ink),
            ]
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

fn left<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let n = slot_count(&app.hypr, app.config.workspaces);
    let active = app.hypr.active_on(&o.name);
    // One slot per workspace: the icon of the app last used there, or a faint number when the
    // workspace is empty. A soft highlight glides to the active one.
    let slots = row((1..=n as i32).map(|id| {
        let ws = app
            .hypr
            .workspaces
            .iter()
            .find(|w| w.id == id && w.windows > 0);
        let content: Element<'a, Message> = match ws {
            Some(w) if !w.app.is_empty() => {
                match crate::services::icons::lookup(&crate::services::apps::icon_for_class(&w.app))
                {
                    Some(p) => ui::image_file(&p, 16.0),
                    None => label(id.to_string(), text::SMALL, s.ink).into(),
                }
            }
            Some(_) => label(id.to_string(), text::SMALL, s.ink).into(),
            None => label(
                id.to_string(),
                text::CAPTION,
                if active == Some(id) {
                    s.ink_dim
                } else {
                    s.ink_faint
                },
            )
            .into(),
        };
        control(container(content).center(SLOT))
            .radius(size::CONTROL_RADIUS)
            .colors(
                iced::Color::TRANSPARENT,
                s.raised.scale_alpha(0.8),
                s.raised_press,
            )
            .on_press(Message::Dispatch(hyprland::focus_workspace(id)))
            .on_scroll(|lines| {
                Message::Dispatch(hyprland::cycle_workspace(if lines > 0.0 { -1 } else { 1 }))
            })
            .into()
    }))
    .spacing(SLOT_GAP);
    let highlight = container(Space::new())
        .width(SLOT)
        .height(SLOT)
        .style(move |_| container::Style {
            background: Some(s.raised_hi.into()),
            border: iced::Border::default().rounded(size::CONTROL_RADIUS),
            ..container::Style::default()
        });
    let width = n as f32 * (SLOT + SLOT_GAP) - SLOT_GAP;
    let dots = stack![
        pin(highlight).x(o.pill_x.value).width(width).height(SLOT),
        slots,
    ]
    .width(width)
    .height(SLOT);
    let title = if app.hypr.focused_monitor == o.name {
        crate::services::apps::name_for_class(&app.hypr.window_class)
    } else {
        String::new()
    };
    row![dots, label(title, text::SMALL, s.ink_dim),]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn center<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let active = open == Some(PanelKind::Calendar);
    let time = app.now.format("%H:%M").to_string();
    let date = app.now.format("%a %-d %b").to_string();
    let id = o.id;
    control(
        row![
            label_w(
                time,
                text::BODY,
                if active { s.accent } else { s.ink },
                theme::SEMIBOLD
            ),
            label(date, text::SMALL, s.ink_dim),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .height(size::CONTROL)
    .radius(size::CONTROL / 2.0)
    .colors(
        if active {
            s.accent_soft
        } else {
            iced::Color::TRANSPARENT
        },
        if active { s.accent_soft } else { s.raised_hi },
        s.raised_press,
    )
    .on_press_at(move |b| Message::Toggle(PanelKind::Calendar, id, b))
    .into()
}

fn right<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let id = o.id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    items.extend(panels::bar_items(app, o, s, open));
    items.push(
        icon_button(
            s,
            Icon::Power,
            Message::Close,
            open == Some(PanelKind::Power),
        )
        .on_press_at(move |b| Message::Toggle(PanelKind::Power, id, b))
        .into(),
    );
    row(items)
        .spacing(4)
        .align_y(Alignment::Center)
        .push(Space::new().width(0))
        .into()
}
