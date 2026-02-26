//! kestrel's component kit. Screens are assembled only from these.

pub mod control;
pub mod icon;
pub mod morph;
pub mod reveal;
pub mod ring;
pub mod shape;
pub mod slider;
pub mod snap;

use crate::theme::{self, Scheme, size};
use iced::widget::{Space, Text, container, row, text};
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

/// `s` on up to two lines of `width` px at `size` px text, broken between words; "…" only
/// if even two lines are not enough. For titles, which should be read, not cut.
pub fn two_lines(s: &str, size: f32, width: f32) -> String {
    let per = (width / (size * 0.56)).floor().max(4.0) as usize;
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut first = String::new();
    let mut i = 0;
    while i < words.len() {
        let next = if first.is_empty() {
            words[i].chars().count()
        } else {
            first.chars().count() + 1 + words[i].chars().count()
        };
        if next > per && !first.is_empty() {
            break;
        }
        if !first.is_empty() {
            first.push(' ');
        }
        first.push_str(words[i]);
        i += 1;
    }
    if i >= words.len() {
        return fit(&first, size, width);
    }
    let rest = words[i..].join(" ");
    format!("{}\n{}", fit(&first, size, width), fit(&rest, size, width))
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
    control(snap::snap(
        iced::widget::image(handle)
            .width(w)
            .height(h)
            .content_fit(iced::ContentFit::Cover)
            .border_radius(radius),
    ))
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

/// An icon in `color` (its alpha included, so it fades with a panel), `size` px square.
pub fn icon(i: Icon, size: f32, color: Color) -> iced::widget::Text<'static> {
    let t = iced::widget::text(i.glyph().to_string())
        .font(icon::FONT)
        .size(size)
        .color(color)
        .line_height(iced::widget::text::LineHeight::Relative(1.0))
        .width(size);
    t.height(size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
}

/// An icon that may show a level over its full shape (Wi-Fi bars over the dimmed fan).
pub fn level_icon<'a, M: 'a>(i: Icon, size: f32, color: Color) -> Element<'a, M> {
    match i.backdrop() {
        Some(b) => {
            iced::widget::stack![icon(b, size, color.scale_alpha(0.3)), icon(i, size, color)].into()
        }
        None => icon(i, size, color).into(),
    }
}

/// A round icon button for the bar and panel headers. `active` marks an open panel or an
/// enabled state.
pub fn icon_button<'a, M: Clone + 'static>(
    s: Scheme,
    i: Icon,
    on_press: M,
    active: bool,
) -> control::Control<'a, M> {
    icon_button_in(s, i, on_press, active, s.ink)
}

/// `icon_button` with the icon in `tint` (a colour that says something about the state).
pub fn icon_button_in<'a, M: Clone + 'static>(
    s: Scheme,
    i: Icon,
    on_press: M,
    active: bool,
    tint: Color,
) -> control::Control<'a, M> {
    let rest = if active {
        s.selected()
    } else {
        Color::TRANSPARENT
    };
    control(container(icon(i, size::ICON, tint)).center(Length::Fixed(size::CONTROL)))
        .radius(size::CONTROL / 2.0)
        .colors(rest, if active { rest } else { s.hover() }, s.raised_press)
        .on_press(on_press)
}

/// The card that groups related controls inside a panel.
/// A group of content in a panel. Flat: no grey box, no outline; groups are told apart by
/// space alone (design call, 2026-11-12).
pub fn card<'a, M: 'a>(_s: Scheme, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(size::PANEL_PAD)
        .width(Length::Fill)
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
/// Width of a quick-settings tile (two per row in the 368 px control centre).
pub const TILE_W: f32 = 180.0;

/// A quick-settings tile.
pub struct Tile<M> {
    pub icon: Icon,
    pub title: String,
    pub status: String,
    pub on: bool,
    /// Turns it on or off (its icon circle; the whole tile when it has no page).
    pub press: M,
    /// Its page, if it has one: a click anywhere on the tile but the icon opens it.
    pub details: Option<M>,
    /// The keyboard is on it.
    pub focused: bool,
}

pub const TILE_H: f32 = 60.0;

/// A tile: a calm rounded card, an icon in a circle that fills with the accent when the thing
/// is on, its name and state. No chevron to aim for: the circle switches it, the rest of the
/// card opens its page.
pub fn tile<'a, M: Clone + 'static>(s: Scheme, t: Tile<M>) -> Element<'a, M> {
    let Tile {
        icon: i,
        title,
        status,
        on,
        press,
        details,
        focused,
    } = t;
    const DOT: f32 = 38.0;
    let text_w = TILE_W - 10.0 - DOT - 12.0 - 12.0;
    let (dot_bg, dot_fg) = if on {
        (s.accent, s.on_accent)
    } else {
        (s.ink.scale_alpha(0.1 * s.opacity), s.ink)
    };
    let circle = control(container(level_icon(i, size::ICON_TILE, dot_fg)).center(DOT))
        .radius(DOT / 2.0)
        .colors(
            dot_bg,
            if on {
                dot_bg
            } else {
                s.ink.scale_alpha(0.16 * s.opacity)
            },
            if on {
                s.accent.scale_alpha(0.85)
            } else {
                s.raised_press
            },
        )
        .on_press(press.clone());
    let words = iced::widget::column![
        label_w(
            fit(&title, theme::text::BODY, text_w),
            theme::text::BODY,
            s.ink,
            theme::MEDIUM
        ),
        label(
            fit(&status, theme::text::CAPTION, text_w),
            theme::text::CAPTION,
            if on { s.ink } else { s.ink_dim }
        ),
    ]
    .spacing(1)
    .width(Length::Fill);
    let fill = s.ink.scale_alpha(0.06 * s.opacity);
    let body = control(row![circle, words].spacing(12).align_y(Alignment::Center))
        .padding(iced::Padding {
            top: 0.0,
            right: 12.0,
            bottom: 0.0,
            left: 11.0,
        })
        .height(TILE_H)
        .width(Length::Fill)
        .radius(TILE_H / 2.0)
        .colors(fill, s.ink.scale_alpha(0.1 * s.opacity), s.raised_press)
        .no_press_scale()
        .on_press(details.unwrap_or(press));
    if !focused {
        return body.into();
    }
    // Where the keyboard is: a fine white ring.
    iced::widget::stack![
        body,
        container(Space::new())
            .width(Length::Fill)
            .height(TILE_H)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: s.ink.scale_alpha(0.7 * s.opacity),
                    width: 1.5,
                    radius: (TILE_H / 2.0).into(),
                },
                ..container::Style::default()
            }),
    ]
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
        if selected { s.accent } else { s.ink },
        if selected {
            theme::MEDIUM
        } else {
            font::Weight::Medium
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
    // No box behind a row: the selected one (the network in use) is told by its name in the
    // accent; the pointer only lifts a row faintly.
    .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
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
    image_file_faded(path, size, 1.0)
}

/// `image_file` at `opacity` (a panel's fade: `Scheme::opacity`).
pub fn image_file_faded<'a, M: 'a>(
    path: &std::path::Path,
    size: f32,
    opacity: f32,
) -> Element<'a, M> {
    if path.extension().is_some_and(|e| e == "svg") {
        iced::widget::svg(iced::widget::svg::Handle::from_path(path))
            .width(size)
            .height(size)
            .opacity(opacity)
            .into()
    } else if let Some(h) = sharp_file(path, size) {
        snap::snap(
            iced::widget::image(h)
                .width(size)
                .height(size)
                .opacity(opacity),
        )
        .into()
    } else {
        // Unreadable: an empty square instead (the renderer aborts on undecodable pictures).
        Space::new().width(size).height(size).into()
    }
}

/// Device pixels per logical pixel to prepare pictures for: every screen here runs at scale 1,
/// and a picture made larger is shrunk by the renderer's plain filter (jagged round edges).
const PICTURE_SCALE: f32 = 1.0;

/// `img` resized to `size` logical px with a high-quality filter. The renderer scales
/// pictures with a plain bilinear filter, which turns a 256 px app icon drawn at 20 px into a
/// jagged, pixelated one; done here once, it stays smooth.
pub fn sharp_image(img: &::image::RgbaImage, size: f32) -> iced::widget::image::Handle {
    let px = (size * PICTURE_SCALE).round().max(1.0) as u32;
    let (w, h) = img.dimensions();
    let out = if w > px || h > px {
        let (tw, th) = if w >= h {
            (
                px,
                (u64::from(h) * u64::from(px) / u64::from(w)).max(1) as u32,
            )
        } else {
            (
                (u64::from(w) * u64::from(px) / u64::from(h)).max(1) as u32,
                px,
            )
        };
        ::image::imageops::resize(img, tw, th, ::image::imageops::FilterType::Lanczos3)
    } else {
        img.clone()
    };
    let (w, h) = out.dimensions();
    iced::widget::image::Handle::from_rgba(w, h, out.into_raw())
}

/// A picture file prepared by `sharp_image` (decoded by content, once per file and size).
/// None when it does not decode (the renderer aborts on undecodable pictures).
fn sharp_file(path: &std::path::Path, size: f32) -> Option<iced::widget::image::Handle> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    type Key = (std::path::PathBuf, u32);
    static CACHE: LazyLock<Mutex<HashMap<Key, Option<iced::widget::image::Handle>>>> =
        LazyLock::new(Default::default);
    let key = (path.to_path_buf(), size as u32);
    if let Some(hit) = CACHE.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let made = ::image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.decode().ok())
        .map(|i| sharp_image(&i.to_rgba8(), size));
    if let Ok(mut m) = CACHE.lock() {
        if m.len() > 512 {
            m.clear();
        }
        m.insert(key, made.clone());
    }
    made
}

/// One renderer handle per distinct image (by content id), so pixels re-read by a service are
/// not uploaded again.
/// `pixels`, prepared for drawing at `size` logical px (see `sharp_image`).
pub fn pixels_at(
    p: &crate::services::notifications::Pixels,
    size: f32,
) -> iced::widget::image::Handle {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    static CACHE: LazyLock<Mutex<HashMap<(u64, u32), iced::widget::image::Handle>>> =
        LazyLock::new(Default::default);
    let key = (p.id, size as u32);
    if let Some(h) = CACHE.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return h;
    }
    let made = ::image::RgbaImage::from_raw(p.width, p.height, p.data.as_ref().clone())
        .map_or_else(|| pixels(p), |img| sharp_image(&img, size));
    if let Ok(mut m) = CACHE.lock() {
        if m.len() > 256 {
            m.clear();
        }
        m.insert(key, made.clone());
    }
    made
}

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
    weather_icon_for(w.code, w.is_day)
}

/// The icon for WMO code `code`, by day or night (for forecasts).
pub fn weather_icon_for(code: u32, day: bool) -> Icon {
    match code {
        0 | 1 if day => Icon::Sun,
        0 | 1 => Icon::Moon,
        2 if day => Icon::CloudSun,
        2 => Icon::CloudMoon,
        3 => Icon::Cloud,
        45 | 48 => Icon::CloudFog,
        51..=57 => Icon::CloudDrizzle,
        61..=67 | 80..=82 => Icon::CloudRain,
        71..=77 | 85 | 86 => Icon::CloudSnow,
        95..=99 => Icon::CloudLightning,
        _ => Icon::Cloud,
    }
}

/// The weather's own colour: a warm sun, a pale moon, grey cloud, blue rain, violet storms.
pub fn weather_tint(w: &crate::services::web::Weather) -> Color {
    weather_tint_for(w.code, w.is_day)
}

/// `weather_tint` for WMO code `code`, by day or night (for forecasts).
pub fn weather_tint_for(code: u32, day: bool) -> Color {
    let rgb = |r, g, b| Color::from_rgb8(r, g, b);
    match code {
        0 | 1 if day => rgb(0xFF, 0xC2, 0x4D),
        0 | 1 => rgb(0xC8, 0xC4, 0xFF),
        2 if day => rgb(0xFF, 0xD7, 0x8A),
        2 => rgb(0xC5, 0xCC, 0xE8),
        45 | 48 | 3 => rgb(0xB8, 0xC0, 0xCC),
        51..=67 | 80..=82 => rgb(0x7C, 0xB8, 0xFF),
        71..=77 | 85 | 86 => rgb(0xE6, 0xF2, 0xFF),
        95..=99 => rgb(0xB9, 0x9C, 0xFF),
        _ => rgb(0xB8, 0xC0, 0xCC),
    }
}

/// A sky for the weather card: two colours, top to bottom, by condition and time of day.
pub fn weather_sky(w: &crate::services::web::Weather) -> (Color, Color) {
    let rgb = |r, g, b| Color::from_rgb8(r, g, b);
    match (w.code, w.is_day) {
        (0 | 1, true) => (rgb(0x2E, 0x6C, 0xC4), rgb(0x16, 0x2F, 0x5A)),
        (0 | 1, false) => (rgb(0x1A, 0x1F, 0x4A), rgb(0x0B, 0x0E, 0x22)),
        (2, true) => (rgb(0x3A, 0x62, 0x9A), rgb(0x1B, 0x2C, 0x48)),
        (2, false) => (rgb(0x22, 0x27, 0x45), rgb(0x0E, 0x11, 0x22)),
        (51..=67 | 80..=82, _) => (rgb(0x2B, 0x3A, 0x52), rgb(0x12, 0x18, 0x24)),
        (95..=99, _) => (rgb(0x2D, 0x24, 0x48), rgb(0x10, 0x0C, 0x1E)),
        (71..=77 | 85 | 86, _) => (rgb(0x50, 0x5E, 0x74), rgb(0x22, 0x2A, 0x36)),
        _ => (rgb(0x37, 0x40, 0x4E), rgb(0x18, 0x1C, 0x24)),
    }
}

/// `img` cropped to fill `w` × `h` (logical px), made at the screen's pixel size, with its
/// corners rounded in the pixels themselves (anti-aliased): the CPU renderer does not round
/// images, so this is what keeps artwork and thumbnails from showing square, cut-off corners.
pub fn rounded_cover(img: &::image::RgbaImage, w: f32, h: f32, radius: f32) -> ::image::RgbaImage {
    let (tw, th) = (
        (w * PICTURE_SCALE).round().max(1.0) as u32,
        (h * PICTURE_SCALE).round().max(1.0) as u32,
    );
    let (sw, sh) = img.dimensions();
    // Crop to the target's shape, centred.
    let (cw, ch) = if u64::from(sw) * u64::from(th) > u64::from(sh) * u64::from(tw) {
        ((u64::from(sh) * u64::from(tw) / u64::from(th)) as u32, sh)
    } else {
        (sw, (u64::from(sw) * u64::from(th) / u64::from(tw)) as u32)
    };
    let crop = ::image::imageops::crop_imm(img, (sw - cw) / 2, (sh - ch) / 2, cw.max(1), ch.max(1))
        .to_image();
    let mut out = ::image::imageops::resize(&crop, tw, th, ::image::imageops::FilterType::Triangle);
    let r = (radius * PICTURE_SCALE)
        .min(tw as f32 / 2.0)
        .min(th as f32 / 2.0);
    if r > 0.5 {
        for y in 0..th {
            for x in 0..tw {
                // Distance outside the rounded rectangle, measured from the nearest corner centre.
                let cx = (x as f32 + 0.5).clamp(r, tw as f32 - r);
                let cy = (y as f32 + 0.5).clamp(r, th as f32 - r);
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (r - d + 0.5).clamp(0.0, 1.0);
                if cover < 1.0 {
                    let px = out.get_pixel_mut(x, y);
                    px[3] = (f32::from(px[3]) * cover) as u8;
                }
            }
        }
    }
    out
}

/// Temperature as a colour: cool blue, mild white, warm amber, hot orange.
pub fn temperature_tint(c: f32) -> Color {
    let rgb = |r, g, b| Color::from_rgb8(r, g, b);
    match c {
        t if t < 12.0 => rgb(0x8E, 0xC5, 0xFF),
        t if t < 22.0 => rgb(0xD8, 0xF0, 0xFF),
        t if t < 30.0 => rgb(0xFF, 0xD0, 0x80),
        _ => rgb(0xFF, 0x9A, 0x6B),
    }
}

/// An image file shown at `w` × `h` with rounded corners (see `rounded_cover`), decoded once
/// per file and size.
pub fn rounded_file(
    path: &std::path::Path,
    w: f32,
    h: f32,
    radius: f32,
) -> Option<iced::widget::image::Handle> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    type Key = (std::path::PathBuf, u32, u32, u32);
    static CACHE: LazyLock<Mutex<HashMap<Key, Option<iced::widget::image::Handle>>>> =
        LazyLock::new(Default::default);
    let key = (path.to_path_buf(), w as u32, h as u32, radius as u32);
    if let Some(hit) = CACHE.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let made = ::image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.decode().ok())
        .filter(|img| img.width() > 0 && img.height() > 0)
        .map(|img| {
            let r = rounded_cover(&img.into_rgba8(), w, h, radius);
            iced::widget::image::Handle::from_rgba(r.width(), r.height(), r.into_raw())
        });
    let mut cache = CACHE.lock().ok()?;
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

/// A picture file shown whole (never cropped) inside `max_w` × `max_h`, its corners rounded:
/// the handle and the size it takes. For previews where the whole picture matters (a
/// screenshot in the clipboard).
pub fn fitted_file(
    path: &std::path::Path,
    max_w: f32,
    max_h: f32,
    radius: f32,
) -> Option<(iced::widget::image::Handle, f32, f32)> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    type Key = (std::path::PathBuf, u32, u32);
    type Hit = Option<(iced::widget::image::Handle, f32, f32)>;
    static CACHE: LazyLock<Mutex<HashMap<Key, Hit>>> = LazyLock::new(Default::default);
    let key = (path.to_path_buf(), max_w as u32, max_h as u32);
    if let Some(hit) = CACHE.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let made = ::image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.decode().ok())
        .filter(|img| img.width() > 0 && img.height() > 0)
        .map(|img| {
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            let k = (max_w / iw).min(max_h / ih);
            // At least a few pixels each way, so a thin strip stays visible.
            let (w, h) = ((iw * k).round().max(4.0), (ih * k).round().max(4.0));
            let r = rounded_cover(&img.into_rgba8(), w, h, radius.min(w / 2.0).min(h / 2.0));
            (
                iced::widget::image::Handle::from_rgba(r.width(), r.height(), r.into_raw()),
                w,
                h,
            )
        });
    let mut cache = CACHE.lock().ok()?;
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

#[cfg(test)]
mod text_tests {
    #[test]
    fn titles_break_between_words() {
        let t = super::two_lines(
            "Warriors vs. Rockets Reaction Houston rolling with",
            13.0,
            160.0,
        );
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(!lines[0].ends_with(' '));
        assert_eq!(super::two_lines("Short", 13.0, 160.0), "Short");
    }
}
