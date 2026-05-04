//! Network: Wi-Fi strength, Ethernet, or offline (breathing while connecting); opens the
//! control centre's Wi-Fi page.

use crate::app::{App, Message, Output};
use crate::panels::{PanelKind, control};
use crate::theme::Scheme;
use iced::Element;

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    if !app.net.available {
        return Vec::new();
    }
    let id = o.id;
    let color = if app.net.connecting {
        s.ink.scale_alpha(app.breath())
    } else {
        s.ink
    };
    let active = super::control_on(app, open, control::Page::Wifi);
    vec![
        super::icon_chip(s, control::wifi_icon(&app.net), color, active)
            .on_press_at(move |r| Message::ControlAt(control::Page::Wifi, id, r))
            .into(),
    ]
}
