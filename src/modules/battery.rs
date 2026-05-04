//! Battery: its charge (red when nearly empty, breathing while charging), with the
//! percentage if chosen; opens the control centre's Power page.

use crate::app::{App, Message, Output};
use crate::panels::{PanelKind, control};
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
    let low = p.percent < 15.0 && !p.charging;
    let charging = p.charging && p.percent < 99.5;
    let color = if low {
        s.bad
    } else if charging {
        s.ink.scale_alpha(app.breath())
    } else {
        s.ink
    };
    let active = super::control_on(app, open, control::Page::Power);
    let press = move |r| Message::ControlAt(control::Page::Power, id, r);
    if !app.config.bar.battery_percent {
        return vec![
            super::icon_chip(s, control::battery_icon(p), color, active)
                .on_press_at(press)
                .into(),
        ];
    }
    let content = row![
        icon(control::battery_icon(p), size::ICON, color),
        label(
            format!("{:.0}%", p.percent),
            text::SMALL,
            if low { s.bad } else { s.ink }
        ),
    ]
    .spacing(5)
    .align_y(Alignment::Center);
    vec![super::chip(s, content, active).on_press_at(press).into()]
}
