//! kestrel's component kit. Screens are assembled only from these.

pub mod control;
pub mod icon;
pub mod morph;
pub mod reveal;
pub mod shape;
pub mod slider;

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

/// Shortens `s` to fit `width` px at `size` px text (Rubik averages about 0.56 em per character),
/// ending with an ellipsis.
pub fn fit(s: &str, size: f32, width: f32) -> String {
    let chars = (width / (size * 0.56)).floor().max(4.0) as usize;
    ellipsize(s, chars)
}

/// An image that fills a `w` × `h` frame (cropping as needed). Wrapped in a `Control`
/// because that is what clips: iced's image draws a covering image past its own bounds, and a
/// container's `clip` only narrows the viewport, it does not cut the drawing.
pub fn cover<'a, M: Clone + 'a>(
    handle: iced::widget::image::Handle,
    w: f32,
    h: f32,
    radius: f32,
) -> Element<'a, M> {
    control(
        iced::widget::image(handle)
            .width(w)
            .height(h)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(radius),
    )
    .width(w)
    .height(h)
    .into()
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

/// A quick-settings tile: icon, title and status. Accent-filled when on. An optional chevron
/// opens the tile's details.
pub fn tile<'a, M: Clone + 'static>(
    s: Scheme,
    i: Icon,
    title: impl text::IntoFragment<'a>,
    status: impl text::IntoFragment<'a>,
    on: bool,
    on_press: M,
    details: Option<M>,
) -> Element<'a, M> {
    let (bg, hover, fg, dim) = if on {
        (s.accent, s.accent, s.on_accent, s.on_accent)
    } else {
        (s.raised, s.raised_hi, s.ink, s.ink_dim)
    };
    let mut content = row![
        icon(i, size::ICON_TILE, fg),
        iced::widget::column![
            label_w(title, theme::text::BODY, fg, theme::MEDIUM),
            label(status, theme::text::CAPTION, dim),
        ]
        .spacing(1)
        .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if let Some(d) = details {
        let press = if on {
            s.on_accent.scale_alpha(0.12)
        } else {
            s.raised_press
        };
        content = content.push(
            control(container(icon(Icon::ChevronRight, size::ICON, dim)).center(28))
                .radius(14.0)
                .colors(Color::TRANSPARENT, press, press)
                .on_press(d),
        );
    }
    control(content)
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 14.0,
        })
        .height(56)
        .width(Length::Fill)
        .radius(size::CARD_RADIUS)
        .colors(bg, hover, if on { bg } else { s.raised_press })
        .on_press(on_press)
        .into()
}

/// An on/off switch.
pub fn switch<'a, M: Clone + 'static>(s: Scheme, on: bool, on_press: M) -> Element<'a, M> {
    let knob = container(Space::new())
        .width(14)
        .height(14)
        .style(move |_| container::Style {
            background: Some(if on { s.on_accent } else { s.ink_dim }.into()),
            border: iced::Border::default().rounded(7),
            ..container::Style::default()
        });
    let track = container(knob)
        .width(36)
        .height(20)
        .padding(3)
        .align_x(if on { Alignment::End } else { Alignment::Start })
        .align_y(Alignment::Center);
    control(track)
        .radius(10.0)
        .colors(
            if on { s.accent } else { s.raised_hi },
            if on { s.accent } else { s.raised_press },
            if on { s.accent } else { s.raised_press },
        )
        .on_press(on_press)
        .into()
}

/// A list row: leading icon, title, optional subtitle, trailing element.
pub fn list_row<'a, M: Clone + 'static>(
    s: Scheme,
    leading: Element<'a, M>,
    title: impl text::IntoFragment<'a>,
    subtitle: Option<String>,
    trailing: Element<'a, M>,
    selected: bool,
    on_press: Option<M>,
) -> Element<'a, M> {
    let mut texts = iced::widget::column![label_w(
        title,
        theme::text::BODY,
        s.ink,
        if selected {
            theme::MEDIUM
        } else {
            font::Weight::Normal
        }
    )]
    .spacing(1)
    .width(Length::Fill);
    if let Some(sub) = subtitle {
        texts = texts.push(label(sub, theme::text::CAPTION, s.ink_dim));
    }
    control(
        row![leading, texts, trailing]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .height(44)
    .width(Length::Fill)
    .radius(size::ROW_RADIUS)
    .colors(
        if selected {
            s.accent_soft
        } else {
            Color::TRANSPARENT
        },
        if selected { s.accent_soft } else { s.raised_hi },
        s.raised_press,
    )
    .no_press_scale()
    .on_press_maybe(on_press)
    .into()
}

/// A sub-page header: back button, title, optional trailing control.
pub fn subpage_header<'a, M: Clone + 'static>(
    s: Scheme,
    title: impl text::IntoFragment<'a>,
    back: M,
    trailing: Element<'a, M>,
) -> Element<'a, M> {
    row![
        icon_button(s, Icon::ArrowLeft, back, false),
        label_w(title, theme::text::TITLE, s.ink, theme::MEDIUM).width(Length::Fill),
        trailing,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .height(size::CONTROL + 4.0)
    .into()
}

/// Muted centred message for empty lists.
pub fn empty<'a, M: 'a>(s: Scheme, i: Icon, msg: impl text::IntoFragment<'a>) -> Element<'a, M> {
    container(
        iced::widget::column![
            icon(i, 22.0, s.ink_faint),
            label(msg, theme::text::SMALL, s.ink_dim)
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([24, 0])
    .into()
}

/// An image file (SVG or raster) at a square size.
pub fn image_file<'a, M: 'a>(path: &std::path::Path, size: f32) -> Element<'a, M> {
    if path.extension().is_some_and(|e| e == "svg") {
        iced::widget::svg(iced::widget::svg::Handle::from_path(path))
            .width(size)
            .height(size)
            .into()
    } else {
        iced::widget::image(iced::widget::image::Handle::from_path(path))
            .width(size)
            .height(size)
            .into()
    }
}

/// One renderer handle per distinct image (by content id), so pixels re-read by a service are
/// not uploaded again.
pub fn pixels(p: &crate::services::notifications::Pixels) -> iced::widget::image::Handle {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<u64, iced::widget::image::Handle>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let Ok(mut map) = cache.lock() else {
        return iced::widget::image::Handle::from_rgba(p.width, p.height, p.data.as_ref().clone());
    };
    if map.len() > 256 {
        map.clear();
    }
    map.entry(p.id)
        .or_insert_with(|| {
            iced::widget::image::Handle::from_rgba(p.width, p.height, p.data.as_ref().clone())
        })
        .clone()
}

/// The icon for a weather condition (WMO code).
pub fn weather_icon(w: &crate::services::web::Weather) -> Icon {
    match w.code {
        0 | 1 if w.is_day => Icon::Sun,
        0 | 1 => Icon::Moon,
        2 => Icon::CloudSun,
        3 => Icon::Cloud,
        45 | 48 => Icon::CloudFog,
        51..=67 | 80..=82 => Icon::CloudRain,
        71..=77 | 85 | 86 => Icon::Snowflake,
        95..=99 => Icon::CloudLightning,
        _ => Icon::Cloud,
    }
}
