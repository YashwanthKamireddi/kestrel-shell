//! The control centre's bluetooth page.

use super::*;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub fn device_icon(icon_name: &str) -> Icon {
    match icon_name {
        n if n.contains("headset") || n.contains("headphone") => Icon::Headphones,
        n if n.contains("mouse") => Icon::Mouse,
        n if n.contains("keyboard") => Icon::Keyboard,
        n if n.contains("phone") => Icon::Smartphone,
        n if n.contains("computer") => Icon::Laptop,
        n if n.contains("audio") || n.contains("speaker") => Icon::Speaker,
        n if n.contains("gaming") || n.contains("joystick") => Icon::Gamepad2,
        n if n.contains("watch") => Icon::Watch,
        _ => Icon::Bluetooth,
    }
}

pub(super) fn bt_page(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let b = &app.bt;
    let header = subpage_header(
        s,
        "Bluetooth",
        msg(Message::Page(Page::Main)),
        switch(s, b.powered, msg(Message::Bluetooth(!b.powered))),
    );
    let st = &app.panels.control;
    let mut list = column![].spacing(2);
    if !b.powered {
        list = list.push(empty(s, Icon::BluetoothOff));
        return column![header, list].spacing(8).into();
    }
    // Your devices: a click connects or disconnects; the trailing button forgets (asking
    // once: "Forget?" in red, a second press removes it).
    for d in &b.devices {
        let busy = in_progress(app, &d.path);
        let err = st
            .bt_error
            .as_ref()
            .filter(|(p, _)| *p == d.path)
            .map(|(_, w)| w.clone());
        let sub = match (d.connected, d.battery) {
            _ if busy.is_some() => Some(
                if busy == Some(true) {
                    "Connecting…"
                } else {
                    "Disconnecting…"
                }
                .into(),
            ),
            _ if err.is_some() => err,
            (true, Some(p)) => Some(format!("Connected · {p}%")),
            (true, None) => Some("Connected".into()),
            (false, _) => None,
        };
        let armed = st.forget.as_deref() == Some(d.path.as_str());
        let forget: Element<'_, AppMessage> = if armed {
            crate::ui::control(label_w("Forget?", text::SMALL, s.base, theme::MEDIUM))
                .padding([4, 10])
                .radius(12.0)
                .colors(s.bad, s.bad, s.bad)
                .on_press(msg(Message::Forget(d.path.clone())))
                .into()
        } else {
            crate::ui::control(icon(Icon::X, 16.0, s.ink_dim))
                .padding(6)
                .radius(14.0)
                .colors(iced::Color::TRANSPARENT, s.hover(), s.raised_press)
                .on_press(msg(Message::Forget(d.path.clone())))
                .into()
        };
        list = list.push(list_row(
            s,
            icon(
                device_icon(&d.icon),
                size::ICON,
                if busy.is_some() {
                    s.accent.scale_alpha(app.breath())
                } else if d.connected {
                    s.accent
                } else {
                    s.ink
                },
            )
            .into(),
            d.name.clone(),
            sub,
            forget,
            d.connected,
            Some(msg(Message::Device(d.path.clone(), d.connected))),
        ));
    }
    // Devices nearby (searched for while this page is open): a click pairs, trusts and
    // connects. The search icon breathes while searching.
    let searching = icon(
        Icon::Search,
        14.0,
        if b.discovering {
            s.ink_dim.scale_alpha(app.breath())
        } else {
            s.ink_faint
        },
    );
    list = list.push(
        container(
            row![
                label_w("Nearby", text::SMALL, s.ink_dim, theme::SEMIBOLD).width(Length::Fill),
                searching,
            ]
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            top: 10.0,
            right: 12.0,
            bottom: 4.0,
            left: 12.0,
        }),
    );
    for d in &b.nearby {
        let pairing = st.pairing.as_deref() == Some(d.path.as_str());
        let sub = if pairing {
            Some("Pairing…".to_string())
        } else {
            st.bt_error
                .as_ref()
                .filter(|(p, _)| *p == d.path)
                .map(|(_, w)| w.clone())
        };
        list = list.push(list_row(
            s,
            icon(
                device_icon(&d.icon),
                size::ICON,
                if pairing {
                    s.accent.scale_alpha(app.breath())
                } else {
                    s.ink_dim
                },
            )
            .into(),
            d.name.clone(),
            sub,
            Space::new().into(),
            false,
            (st.pairing.is_none()).then(|| msg(Message::Pair(d.path.clone()))),
        ));
    }
    column![header, list].spacing(8).into()
}
