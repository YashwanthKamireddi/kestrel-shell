//! Settings › mouse: its page.

use super::*;
use crate::theme::tok;

pub(super) fn mouse_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    if app.panels.settings.hypr.is_empty() {
        return Vec::new();
    }
    [
        group(
            s,
            Some("Mouse"),
            crate::hypr_settings::MOUSE
                .iter()
                .map(|o| hypr_row(app, s, *o))
                .collect(),
        ),
        group(
            s,
            Some("Touchpad"),
            crate::hypr_settings::TOUCHPAD
                .iter()
                .map(|o| hypr_row(app, s, *o))
                .collect(),
        ),
        group(s, Some("Speed of each device"), device_rows(app, s)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// "pixart-dell-ms116-usb-optical-mouse" as "Pixart Dell Ms116 Usb Optical Mouse"; a
/// touchpad as "Touchpad".
pub(super) fn device_name(n: &str) -> String {
    if n.ends_with("touchpad") {
        return "Touchpad".into();
    }
    n.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            // Model numbers and USB in capitals ("MS116"), words capitalised.
            if w == "usb" || w.chars().any(|c| c.is_ascii_digit()) {
                return w.to_uppercase();
            }
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each pointing device's speed: its own when set, else the global pointer speed.
pub(super) fn device_rows(app: &App, s: Scheme) -> Vec<Option<Element<'_, AppMessage>>> {
    let st = &app.panels.settings;
    let global = st
        .hypr
        .get("input:sensitivity")
        .map_or(0.0, crate::hypr_settings::Value::as_num);
    st.pointers
        .iter()
        .map(|name| {
            let now = st
                .device_speeds
                .iter()
                .find(|d| d.name == *name)
                .map_or(global, |d| d.sensitivity);
            let (n1, n2) = (name.clone(), name.clone());
            let bar = crate::ui::slider::slider(
                iced::widget::Space::new().width(Length::Fill),
                ((now + 1.0) / 2.0).clamp(0.0, 1.0),
                22.0,
                (s.ink.scale_alpha(0.08 * s.opacity), s.accent),
                move |t| {
                    let v = ((t * 2.0 - 1.0) / 0.05).round() * 0.05;
                    msg(Message::DeviceSpeed(n1.clone(), v.clamp(-1.0, 1.0), false))
                },
            )
            .on_release(msg(Message::DeviceSpeed(n2, now, true)));
            row_of(
                s,
                kept(device_name(name)),
                None,
                row![
                    container(bar).width(200),
                    container(label(format!("{now:+.2}"), text::SMALL, s.ink_dim))
                        .width(56)
                        .align_right(56),
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center)
                .into(),
            )
        })
        .collect()
}
