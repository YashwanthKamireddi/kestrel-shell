//! kestrel's component kit. Screens are assembled only from these.

pub mod control;
pub mod icon;
pub mod reveal;
pub mod shape;
pub mod slider;
pub mod workspaces;

use crate::theme::{self, Scheme, size};
use iced::widget::{Space, Svg, Text, container, row, svg, text};
use iced::{Alignment, Color, Element, Length, font};

pub use control::control;
pub use icon::Icon;
pub use reveal::reveal;

/// Text in the kestrel type scale.
pub fn label<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color) -> Text<'a> {
    text(s)
        .size(size)
        .color(color)
        .font(theme::FONT)
        .wrapping(text::Wrapping::None)
}

pub fn label_w<'a>(
    s: impl text::IntoFragment<'a>,
    size: f32,
    color: Color,
    weight: font::Weight,
) -> Text<'a> {
    label(s, size, color).font(theme::font(weight))
}

/// Shortens `s` to at most `max` characters, ending with an ellipsis.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out = out.trim_end().to_string();
        out.push('…');
        out
    }
}

pub fn icon(i: Icon, size: f32, color: Color) -> Svg<'static> {
    svg(i.handle())
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}

/// A round icon button for the bar and panel headers. `active` marks an open panel or an
/// enabled state.
pub fn icon_button<'a, M: Clone + 'static>(
    s: Scheme,
    i: Icon,
    on_press: M,
    active: bool,
) -> control::Control<'a, M> {
    let (fg, rest) = if active {
        (s.accent, s.accent_soft)
    } else {
        (s.ink, Color::TRANSPARENT)
    };
    control(container(icon(i, size::ICON, fg)).center(Length::Fixed(size::CONTROL)))
        .radius(size::CONTROL / 2.0)
        .colors(
            rest,
            if active { rest } else { s.raised_hi },
            s.raised_press,
        )
        .on_press(on_press)
}

/// The card that groups related controls inside a panel.
pub fn card<'a, M: 'a>(s: Scheme, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(size::PANEL_PAD)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(s.raised.into()),
            border: iced::Border::default().rounded(size::CARD_RADIUS),
            ..container::Style::default()
        })
        .into()
}

/// A panel heading with optional trailing controls.
pub fn heading<'a, M: 'a>(
    s: Scheme,
    title: impl text::IntoFragment<'a>,
    trailing: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    row![
        label_w(title, theme::text::HEADING, s.ink, theme::SEMIBOLD),
        Space::new().width(Length::Fill),
        trailing.into(),
    ]
    .align_y(Alignment::Center)
    .height(size::CONTROL)
    .into()
}

/// A 1 px hairline.
pub fn divider<'a, M: 'a>(s: Scheme) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(s.line.into()),
            ..container::Style::default()
        })
        .into()
}

pub fn gap<'a, M: 'a>(n: f32) -> Element<'a, M> {
    Space::new().width(n).height(n).into()
}
