//! Pictures for the kit: files and app icons drawn sharp at their size (SVG rasterised, PNGs
//! scaled once), rounded and fitted copies kept so a list never decodes twice, and
//! notification pixel data.

use super::*;

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

/// An image file (SVG or raster) at a square size.
pub fn image_file<'a, M: 'a>(path: &std::path::Path, size: f32) -> Element<'a, M> {
    image_file_faded(path, size, 1.0)
}

/// An app's icon by its window class or name (from its desktop entry), at `px`; without one,
/// `fallback` drawn a little smaller in the dim ink.
pub fn app_icon<'a, M: 'a>(
    s: crate::theme::Scheme,
    class: &str,
    px: f32,
    fallback: Icon,
) -> Element<'a, M> {
    use crate::services::{apps, icons};
    match icons::lookup(&apps::icon_for_class(class)) {
        Some(p) => image_file_faded(&p, px, s.opacity),
        None => icon(fallback, (px * 0.85).round(), s.ink_dim).into(),
    }
}

/// `image_file` at `opacity` (a panel's fade: `Scheme::opacity`).
pub fn image_file_faded<'a, M: 'a>(
    path: &std::path::Path,
    size: f32,
    opacity: f32,
) -> Element<'a, M> {
    if let Some(h) = sharp_file(path, size) {
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
pub(super) const PICTURE_SCALE: f32 = 1.0;

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
/// An SVG file rendered to `size` logical px (aspect kept, centred in a square).
pub fn svg_pixels(path: &std::path::Path, size: f32) -> Option<iced::widget::image::Handle> {
    use resvg::{tiny_skia, usvg};
    let data = std::fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
    let px = (size * PICTURE_SCALE).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let s = tree.size();
    let k = px as f32 / s.width().max(s.height());
    let (dx, dy) = (
        (px as f32 - s.width() * k) / 2.0,
        (px as f32 - s.height() * k) / 2.0,
    );
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(k, 0.0, 0.0, k, dx, dy),
        &mut pixmap.as_mut(),
    );
    // tiny-skia keeps premultiplied alpha; images take straight RGBA.
    let mut rgba = pixmap.take();
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(iced::widget::image::Handle::from_rgba(px, px, rgba))
}

/// Makes `path`'s picture at `size` ahead of time (off the UI thread), so drawing it later
/// is only a cache look-up.
pub fn warm_picture(path: &std::path::Path, size: f32) {
    let _ = sharp_file(path, size);
}

pub fn sharp_file(path: &std::path::Path, size: f32) -> Option<iced::widget::image::Handle> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    type Key = (std::path::PathBuf, u32);
    static CACHE: LazyLock<Mutex<HashMap<Key, Option<iced::widget::image::Handle>>>> =
        LazyLock::new(Default::default);
    let key = (path.to_path_buf(), size as u32);
    if let Some(hit) = CACHE.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let made = if path.extension().is_some_and(|e| e == "svg" || e == "svgz") {
        // Drawn once at the size it shows, then only those pixels are kept: the renderer's own
        // SVG path held every parsed icon for good (a panel full of app icons left ~8 MB).
        svg_pixels(path, size)
    } else {
        ::image::ImageReader::open(path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.decode().ok())
            .map(|i| sharp_image(&i.to_rgba8(), size))
    };
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

/// An image file shown at `w` × `h` with rounded corners (see `rounded_cover`), decoded once
/// per file and size.
pub(super) type RoundedKey = (std::path::PathBuf, u32, u32, u32);

pub(super) static ROUNDED: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<RoundedKey, Option<iced::widget::image::Handle>>>,
> = std::sync::LazyLock::new(Default::default);

/// Forgets the pictures made from `path` (the file changed under the same name).
pub fn forget_picture(path: &std::path::Path) {
    if let Ok(mut c) = ROUNDED.lock() {
        c.retain(|(p, ..), _| p != path);
    }
}

pub fn rounded_file(
    path: &std::path::Path,
    w: f32,
    h: f32,
    radius: f32,
) -> Option<iced::widget::image::Handle> {
    let key = (path.to_path_buf(), w as u32, h as u32, radius as u32);
    if let Some(hit) = ROUNDED.lock().ok()?.get(&key) {
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
    let mut cache = ROUNDED.lock().ok()?;
    // Capped by size: ~24 MB of pictures at most, then it starts over.
    let bytes: u64 = cache
        .iter()
        .map(|((_, w, h, _), _)| u64::from(*w) * u64::from(*h) * 4)
        .sum();
    if bytes > 24 * 1024 * 1024 {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

/// A picture file shown whole (never cropped) inside `max_w` × `max_h`, its corners rounded:
/// the handle and the size it takes. For previews where the whole picture matters (a
/// screenshot in the clipboard).
pub(super) type FittedKey = (std::path::PathBuf, u32, u32);

pub(super) type FittedHit = Option<(iced::widget::image::Handle, f32, f32)>;

pub(super) static FITTED: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<FittedKey, FittedHit>>,
> = std::sync::LazyLock::new(Default::default);

/// `fitted_file`, only if it is already made (never decodes: safe to call from `view`).
/// `None` = not made yet; `Some(None)` = made, and the file could not be read.
pub fn fitted_file_ready(path: &std::path::Path, max_w: f32, max_h: f32) -> Option<FittedHit> {
    FITTED
        .lock()
        .ok()?
        .get(&(path.to_path_buf(), max_w as u32, max_h as u32))
        .cloned()
}

pub fn fitted_file(
    path: &std::path::Path,
    max_w: f32,
    max_h: f32,
    radius: f32,
) -> Option<(iced::widget::image::Handle, f32, f32)> {
    static CACHE: &std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<FittedKey, FittedHit>>,
    > = &FITTED;
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
    // Capped by size, not count: a clipboard preview is up to ~1 MB, a row thumbnail ~10 KB.
    // At most ~24 MB of pictures; then it starts over (pictures are cheap to make again).
    let bytes: f32 = cache
        .values()
        .filter_map(Option::as_ref)
        .map(|(_, w, h)| w * h * 4.0)
        .sum();
    if bytes > 24.0 * 1024.0 * 1024.0 {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}
