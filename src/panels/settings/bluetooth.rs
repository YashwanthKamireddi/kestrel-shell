//! Settings › bluetooth: its page.

use super::*;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A name that becomes a field when clicked (Enter keeps it, the field leaving drops it).
pub(super) fn renamable<'a>(
    s: Scheme,
    st: &'a State,
    target: Option<String>,
    now: String,
) -> Element<'a, AppMessage> {
    match &st.bt_rename {
        Some((t, text)) if *t == target => field(
            s,
            "bt-rename",
            "Name",
            text,
            |t| msg(Message::BtRenameText(t)),
            Some(msg(Message::BtRenameDone)),
            false,
        ),
        _ => control(label(now.clone(), text::BODY, s.ink_dim))
            .padding([4, 10])
            .radius(14.0)
            .kind(s, crate::ui::control::Kind::Ghost)
            .no_press_scale()
            .on_press(msg(Message::BtRename(target, now)))
            .into(),
    }
}

pub(super) fn bluetooth_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    use crate::panels::control::Message as C;
    let st = &app.panels.settings;
    let cst = &app.panels.control;
    let b = &app.bt;
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    if !b.available {
        out.push(crate::ui::empty(s, Icon::Bluetooth));
        return out;
    }
    let mut top = vec![row_of(
        s,
        "Bluetooth",
        None,
        ui::switch(s, b.powered, snd(C::Bluetooth(!b.powered))),
    )];
    if b.powered {
        // While this page is open, other devices can find this computer by this name.
        top.push(row_of(
            s,
            "Name",
            Some(if b.discoverable {
                "Visible to devices nearby while this page is open"
            } else {
                "Other devices see this name"
            }),
            renamable(s, st, None, b.name.clone()),
        ));
        if !b.address.is_empty() {
            top.push(copy_row(s, "Address", b.address.clone()));
        }
    }
    out.extend(group(s, None, top));
    if !b.powered {
        return out;
    }

    // Your devices: connect or disconnect, its battery, rename, forget (asked once).
    let mut mine = Vec::new();
    for d in &b.devices {
        if !matches(&d.name) && !matches("bluetooth") {
            continue;
        }
        let busy = crate::panels::control::in_progress(app, &d.path);
        let err = cst
            .bt_error
            .as_ref()
            .filter(|(p, _)| *p == d.path)
            .map(|(_, w)| w.clone());
        let state = match (busy, d.connected, d.battery) {
            (Some(true), _, _) => "Connecting…".to_string(),
            (Some(false), _, _) => "Disconnecting…".to_string(),
            _ if err.is_some() => err.unwrap_or_default(),
            (None, true, Some(p)) => format!("Connected · battery {p}%"),
            (None, true, None) => "Connected".to_string(),
            (None, false, _) => "Not connected".to_string(),
        };
        let armed = cst.forget.as_deref() == Some(d.path.as_str());
        let name: Element<'_, AppMessage> = match &st.bt_rename {
            Some((Some(p), _)) if *p == d.path => {
                renamable(s, st, Some(d.path.clone()), d.name.clone())
            }
            // A click on the name renames it.
            _ => column![
                control(label(d.name.clone(), text::BODY, s.ink))
                    .padding([2, 6])
                    .radius(10.0)
                    .kind(s, crate::ui::control::Kind::Ghost)
                    .no_press_scale()
                    .on_press(msg(Message::BtRename(Some(d.path.clone()), d.name.clone()))),
                container(label(state, text::CAPTION, s.ink_dim)).padding([0, 6]),
            ]
            .spacing(0)
            .into(),
        };
        mine.push(Some(
            container(
                row![
                    icon(
                        crate::panels::control::device_icon(&d.icon),
                        18.0,
                        if d.connected { s.accent } else { s.ink_dim }
                    ),
                    container(name).width(Length::Fill),
                    control(label(
                        if d.connected { "Disconnect" } else { "Connect" },
                        text::SMALL,
                        s.ink
                    ))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(snd(C::Device(d.path.clone(), d.connected))),
                    control(label(
                        if armed { "Forget?" } else { "Forget" },
                        text::SMALL,
                        if armed { s.bad } else { s.ink }
                    ))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(snd(C::Forget(d.path.clone()))),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([0, 16])
            .height(56)
            .align_y(Alignment::Center)
            .into(),
        ));
    }
    out.extend(group(s, Some("My devices"), mine));

    // Devices nearby, found while this page is open: Pair pairs, trusts and connects.
    let mut near = Vec::new();
    for d in &b.nearby {
        if !matches(&d.name) && !matches("bluetooth") {
            continue;
        }
        let pairing = cst.pairing.as_deref() == Some(d.path.as_str());
        let err = cst
            .bt_error
            .as_ref()
            .filter(|(p, _)| *p == d.path)
            .map(|(_, w)| w.clone());
        let mut words = column![label(d.name.clone(), text::BODY, s.ink)].spacing(2);
        if pairing {
            words = words.push(label("Pairing…", text::CAPTION, s.ink_dim));
        } else if let Some(e) = err {
            words = words.push(label(e, text::CAPTION, s.bad));
        }
        let mut pair =
            control(label("Pair", text::SMALL, s.ink)).pill(s, crate::ui::control::Pill::Plain);
        if cst.pairing.is_none() {
            pair = pair.on_press(snd(C::Pair(d.path.clone())));
        }
        near.push(Some(
            container(
                row![
                    icon(
                        crate::panels::control::device_icon(&d.icon),
                        18.0,
                        s.ink_dim
                    ),
                    words.width(Length::Fill),
                    pair,
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([0, 16])
            .height(52)
            .align_y(Alignment::Center)
            .into(),
        ));
    }
    if near.is_empty() {
        near.push(Some(ui::waiting(s, Icon::Search, "Looking for devices…")));
    }
    out.extend(group(s, Some("Devices nearby"), near));
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
