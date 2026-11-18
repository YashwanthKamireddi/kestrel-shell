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
    /// The wallpaper's source colour as Material You picks it (quantized and scored, the
    /// material-color-utilities way), 0xRRGGBB; the scheme is built from it when present.
    pub source: Option<u32>,
}

impl Default for Seed {
    fn default() -> Self {
        Self {
            hue: 255.0,
            chroma: 0.06,
            coverage: 0.1,
            source: None,
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
    /// Blue: connected, informational (Wi-Fi with a good signal, a Bluetooth device).
    pub info: Color,
    /// How far content has faded in (1 = fully shown); see `faded`.
    pub opacity: f32,
}

impl Scheme {
    pub fn dark(seed: Seed) -> Self {
        // A wallpaper with (next to) no colour gets a neutral accent: Material's colour picker
        // falls back to Google's blue when a picture has none, which gave a black-and-white
        // wallpaper a blue shell.
        // Colour has to cover some of the picture too: a monochrome photo with a little yellow
        // lettering has saturated pixels but no colour to speak of.
        if let Some(src) = seed.source
            && seed.chroma >= 0.02
            && seed.coverage > 0.0008
        {
            return Self::material(src);
        }
        Self::oklch(seed)
    }

    /// Material 3's dark scheme from the wallpaper's source colour (Vibrant variant, spec 2025):
    /// tone ladders made for contrast, an accent that stays clear on any wallpaper, surfaces
    /// tinted towards it. The base stays true black (AMOLED, his call).
    fn material(source: u32) -> Self {
        use material_colors::color::Rgb;
        use material_colors::dynamic_color::Variant;
        use material_colors::dynamic_color::color_spec::SpecVersion;
        use material_colors::theme::ThemeBuilder;
        let theme = ThemeBuilder::with_source(Rgb::from_u32(source))
            .variant(Variant::Vibrant)
            .spec_version(SpecVersion::Spec2025)
            .build();
        let d = theme.schemes.dark;
        let c = |x: Rgb| Color::from_rgb8(x.red, x.green, x.blue);
        // Only the accent comes from the wallpaper. Text and surfaces stay neutral greys (his
        // call 2026-11-13: tinted text and surfaces read as too much colour).
        let neutral = Self::oklch(Seed {
            chroma: 0.0,
            ..Seed::default()
        });
        let accent = c(d.primary);
        // Soft accent: the accent a little way into the dark surface (Material's container is
        // too bright to put light text on with vivid sources like yellow).
        Self {
            accent,
            on_accent: c(d.on_primary),
            accent_soft: color::mix(neutral.raised, accent, 0.28),
            ..neutral
        }
    }

    fn oklch(seed: Seed) -> Self {
        // Surfaces take the wallpaper's hue only when the picture is broadly that colour; a
        // grey picture with one red stripe gets neutral surfaces and a red accent.
        let h = seed.hue;
        let has_colour = seed.chroma >= 0.02 && seed.coverage > 0.0008;
        let c = if has_colour { seed.chroma } else { 0.0 };
        let tint = (c / 0.1).min(1.0) * (seed.coverage / 0.03).clamp(0.0, 1.0);
        let n = |l: f32, k: f32| Oklch::new(l, k * tint, h).to_color();
        // A vivid accent in the wallpaper's hue: a pale wallpaper's own saturation made a
        // washed-out, muddy accent (his call 2026-11-13: "dull and cheap"). The hue is the
        // wallpaper's; the strength is always clear (as strong as sRGB allows, up to 0.2).
        let accent_c = if has_colour {
            (c * 2.0).clamp(0.15, 0.2)
        } else {
            0.0
        };
        Self {
            // True black (AMOLED, his call 2026-11-13): pills and panels stand out against any
            // wallpaper; raised surfaces keep a slight lift so they still read as layers.
            base: Color::BLACK,
            raised: n(0.215, 0.013),
            raised_hi: n(0.25, 0.015),
            // Pressed: the hover overlay made a little stronger (libadwaita / Material state
            // layers), never an opaque grey block.
            raised_press: Color {
                a: 0.16,
                ..n(0.97, 0.006)
            },
            line: n(0.30, 0.012),
            ink: n(0.97, 0.006),
            ink_dim: n(0.84, 0.010),
            ink_faint: n(0.66, 0.012),
            accent: Oklch::new(if has_colour { 0.74 } else { 0.90 }, accent_c, h).to_color(),
            on_accent: Oklch::new(0.18, accent_c * 0.35, h).to_color(),
            accent_soft: Oklch::new(0.32, accent_c * 0.5, h).to_color(),
            good: Oklch::new(0.80, 0.13, 150.0).to_color(),
            warn: Oklch::new(0.84, 0.13, 85.0).to_color(),
            bad: Oklch::new(0.70, 0.17, 25.0).to_color(),
            info: Oklch::new(0.78, 0.12, 245.0).to_color(),
            opacity: 1.0,
        }
    }

    /// A bar control under the pointer: a faint neutral lift.
    pub fn hover(&self) -> Color {
        self.ink.scale_alpha(0.07 * self.opacity)
    }

    /// A bar control whose panel is open: a clear neutral fill. Colour is kept for
    /// indicators (the current workspace, a level), never for "this is selected".
    pub fn selected(&self) -> Color {
        self.ink.scale_alpha(0.14 * self.opacity)
    }

    /// Any other colour (artwork tints, a weather sky) faded in step with the scheme.
    pub fn fade(&self, c: Color) -> Color {
        if self.opacity >= 1.0 {
            c
        } else {
            c.scale_alpha(self.opacity.max(0.0))
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
        // True transparency: panels fade over whatever is behind them.
        let f = |c: Color| c.scale_alpha(alpha.clamp(0.0, 1.0));
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
            info: f(self.info),
            opacity: alpha.max(0.0),
        }
    }
}

/// The pills and panels are see-through glass (config `material` is not "black").
static GLASS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_glass(on: bool) {
    GLASS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn glass() -> bool {
    GLASS.load(std::sync::atomic::Ordering::Relaxed)
}

/// A pill-shaped thing's own fill: the black in the Black look, none when frosted (the frost
/// under the pill shows through).
pub fn surface(s: Scheme) -> iced::Color {
    if glass() {
        iced::Color::TRANSPARENT
    } else {
        s.base
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
}

/// Rubik at fixed weights, cut from the variable font (OFL, assets/fonts/LICENSE-rubik).
pub const RUBIK_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Rubik-Medium.ttf");
pub const RUBIK_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Rubik-SemiBold.ttf");
pub const RUBIK_BOLD: &[u8] = include_bytes!("../../assets/fonts/Rubik-Bold.ttf");

/// The shell's typeface (also given to Qt apps and the lock screen).
pub const FONT_NAME: &str = "Rubik";
/// Medium by default, like the lock screen: the regular weight looked thin and grainy at bar
/// sizes (his call, 2026-11-13). Headings go to Semibold.
pub const FONT: Font = Font {
    weight: font::Weight::Medium,
    ..Font::with_name(FONT_NAME)
};

pub fn font(weight: font::Weight) -> Font {
    Font { weight, ..FONT }
}

/// The design kit's tokens (docs/DESIGN.md). Panels use these names, never numbers.
#[allow(dead_code)] // the whole scale, in use as panels move onto it
pub mod tok {
    // Spacing: the 4 px grid (with 2 and 6 for tight optical fixes).
    pub const XXS: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const S: f32 = 6.0;
    pub const M: f32 = 8.0;
    pub const L: f32 = 12.0;
    pub const XL: f32 = 16.0;
    pub const XXL: f32 = 20.0;
    pub const XXXL: f32 = 24.0;
    /// A row's text lined up under the text of a row with an icon (icon 16 + gap 12).
    pub const INDENT: f32 = 28.0;
    /// A one-pixel frame around a picture.
    pub const HAIR: f32 = 1.0;

    // Radii, concentric: a panel (24) padded 12 holds cards of 12; rows 10; small controls 8.
    // A pill is half its height (`pill`).
    pub const R_PANEL: f32 = 24.0;
    pub const R_CARD: f32 = 12.0;
    /// Where cards of a grouped list meet (Android 16, DMS groupedListInnerRadius).
    pub const R_JOIN: f32 = 4.0;
    pub const R_ROW: f32 = 10.0;
    pub const R_CONTROL: f32 = 8.0;

    /// Fully round, whatever the height (controls cap a radius at half their height).
    pub const FULL: f32 = 999.0;
    /// No space.
    pub const NONE: f32 = 0.0;

    /// Fully round for something `h` tall.
    pub const fn pill(h: f32) -> f32 {
        h / 2.0
    }

    // Control heights.
    pub const H_SMALL: f32 = 28.0;
    pub const H_CONTROL: f32 = 32.0;
    pub const H_ROW: f32 = 40.0;
    /// A row with a second line under its name (the launcher's results).
    pub const H_ROW2: f32 = 48.0;

    // The state layer: ink laid over a container.
    pub const HOVER: f32 = 0.08;
    pub const PRESS: f32 = 0.14;
    pub const SELECTED: f32 = 0.14;
    pub const DISABLED: f32 = 0.38;
    /// A tonal container at rest (a quiet filled button, a chip).
    pub const TONAL: f32 = 0.06;
    /// A track or tile's fill (slider and meter tracks, picture placeholders).
    pub const FILL: f32 = 0.08;
    /// A fill that must read against a tonal one (an icon's circle on a card).
    pub const FILL_STRONG: f32 = 0.10;
    /// A selection ring's ink (the overview's chosen window: neutral, never a colour).
    pub const RING: f32 = 0.55;

    // Motion, in seconds.
    pub const HOVER_IN: f32 = 0.07;
    pub const HOVER_OUT: f32 = 0.12;

    // Surfaces.
    /// The only panel widths (Apple-style: a few fixed sizes, not one per panel): a small menu
    /// (power, tray, privacy, AirPods), a panel (control centre, dashboard, weather, music,
    /// capture, system), the launcher, and the wide ones (clipboard, shortcuts, files).
    pub const PANEL_S: f32 = 320.0;
    pub const PANEL_M: f32 = 400.0;
    pub const PANEL_L: f32 = 560.0;
    pub const PANEL_XL: f32 = 860.0;
    /// A panel's inner padding (a card inside it is R_PANEL − PANEL_PAD = R_CARD round).
    pub const PANEL_PAD: f32 = 12.0;
    pub const BAR: f32 = 40.0;
    /// A round item in the bar (a status icon, the tray).
    pub const BAR_ITEM: f32 = 30.0;
    /// Concave curve where a panel leaves the bar.
    pub const JOIN: f32 = 16.0;

    // Icons: one scale.
    pub const ICON_XS: f32 = 12.0;
    pub const ICON_S: f32 = 14.0;
    pub const ICON: f32 = 16.0;
    pub const ICON_M: f32 = 18.0;
    pub const ICON_L: f32 = 20.0;
    pub const ICON_XL: f32 = 22.0;
    pub const ICON_XXL: f32 = 28.0;
    /// The one big glyph of an empty or waiting panel's centre.
    pub const ICON_HERO: f32 = 40.0;
    /// The weather panel's condition, beside the temperature it goes with.
    pub const ICON_DISPLAY: f32 = 52.0;
}

pub const MEDIUM: font::Weight = font::Weight::Medium;
pub const SEMIBOLD: font::Weight = font::Weight::Semibold;

#[cfg(test)]
mod tests {
    use super::*;
    use color::contrast;

    #[test]
    fn material_schemes_stay_readable() {
        for source in [
            0xff_3355, 0x33_cc66, 0x2266ff, 0xffcc00, 0x808080, 0x111111, 0xfafafa, 0xe991b0,
        ] {
            let s = Scheme::dark(Seed {
                source: Some(source),
                chroma: 0.1,
                ..Seed::default()
            });
            assert!(contrast(s.ink, s.base) > 12.0, "{source:06x}");
            assert!(contrast(s.ink_dim, s.raised) > 5.0, "{source:06x}");
            assert!(contrast(s.on_accent, s.accent) > 4.5, "{source:06x}");
            assert!(contrast(s.accent, s.base) > 6.0, "{source:06x}");
        }
    }

    #[test]
    fn a_monochrome_wallpaper_with_a_speck_of_colour_gets_a_neutral_accent() {
        let s = Scheme::dark(Seed {
            source: Some(0x4285f4),
            hue: 97.0,
            chroma: 0.036,
            coverage: 0.0,
        });
        let o = color::Oklch::from_srgb(s.accent.r, s.accent.g, s.accent.b);
        assert!(o.c < 0.03, "accent chroma {}", o.c);
    }

    #[test]
    fn a_colourless_wallpaper_gets_a_neutral_accent() {
        // Material's picker hands back Google blue for a grey picture; the accent must not.
        let s = Scheme::dark(Seed {
            source: Some(0x4285f4),
            chroma: 0.0,
            ..Seed::default()
        });
        let o = color::Oklch::from_srgb(s.accent.r, s.accent.g, s.accent.b);
        assert!(o.c < 0.03, "accent chroma {}", o.c);
    }

    #[test]
    fn text_stays_readable_for_every_hue() {
        for hue in (0..360).step_by(15) {
            for chroma in [0.0, 0.05, 0.12, 0.3] {
                let s = Scheme::dark(Seed {
                    hue: hue as f32,
                    chroma,
                    coverage: 0.05,
                    source: None,
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
