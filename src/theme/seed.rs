//! Finds a wallpaper's characteristic colour: the hue that the most colourful, well-lit pixels
//! agree on, and how saturated that hue is.

use super::Seed;
use super::color::Oklch;
use std::path::Path;

const BUCKETS: usize = 36;

pub fn seed_from_image(path: &Path) -> Option<Seed> {
    let img = image::open(path).ok()?.thumbnail(96, 96).to_rgb8();
    let mut weight = [0f32; BUCKETS];
    let mut hue_x = [0f32; BUCKETS];
    let mut hue_y = [0f32; BUCKETS];
    let mut chroma = [0f32; BUCKETS];
    let mut total = 0f32;
    for p in img.pixels() {
        let c = Oklch::from_srgb(
            f32::from(p[0]) / 255.0,
            f32::from(p[1]) / 255.0,
            f32::from(p[2]) / 255.0,
        );
        total += 1.0;
        // Ignore near-black, near-white and grey pixels; favour mid lightness.
        if c.c < 0.03 || c.l < 0.2 || c.l > 0.95 {
            continue;
        }
        let w = c.c * (1.0 - (c.l - 0.62).abs());
        let b = ((c.h / 360.0 * BUCKETS as f32) as usize).min(BUCKETS - 1);
        weight[b] += w;
        hue_x[b] += w * c.h.to_radians().cos();
        hue_y[b] += w * c.h.to_radians().sin();
        chroma[b] += w * c.c;
    }
    if total == 0.0 {
        return None;
    }
    // Score each bucket together with its neighbours, so a hue split across a boundary counts.
    let score = |i: usize| {
        weight[(i + BUCKETS - 1) % BUCKETS] * 0.5 + weight[i] + weight[(i + 1) % BUCKETS] * 0.5
    };
    let best = (0..BUCKETS).max_by(|&a, &b| score(a).total_cmp(&score(b)))?;
    // Colour must cover a meaningful part of the picture, otherwise it is a grey wallpaper.
    let coverage = score(best) / total;
    let idx = [(best + BUCKETS - 1) % BUCKETS, best, (best + 1) % BUCKETS];
    let (x, y, w, c): (f32, f32, f32, f32) = idx.iter().fold((0., 0., 0., 0.), |acc, &i| {
        (
            acc.0 + hue_x[i],
            acc.1 + hue_y[i],
            acc.2 + weight[i],
            acc.3 + chroma[i],
        )
    });
    Some(Seed {
        hue: y.atan2(x).to_degrees().rem_euclid(360.0),
        chroma: if w > 0.0 { c / w } else { 0.0 },
        coverage,
    })
}
