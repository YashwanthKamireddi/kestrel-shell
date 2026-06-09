//! CPU, memory and temperature (amber when high); opens the system monitor panel.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::tok;
use crate::theme::{Scheme, text};
use crate::ui::{Icon, icon, label};
use iced::widget::row;
use iced::{Alignment, Element};

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let sys = app.system;
    let warn = |hot: bool| if hot { s.warn } else { s.ink_dim };
    let mut r = row![
        icon(Icon::Cpu, 14.0, warn(sys.cpu >= 85)),
        label(format!("{}%", sys.cpu), text::SMALL, warn(sys.cpu >= 85)),
        icon(Icon::MemoryStick, 14.0, warn(sys.mem >= 85)),
        label(format!("{}%", sys.mem), text::SMALL, warn(sys.mem >= 85)),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    if let Some(t) = sys.temp {
        r = r
            .push(icon(Icon::Thermometer, 14.0, warn(t >= 85)))
            .push(label(format!("{t}°"), text::SMALL, warn(t >= 85)));
    }
    let active = open == Some(PanelKind::Monitor);
    vec![
        super::chip(s, r, active)
            .on_press_at(move |b| Message::Toggle(PanelKind::Monitor, id, b))
            .into(),
    ]
}
