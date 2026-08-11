//! AirPods, opened from their circle in the bar: each bud's and the case's charge, and a way
//! to disconnect.

use super::cx::Cx;
use crate::app::Message as AppMessage;
use crate::services::airpods::Charge;
use crate::services::bluetooth;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row, stack};
use iced::{Alignment, Color, Element, Length};

pub const WIDTH: f32 = crate::theme::tok::PANEL_S;

/// The connected AirPods, if any.
pub fn device(bt: &bluetooth::State) -> Option<&bluetooth::Device> {
    bt.connected()
        .find(|d| crate::services::airpods::is_airpods(&d.modalias, &d.icon))
}

/// The colour for a charge: the theme's accent; green charging, amber then red when low.
pub fn tint(s: Scheme, c: Charge) -> Color {
    if c.charging {
        s.good
    } else if c.level <= 15 {
        s.bad
    } else if c.level <= 30 {
        s.warn
    } else {
        s.accent
    }
}

pub fn view<'a>(cx: &Cx<'a>) -> Element<'a, AppMessage> {
    let s = cx.scheme;
    let name = device(cx.bt).map_or("AirPods", |d| d.name.as_str());
    let disconnect = device(cx.bt).map(|d| {
        AppMessage::Panel(super::Message::Control(super::control::Message::Device(
            d.path.clone(),
            true,
        )))
    });
    let header = row![
        column![
            label_w(
                ui::fit(name, text::TITLE, 200.0),
                text::TITLE,
                s.ink,
                theme::SEMIBOLD
            ),
            label(
                if device(cx.bt).is_some_and(|d| cx.in_progress(&d.path) == Some(false)) {
                    "Disconnecting…"
                } else {
                    "Connected"
                },
                text::CAPTION,
                s.ink_dim
            ),
        ]
        .spacing(tok::HAIR)
        .width(Length::Fill),
        control(label("Disconnect", text::CAPTION, s.ink))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press_maybe(disconnect),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);

    let p = cx.pods.cloned().unwrap_or_default();
    let mut list = column![].spacing(tok::L);
    let nothing = p.headset.is_none() && p.left.is_none() && p.right.is_none() && p.case.is_none();
    if nothing {
        // No battery heard yet: say so, instead of empty bars.
        list = list.push(label(
            "Battery not reported yet. Take a bud out or open the case near the laptop.",
            text::SMALL,
            s.ink_dim,
        ));
    } else if let Some(h) = p.headset {
        list = list.push(part(s, Icon::Headphones, "Headphones", Some(h)));
    } else {
        // Left and right always; the case only while it reports (a bud inside).
        list = list
            .push(part(s, Icon::EarbudLeft, "Left", p.left))
            .push(part(s, Icon::EarbudRight, "Right", p.right));
        if p.case.is_some() {
            list = list.push(part(s, Icon::EarbudCase, "Case", p.case));
        }
    }
    container(column![header, list].spacing(tok::XL).width(Length::Fill))
        .padding(crate::theme::tok::PANEL_PAD)
        .into()
}

/// One part on a line: its glyph and name, then a charge bar and the number.
fn part<'a>(
    s: Scheme,
    glyph: Icon,
    name: &'a str,
    charge: Option<Charge>,
) -> Element<'a, AppMessage> {
    const TRACK: f32 = 110.0;
    let (fill, color) = match charge {
        Some(c) => (f32::from(c.level) / 100.0, tint(s, c)),
        None => (0.0, s.ink_faint),
    };
    let bar = |w: f32, c: Color| {
        container(Space::new())
            .width(w)
            .height(6)
            .style(move |_| container::Style {
                background: Some(c.into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            })
    };
    let mut track = stack![bar(TRACK, s.ink.scale_alpha(tok::FILL_STRONG * s.opacity))];
    if fill > 0.0 {
        track = track.push(bar((TRACK * fill).round().max(6.0), s.fade(color)));
    }
    let number: Element<'a, AppMessage> = match charge {
        Some(c) => {
            let mut r = row![label_w(
                format!("{}%", c.level),
                text::BODY,
                s.ink,
                theme::SEMIBOLD
            )]
            .spacing(tok::XXS)
            .align_y(Alignment::Center);
            if c.charging {
                r = r.push(icon(Icon::Zap, tok::ICON_XS, s.good));
            }
            r.into()
        }
        None => label("–", text::BODY, s.ink_faint).into(),
    };
    let on = charge.is_some();
    row![
        container(icon(
            glyph,
            tok::ICON_L,
            if on { s.ink } else { s.ink_faint }
        ))
        .center(26),
        label_w(
            name,
            text::BODY,
            if on { s.ink } else { s.ink_dim },
            theme::MEDIUM
        )
        .width(Length::Fill),
        track,
        container(number).width(48).align_x(Alignment::End),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center)
    .into()
}
