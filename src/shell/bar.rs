//! The bar: the top edge of the kestrel surface, and the panels hanging from it.

use crate::app::{App, Message, Output};
use crate::panels::{self, PanelKind};
use crate::services::hyprland;
use crate::theme::{self, Scheme, size, text};
use crate::ui::shape::{BarShape, Drop};
use crate::ui::workspaces::{self, Slot, workspaces};
use crate::ui::{self, Icon, control, icon_button, label, label_w};
use iced::widget::{Space, canvas, container, mouse_area, pin, row, stack};
use iced::{Alignment, Element, Length, Point};

/// Pill centre and width for the active workspace, relative to the dots widget.
pub fn pill_geometry(h: &hyprland::State, _monitor: &str, active: i32, min: u32) -> (f32, f32) {
    let _ = slot_count(h, min);
    let i = (active.max(1) - 1) as usize;
    (workspaces::slot_x(i), workspaces::PILL_W)
}

fn slot_count(h: &hyprland::State, min: u32) -> usize {
    let max_id = h.workspaces.iter().map(|w| w.id).max().unwrap_or(1).max(1) as u32;
    max_id.max(min) as usize
}

pub fn view<'a>(app: &'a App, o: &'a Output) -> Element<'a, Message> {
    let s = app.scheme;
    let t = app.config.frame.thickness;
    let panel = app.panel.as_ref().filter(|p| p.output == o.id);

    let shape = canvas(BarShape {
        color: s.base,
        bar: size::BAR,
        frame: t,
        corner: app.config.frame.radius,
        drops: panel
            .map(|p| Drop {
                x: p.x.value,
                width: p.w.value,
                height: p.h.value,
                radius: size::PANEL_RADIUS,
                join: size::JOIN,
                flush_left: p.x.value <= t + 0.5,
                flush_right: p.x.value + p.w.value >= o.size.width - t - 0.5,
            })
            .into_iter()
            .collect(),
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
    stack(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn left<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let n = slot_count(&app.hypr, app.config.workspaces);
    let slots = (1..=n as i32)
        .map(|id| {
            if app
                .hypr
                .workspaces
                .iter()
                .any(|w| w.id == id && w.windows > 0)
            {
                Slot::Occupied
            } else {
                Slot::Empty
            }
        })
        .collect();
    let dots = workspaces(
        s,
        slots,
        (o.pill_x.value, o.pill_w.value),
        |i| Message::Dispatch(hyprland::focus_workspace(i as i32 + 1)),
        |dir| Message::Dispatch(hyprland::cycle_workspace(dir)),
    );
    let id = o.id;
    let launcher = icon_button(s, Icon::LayoutGrid, Message::Close, false)
        .on_press_at(move |b| Message::Toggle(PanelKind::Launcher, id, b));
    let title = if app.hypr.focused_monitor == o.name {
        ui::ellipsize(&app.hypr.window_title, 64)
    } else {
        String::new()
    };
    row![launcher, dots, label(title, text::SMALL, s.ink_dim),]
        .spacing(6)
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
