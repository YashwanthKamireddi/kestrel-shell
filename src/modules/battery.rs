//! Battery: its charge (red when nearly empty, breathing while charging), with the
//! percentage if chosen; opens the control centre's Power page.

use crate::app::{App, Message, Output};
use crate::panels::{PanelKind, control};
use crate::theme::tok;
use crate::theme::{Scheme, size, text};
use crate::ui::{icon, label};
use iced::widget::row;
use iced::{Alignment, Element};

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let p = &app.power;
    if !p.has_battery {
        return Vec::new();
    }
    let id = o.id;
    let low = p.percent < 15.0 && !p.charging && !p.held;
    let charging = p.charging && p.percent < 99.5;
    let color = if low { s.bad } else { s.ink };
    // Charging: the icon breathes by itself (redrawing only the bar, ~20×/s).
    let glyph = |px: f32| -> Element<'a, Message> {
        if charging {
            crate::ui::breathe::breathe(control::battery_icon(p), px, color).into()
        } else {
            icon(control::battery_icon(p), px, color).into()
        }
    };
    let active = super::control_on(app, open, control::Page::Main);
    let press = move |r| Message::ControlAt(control::Page::Main, id, r);
    if !app.config.bar.battery_percent {
        return vec![
            crate::ui::control(iced::widget::container(glyph(size::ICON)).center(size::CONTROL))
                .radius(size::CONTROL / 2.0)
                .bar(s, active)
                .on_press_at(press)
                .into(),
        ];
    }
    let content = row![
        glyph(size::ICON),
        label(
            format!("{:.0}%", p.percent),
            text::SMALL,
            if low { s.bad } else { s.ink }
        ),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    vec![super::chip(s, content, active).on_press_at(press).into()]
}
