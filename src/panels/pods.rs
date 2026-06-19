//! AirPods: each bud's and the case's charge, shown under the AirPods in the control centre's
//! Bluetooth page (their own panel is gone, design call 2026-11-15: one home each).

use crate::app::{App, Message as AppMessage};
use crate::services::airpods::Charge;
use crate::services::bluetooth;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{Icon, icon, label, label_w};
use iced::widget::{Space, column, container, row, stack};
use iced::{Alignment, Color, Element, Length};

/// The connected AirPods, if any.
pub fn device(app: &App) -> Option<&bluetooth::Device> {
    app.bt
        .connected()
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

/// Each part's charge, a line each (left, right and the case while it reports; or the
/// headphones), for under the AirPods row.
pub fn parts(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let p = app.pods.clone().unwrap_or_default();
    let mut list = column![].spacing(tok::L);
    if let Some(h) = p.headset {
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
    container(list.width(Length::Fill))
        .padding([tok::S, tok::L])
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
                r = r.push(icon(Icon::Zap, 12.0, s.good));
            }
            r.into()
        }
        None => label("–", text::BODY, s.ink_faint).into(),
    };
    let on = charge.is_some();
    row![
        container(icon(glyph, 20.0, if on { s.ink } else { s.ink_faint })).center(26),
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
