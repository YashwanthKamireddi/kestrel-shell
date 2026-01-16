//! kestrel's tokens: colour roles, type, spacing and shape (see docs/DESIGN.md).

pub mod color;
mod seed;

use color::Oklch;
use iced::{Color, Font, font};

pub use seed::seed_from_image;

/// The wallpaper's character: a hue and how colourful it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seed {
    pub hue: f32,
    pub chroma: f32,
    /// How much of the picture carries that colour (weighted share of pixels).
    pub coverage: f32,
}

impl Default for Seed {
    fn default() -> Self {
        Self {
            hue: 255.0,
            chroma: 0.06,
            coverage: 0.1,
        }
    }
}

/// Every colour the UI may use. Nothing else picks colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scheme {
    pub base: Color,
    pub raised: Color,
    pub raised_hi: Color,
    pub raised_press: Color,
    pub line: Color,
    pub ink: Color,
    pub ink_dim: Color,
    pub ink_faint: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub accent_soft: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
}

impl Scheme {
    pub fn dark(seed: Seed) -> Self {
        // Surfaces take the wallpaper's hue only when the picture is broadly that colour; a
        // grey picture with one red stripe gets neutral surfaces and a red accent.
        let h = seed.hue;
        let has_colour = seed.chroma >= 0.02 && seed.coverage > 0.0008;
        let c = if has_colour { seed.chroma } else { 0.0 };
        let tint = (c / 0.1).min(1.0) * (seed.coverage / 0.03).clamp(0.0, 1.0);
        let n = |l: f32, k: f32| Oklch::new(l, k * tint, h).to_color();
        let accent_c = if has_colour { c.clamp(0.09, 0.16) } else { 0.0 };
        Self {
            base: n(0.16, 0.010),
            raised: n(0.215, 0.013),
            raised_hi: n(0.25, 0.015),
            raised_press: n(0.28, 0.016),
            line: n(0.30, 0.012),
            ink: n(0.95, 0.006),
            ink_dim: n(0.74, 0.010),
            ink_faint: n(0.54, 0.010),
            accent: Oklch::new(if has_colour { 0.76 } else { 0.90 }, accent_c, h).to_color(),
            on_accent: Oklch::new(0.20, accent_c * 0.3, h).to_color(),
            accent_soft: Oklch::new(0.30, accent_c * 0.4, h).to_color(),
            good: Oklch::new(0.80, 0.13, 150.0).to_color(),
            warn: Oklch::new(0.84, 0.13, 85.0).to_color(),
            bad: Oklch::new(0.70, 0.17, 25.0).to_color(),
        }
    }
}

impl Scheme {
    /// Every colour faded towards `base` by `alpha`: on an opaque panel this is the same as
    /// content opacity, without an offscreen layer.
    pub fn faded(self, alpha: f32) -> Self {
        if alpha >= 1.0 {
            return self;
        }
        let f = |c: Color| color::mix(self.base, c, alpha.max(0.0));
        Self {
            base: self.base,
            raised: f(self.raised),
            raised_hi: f(self.raised_hi),
            raised_press: f(self.raised_press),
            line: f(self.line),
            ink: f(self.ink),
            ink_dim: f(self.ink_dim),
            ink_faint: f(self.ink_faint),
            accent: f(self.accent),
            on_accent: f(self.on_accent),
            accent_soft: f(self.accent_soft),
            good: f(self.good),
            warn: f(self.warn),
            bad: f(self.bad),
        }
    }
}

impl Default for Scheme {
    fn default() -> Self {
        Self::dark(Seed::default())
    }
}

/// Type scale.
pub mod text {
    pub const CAPTION: f32 = 11.0;
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 13.0;
    pub const TITLE: f32 = 14.0;
    pub const HEADING: f32 = 16.0;
    pub const DISPLAY: f32 = 32.0;
}

/// The shell's typeface (also given to Qt apps and the lock screen).
pub const FONT_NAME: &str = "Rubik";
pub const FONT: Font = Font::with_name(FONT_NAME);

pub fn font(weight: font::Weight) -> Font {
    Font { weight, ..FONT }
}

pub const MEDIUM: font::Weight = font::Weight::Medium;
pub const SEMIBOLD: font::Weight = font::Weight::Semibold;

/// Spacing and shape.
pub mod size {
    pub const BAR: f32 = 36.0;
    pub const CONTROL: f32 = 28.0;
    pub const PANEL_RADIUS: f32 = 24.0;
    pub const PANEL_PAD: f32 = 12.0;
    /// Concave curve where a panel leaves the bar.
    pub const JOIN: f32 = 16.0;
    pub const CARD_RADIUS: f32 = PANEL_RADIUS - PANEL_PAD;
    pub const ROW_RADIUS: f32 = 10.0;
    pub const CONTROL_RADIUS: f32 = 8.0;
    pub const ICON: f32 = 16.0;
    pub const ICON_TILE: f32 = 18.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use color::contrast;

    #[test]
    fn text_stays_readable_for_every_hue() {
        for hue in (0..360).step_by(15) {
            for chroma in [0.0, 0.05, 0.12, 0.3] {
                let s = Scheme::dark(Seed {
                    hue: hue as f32,
                    chroma,
                    coverage: 0.05,
                });
                assert!(contrast(s.ink, s.base) > 12.0);
                assert!(contrast(s.ink, s.raised) > 10.0);
                assert!(contrast(s.ink_dim, s.raised) > 5.0, "hue {hue}");
                assert!(contrast(s.ink_faint, s.base) > 2.5);
                assert!(
                    contrast(s.on_accent, s.accent) > 6.0,
                    "hue {hue} c {chroma}"
                );
                assert!(contrast(s.accent, s.base) > 7.0);
            }
        }
    }
}
