//! AirPods, while connected: a circle whose edge fills to the lower bud's charge.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::Scheme;
use crate::theme::tok;
use crate::ui::{self, Icon, icon};
use iced::Element;
use iced::widget::{container, stack};

/// AirPods, while connected: a circle whose edge is a ring filled to the lower bud's charge
/// (green while charging, amber then red when low), headphones inside. Opens their panel.
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    pods(app, o, s, open).into_iter().collect()
}

fn pods<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Option<Element<'a, Message>> {
    crate::panels::pods::device(&app.bt)?;
    let id = o.id;
    let d = tok::BAR - 6.0;
    let active = open == Some(PanelKind::Pods);
    let charge = app.pods.as_ref().and_then(|p| {
        Some(crate::services::airpods::Charge {
            level: p.level()?,
            charging: p.charging(),
        })
    });
    // The circle's edge is the gauge: a thin ring filled to the charge, headphones inside.
    let (value, tint) = match charge {
        Some(c) => (f32::from(c.level) / 100.0, crate::panels::pods::tint(s, c)),
        None => (0.0, s.ink_faint),
    };
    Some(
        // Inset from the circle's edge: a band of the pill's black keeps the ring apart from
        // the wallpaper (a ring on the very edge melted into a wallpaper of the same colour).
        ui::control(stack![
            // Slim: the charge in a 2 px arc over a barely-there track (a 3 px ring on a bright
            // track read as a heavy outline: his call 2026-11-15).
            container(ui::ring::ring_on(
                value,
                d - crate::modules::island::BUBBLE_INSET,
                2.0,
                tint,
                s.ink.scale_alpha(tok::TONAL * s.opacity),
                crate::theme::surface(s),
            ))
            .center(d),
            container(icon(Icon::Headphones, tok::ICON_S, s.ink)).center(d),
        ])
        .width(d)
        .height(d)
        .center_x()
        .radius(d / 2.0)
        .bar(s, active)
        .on_press_at(move |r| Message::Toggle(PanelKind::Pods, id, r))
        .into(),
    )
}
