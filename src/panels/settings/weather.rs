//! Settings › weather: its page.

use super::*;
use crate::theme::tok;

pub(super) fn time_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let city = text_input("City", &st.city)
        .on_input(Message::City)
        .on_submit(Message::SubmitCity)
        .padding([tok::S, tok::L])
        .size(text::BODY)
        .font(theme::FONT)
        .width(200)
        .style(move |_, _| text_input::Style {
            background: s.ink.scale_alpha(tok::FILL * s.opacity).into(),
            border: iced::Border::default().rounded(tok::R_ROW()),
            icon: s.ink_dim,
            placeholder: s.ink_faint,
            value: s.ink,
            selection: s.accent_soft,
        });
    let mut rows = vec![row_of(s, "City", None, city.into())];
    // The weather there now (shows the city was found); the place it resolved to only when
    // that says something new ("nyc" → New York), not the typed name again.
    if let Some(w) = &cx.weather {
        let typed = st.city.trim().to_lowercase();
        if !w.place.to_lowercase().starts_with(&typed) || typed.is_empty() {
            rows.push(value_row(s, "Place", w.place.clone()));
        }
        rows.push(value_row(
            s,
            "Now",
            format!("{} · {}", cx.temp(w.temp), w.describe()),
        ));
    }
    let units = cx.config.weather_units;
    rows.push(row_of(
        s,
        "Temperature",
        None,
        segments(
            s,
            &[
                (
                    "°C",
                    units == crate::config::Units::Celsius,
                    Message::Units("celsius"),
                ),
                (
                    "°F",
                    units == crate::config::Units::Fahrenheit,
                    Message::Units("fahrenheit"),
                ),
            ],
        ),
    ));
    group(s, None, rows).into_iter().collect()
}
