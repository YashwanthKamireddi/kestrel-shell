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
    let active = super::control_on(app, open, control::Page::Wifi);
    let glyph = crate::ui::breathe::icon_or(
        control::wifi_icon(&app.net),
        crate::theme::size::ICON,
        s.ink,
        app.net.connecting,
    );
    vec![
        crate::ui::control(
            iced::widget::container(glyph)
                .center_x(crate::theme::bar::ITEM)
                .center_y(crate::theme::bar::PILL),
        )
        .radius(crate::theme::tok::FULL)
        .bar(s, active)
        .on_press_at(move |r| Message::ControlAt(control::Page::Wifi, id, r))
        .into(),
    ]
}
