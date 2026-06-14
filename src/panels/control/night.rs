//! The control centre's night page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn night_page(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let on = app.night_light;
    let header = subpage_header(
        s,
        "Night light",
        msg(Message::Page(Page::Main)),
        switch(s, on, msg(Message::NightLight)),
    );
    let k = app
        .panels
        .control
        .kelvin
        .unwrap_or(app.config.night_light_temperature);
    let v = ((k as f32 - WARMEST) / (COOLEST - WARMEST)).clamp(0.0, 1.0);
    // The track runs from candle to daylight, so the slider shows what it does.
    let inner = row![
        container(icon(Icon::Sunset, size::ICON, s.on_accent)).center(32),
        Space::new().width(Length::Fill),
        label(
            format!("{k} K"),
            text::SMALL,
            if v > 0.82 { s.on_accent } else { s.ink_dim }
        ),
        Space::new().width(tok::L),
    ]
    .align_y(Alignment::Center);
    let warm = iced::Color::from_rgb8(0xFF, 0xA8, 0x55);
    let bar = slider(
        inner,
        v,
        32.0,
        (s.ink.scale_alpha(0.07 * s.opacity), s.fade(warm)),
        |v| msg(Message::Warmth(v)),
    )
    .on_release(msg(Message::WarmthDone));
    let ends = row![
        label("Warmer", text::CAPTION, s.ink_faint),
        Space::new().width(Length::Fill),
        label("Cooler", text::CAPTION, s.ink_faint),
    ];
    // Sunset to sunrise: the times come from the weather (its city); without them the switch
    // has nothing to follow, so it is not offered.
    let schedule: Element<'_, AppMessage> = match &app.weather {
        Some(w) if !w.sunset.is_empty() => {
            let auto = app.config.night_light_schedule;
            list_row(
                s,
                icon(Icon::Sunset, size::ICON, s.ink_dim).into(),
                "Sunset to sunrise",
                Some(format!("{} – {}", w.sunset, w.sunrise)),
                switch(s, auto, msg(Message::Schedule(!auto))),
                false,
                Some(msg(Message::Schedule(!auto))),
            )
        }
        _ => Space::new().into(),
    };
    column![
        header,
        column![bar, ends]
            .spacing(tok::S)
            .padding([tok::XS, tok::XS]),
        schedule
    ]
    .spacing(tok::L)
    .into()
}
