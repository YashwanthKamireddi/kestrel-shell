//! The control centre's night page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn night_page<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let on = cx.night_light;
    let header = subpage_header(
        s,
        "Night light",
        Message::Page(Page::Main),
        switch(s, on, Message::NightLight),
    );
    let k = st.kelvin.unwrap_or(cx.config.night_light_temperature);
    let v = ((k as f32 - WARMEST) / (COOLEST - WARMEST)).clamp(0.0, 1.0);
    // The track runs from candle to daylight, so the slider shows what it does.
    let inner = row![
        container(icon(Icon::Sunset, tok::ICON, s.on_accent)).center(32),
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
        (s.ink.scale_alpha(tok::TONAL * s.opacity), s.fade(warm)),
        Message::Warmth,
    )
    .on_release(Message::WarmthDone);
    let ends = row![
        label("Warmer", text::CAPTION, s.ink_faint),
        Space::new().width(Length::Fill),
        label("Cooler", text::CAPTION, s.ink_faint),
    ];
    // Sunset to sunrise: the times come from the weather (its city); without them the switch
    // has nothing to follow, so it is not offered.
    let schedule: Element<'_, Message> = match cx.weather {
        Some(w) if !w.sunset.is_empty() => {
            let auto = cx.config.night_light_schedule;
            list_row(
                s,
                icon(Icon::Sunset, tok::ICON, s.ink_dim).into(),
                "Sunset to sunrise",
                Some(format!("{} – {}", w.sunset, w.sunrise)),
                switch(s, auto, Message::Schedule(!auto)),
                false,
                Some(Message::Schedule(!auto)),
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
