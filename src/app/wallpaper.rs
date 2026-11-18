//! The wallpaper: reading and scaling the picture off the UI thread, and its cache. The
//! picture is kept while shown (it was let go after a guessed 3 s to save 8 MB, which left the
//! GPU renderer, that uploads pictures in the background, with nothing to draw).

use super::*;

/// Where the screen-sized copy of `path` is kept: named by the file, its size and its time, so
/// an edited or replaced picture is never shown stale.
pub(crate) fn wallpaper_cache_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(path).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok()?.hash(&mut h);
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".cache")))?;
    Some(
        base.join("kestrel")
            .join(format!("wallpaper-{:016x}.rgba", h.finish())),
    )
}

/// The cached copy: "KWP2", width, height, the colour seed (three f32 and the source colour as
/// a u32, 0xFFFFFFFF for none), then RGBA pixels.
pub(crate) fn wallpaper_cache_read(
    path: &std::path::Path,
) -> Option<(image::DynamicImage, crate::theme::Seed)> {
    let bytes = std::fs::read(wallpaper_cache_path(path)?).ok()?;
    let rest = bytes.strip_prefix(b"KWP2")?;
    let u = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(rest.get(i..i + 4)?.try_into().ok()?))
    };
    let f = |i: usize| -> Option<f32> {
        Some(f32::from_le_bytes(rest.get(i..i + 4)?.try_into().ok()?))
    };
    let (w, h) = (u(0)?, u(4)?);
    let seed = crate::theme::Seed {
        hue: f(8)?,
        chroma: f(12)?,
        coverage: f(16)?,
        source: Some(u(20)?).filter(|&s| s != u32::MAX),
    };
    let px = rest.get(24..)?.to_vec();
    let img = image::RgbaImage::from_raw(w, h, px)?;
    Some((image::DynamicImage::ImageRgba8(img), seed))
}

pub(crate) fn wallpaper_cache_write(
    path: &std::path::Path,
    img: &image::DynamicImage,
    seed: crate::theme::Seed,
) {
    let Some(file) = wallpaper_cache_path(path) else {
        return;
    };
    let Some(dir) = file.parent() else {
        return;
    };
    // One wallpaper's copy at a time: older ones go.
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name();
            if n.to_string_lossy().starts_with("wallpaper-") && e.path() != file {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let rgba = img.to_rgba8();
    let mut out = Vec::with_capacity(24 + rgba.len());
    out.extend_from_slice(b"KWP2");
    out.extend_from_slice(&rgba.width().to_le_bytes());
    out.extend_from_slice(&rgba.height().to_le_bytes());
    for v in [seed.hue, seed.chroma, seed.coverage] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&seed.source.unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(rgba.as_raw());
    let _ = std::fs::create_dir_all(dir);
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, out).is_ok() {
        let _ = std::fs::rename(tmp, file);
    }
}

pub(crate) fn load_wallpaper(path: Option<String>) -> Task<Message> {
    let Some(path) = path else {
        return Task::none();
    };
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let path = config::expand(&path);
                // The screen-sized picture and its colour are kept in the cache, so the
                // wallpaper is up as fast as the bar (decoding and scaling a 4K photo took
                // about a second on every start).
                let (img, seed) = match wallpaper_cache_read(&path) {
                    Some(hit) => hit,
                    None => {
                        // Decode once, cover 1920x1080, and keep only that: a 5K original
                        // would otherwise hold ~60 MB.
                        let img = image::open(&path).ok()?.resize_to_fill(
                            1920,
                            1080,
                            image::imageops::FilterType::CatmullRom,
                        );
                        let seed = crate::theme::seed_from_image(&img)?;
                        wallpaper_cache_write(&path, &img, seed);
                        (img, seed)
                    }
                };
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                let handle = iced::widget::image::Handle::from_rgba(w, h, rgba.into_raw());
                // Give the decoder's large temporary buffers back to the system.
                // Safety: malloc_trim only releases free heap memory.
                unsafe {
                    libc::malloc_trim(0);
                }
                Some((seed, Arc::new(Wallpaper { handle })))
            })
            .await
            .ok()
            .flatten()
        },
        Message::WallpaperLoaded,
    )
}
