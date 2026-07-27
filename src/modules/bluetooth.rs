//! Bluetooth: on, connected, or off (dimmed, still there to turn it on); opens the control
//! centre's Bluetooth page.

use crate::app::{App, Message, Output};
use crate::panels::{PanelKind, control};
use crate::theme::Scheme;
use crate::ui::Icon;
use iced::Element;

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    if !app.bt.available {
        return Vec::new();
    }
    let id = o.id;
    let (i, color) = if !app.bt.powered {
        (Icon::BluetoothOff, s.ink_dim)
    } else if app.bt.connected().next().is_some() {
        (Icon::BluetoothConnected, s.ink)
    } else {
        (Icon::Bluetooth, s.ink)
    };
    let active = super::control_on(app, open, control::Page::Main);
    vec![
        super::icon_chip(s, i, color, active)
            .on_press_at(move |r| Message::ControlAt(control::Page::Main, id, r))
            .into(),
    ]
}
