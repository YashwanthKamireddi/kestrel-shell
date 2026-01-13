//! OKLCH colour maths. OKLCH keeps lightness perceptually even, so every role in the scheme can
//! be defined by a fixed lightness and a chroma, whatever hue the wallpaper brings.

use iced::Color;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    /// Degrees, 0..360.
    pub h: f32,
}

fn to_linear(u: f32) -> f32 {
    if u <= 0.040_45 {
        u / 12.92
    } else {
        ((u + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(u: f32) -> f32 {
    if u <= 0.003_130_8 {
        u * 12.92
    } else {
        1.055 * u.powf(1.0 / 2.4) - 0.055
    }
}

impl Oklch {
    pub const fn new(l: f32, c: f32, h: f32) -> Self {
        Self { l, c, h }
    }

    /// From sRGB components in 0..1.
    pub fn from_srgb(r: f32, g: f32, b: f32) -> Self {
        let (r, g, b) = (to_linear(r), to_linear(g), to_linear(b));
        let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b;
        let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
        let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
        let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
        let ll = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        let c = (a * a + bb * bb).sqrt();
        let h = bb.atan2(a).to_degrees().rem_euclid(360.0);
        Self { l: ll, c, h }
    }

    /// Linear sRGB, possibly out of gamut.
    fn to_linear_rgb(self) -> (f32, f32, f32) {
        let (a, b) = (
            self.c * self.h.to_radians().cos(),
            self.c * self.h.to_radians().sin(),
        );
        let l = self.l + 0.396_337_78 * a + 0.215_803_76 * b;
        let m = self.l - 0.105_561_346 * a - 0.063_854_17 * b;
        let s = self.l - 0.089_484_18 * a - 1.291_485_5 * b;
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        (
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        )
    }

    fn in_gamut(self) -> bool {
        let (r, g, b) = self.to_linear_rgb();
        let ok = |v: f32| (-1e-4..=1.0 + 1e-4).contains(&v);
        ok(r) && ok(g) && ok(b)
    }

    /// The nearest displayable colour: same lightness and hue, chroma reduced until it fits.
    pub fn to_color(self) -> Color {
        let mut c = self;
        if !c.in_gamut() {
            let (mut lo, mut hi) = (0.0, self.c);
            for _ in 0..20 {
                let mid = (lo + hi) / 2.0;
                if (Oklch { c: mid, ..self }).in_gamut() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            c.c = lo;
        }
        let (r, g, b) = c.to_linear_rgb();
        Color::from_rgb(
            from_linear(r.clamp(0.0, 1.0)),
            from_linear(g.clamp(0.0, 1.0)),
            from_linear(b.clamp(0.0, 1.0)),
        )
    }
}

/// Relative luminance contrast ratio (WCAG) between two opaque colours.
#[cfg(test)]
pub fn contrast(a: Color, b: Color) -> f32 {
    let lum =
        |c: Color| 0.2126 * to_linear(c.r) + 0.7152 * to_linear(c.g) + 0.0722 * to_linear(c.b);
    let (x, y) = (lum(a), lum(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// `top` with `alpha`, composited over `bottom`.
pub fn mix(bottom: Color, top: Color, alpha: f32) -> Color {
    Color {
        r: bottom.r + (top.r - bottom.r) * alpha,
        g: bottom.g + (top.g - bottom.g) * alpha,
        b: bottom.b + (top.b - bottom.b) * alpha,
        a: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for (r, g, b) in [
            (0.2, 0.5, 0.8),
            (1.0, 1.0, 1.0),
            (0.9, 0.1, 0.3),
            (0.0, 0.0, 0.0),
        ] {
            let c = Oklch::from_srgb(r, g, b).to_color();
            assert!((c.r - r).abs() < 2e-3 && (c.g - g).abs() < 2e-3 && (c.b - b).abs() < 2e-3);
        }
    }

    #[test]
    fn out_of_gamut_is_clamped_not_garbage() {
        let c = Oklch::new(0.8, 0.4, 140.0).to_color();
        for v in [c.r, c.g, c.b] {
            assert!((0.0..=1.0).contains(&v));
        }
        let back = Oklch::from_srgb(c.r, c.g, c.b);
        assert!((back.l - 0.8).abs() < 0.01);
    }
}
