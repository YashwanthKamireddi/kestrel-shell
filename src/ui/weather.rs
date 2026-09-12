//! The weather's icons and colours: the condition's glyph and tint, the sky behind it, and a
//! temperature's colour.

use super::*;

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
