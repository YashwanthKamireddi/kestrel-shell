//! AirPods, opened from their circle in the bar: each bud's and the case's charge as rings,
//! where each bud is (in an ear, out, in the case), and a way to disconnect.

use crate::app::{App, Message as AppMessage};
use crate::services::airpods::{Charge, Place};
use crate::services::bluetooth;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row, stack};
use iced::{Alignment, Color, Element, Length};

pub const WIDTH: f32 = 360.0;

/// The connected AirPods, if any.
pub fn device(app: &App) -> Option<&bluetooth::Device> {
    app.bt
        .connected()
        .find(|d| crate::services::airpods::is_airpods(&d.modalias, &d.icon))
}

/// The colour for a charge: green charging, red low, amber getting low, else white.
pub fn tint(s: Scheme, c: Charge) -> Color {
    if c.charging {
        s.good
    } else if c.level <= 15 {
        s.bad
    } else if c.level <= 30 {
        s.warn
    } else {
        s.ink
    }
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let name = device(app).map_or("AirPods", |d| d.name.as_str());
    let header = row![
        icon(Icon::Earbuds, 20.0, s.ink),
        column![
            label_w(
                ui::fit(name, text::TITLE, 240.0),
                text::TITLE,
                s.ink,
                theme::SEMIBOLD
            ),
            label(
                if app.pods.is_some() {
                    "Connected"
                } else {
                    "Connected · reading battery…"
                },
                text::CAPTION,
                s.ink_dim
            ),
        ]
        .spacing(1),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let p = app.pods.clone().unwrap_or_default();
    let parts: Element<'_, AppMessage> = if let Some(h) = p.headset {
        row![part(s, Icon::Headphones, "Headphones", Some(h), None)].into()
    } else {
        row![
            part(s, Icon::EarbudLeft, "Left", p.left, Some(p.left_at)),
            part(s, Icon::EarbudCase, "Case", p.case, None),
            part(s, Icon::EarbudRight, "Right", p.right, Some(p.right_at)),
        ]
        .into()
    };

    let disconnect = device(app).map(|d| {
        AppMessage::Panel(super::Message::Control(super::control::Message::Device(
            d.path.clone(),
            true,
        )))
    });
    let actions = row![
        Space::new().width(Length::Fill),
        control(label("Disconnect", text::SMALL, s.ink))
            .padding([7, 16])
            .radius(16.0)
            .colors(
                s.ink.scale_alpha(0.08 * s.opacity),
                s.hover(),
                s.raised_press
            )
            .on_press_maybe(disconnect),
    ];
    container(
        column![header, container(parts).padding([6, 0]), actions]
            .spacing(14)
            .width(Length::Fill),
    )
    .padding([16, 18])
    .into()
}

/// One part: a ring filled to its charge around its glyph, its name, its charge and place.
fn part<'a>(
    s: Scheme,
    glyph: Icon,
    name: &'a str,
    charge: Option<Charge>,
    at: Option<Place>,
) -> Element<'a, AppMessage> {
    const D: f32 = 74.0;
    let (value, color) = match charge {
        Some(c) => (f32::from(c.level) / 100.0, tint(s, c)),
        None => (0.0, s.ink_faint),
    };
    let ring = stack![
        ui::ring::ring(
            value,
            D,
            5.0,
            s.fade(color),
            s.ink.scale_alpha(0.1 * s.opacity)
        ),
        container(icon(
            glyph,
            28.0,
            if charge.is_some() { s.ink } else { s.ink_faint }
        ))
        .center(D),
    ];
    let level: Element<'a, AppMessage> = match charge {
        Some(c) => {
            let mut r = row![label_w(
                format!("{}%", c.level),
                text::TITLE,
                s.ink,
                theme::SEMIBOLD
            )]
            .spacing(3)
            .align_y(Alignment::Center);
            if c.charging {
                r = r.push(icon(Icon::Zap, 14.0, s.good));
            }
            r.into()
        }
        None => label_w("—", text::TITLE, s.ink_faint, theme::SEMIBOLD).into(),
    };
    let place = match (at, charge) {
        (_, None) if name == "Case" => "Put a bud in",
        (_, None) => "Not connected",
        (Some(Place::Ear), _) => "In ear",
        (Some(Place::Out), _) => "Out",
        (Some(Place::Case), _) => "In case",
        _ => "",
    };
    column![
        ring,
        Space::new().height(4),
        label(name, text::CAPTION, s.ink_dim),
        level,
        label(place, text::CAPTION, s.ink_faint),
    ]
    .spacing(2)
    .align_x(Alignment::Center)
    .width(Length::Fill)
    .into()
}
