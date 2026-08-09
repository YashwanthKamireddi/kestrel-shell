//! Outputs, frames, measurements, timers and the wallpaper.

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Output(OutputEvent::Added(info)) => {
            log::info!("output added: {:?} {:?}", info.name, info.logical_size);
            let t = app.add_output(info);
            app.update_pills(true);
            Task::batch([t, app.wallpaper_back()])
        }
        Message::Output(OutputEvent::Removed(id)) => app.remove_output(id),
        Message::Output(OutputEvent::InfoChanged(info)) => {
            if let Some(o) = app.outputs.iter_mut().find(|o| o.id == info.id)
                && let Some((w, h)) = info.logical_size
            {
                o.info = info.clone();
                let size = Size::new(w as f32, h as f32);
                if o.size != size {
                    o.size = size;
                    // Re-apply the bar's size and input region for the new width.
                    o.plan = None;
                    // Its wallpaper surface will draw again: the picture is read back.
                    o.name = info.name;
                    return app.wallpaper_back();
                }
                o.name = info.name;
            }
            Task::none()
        }
        Message::Output(_) => Task::none(),
        Message::Second => {
            app.now = chrono::Local::now();
            Task::none()
        }
        Message::Minute => {
            app.now = chrono::Local::now();
            let night = app.night_schedule();
            app.quiet_schedule();
            // Do not disturb for a while: ends by itself at its time.
            if app.dnd
                && let Some(t) = crate::state::get(|s| s.toggles.dnd_until)
                && std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .is_ok_and(|d| d.as_secs() >= t)
            {
                app.dnd = false;
                crate::state::update(|s| {
                    s.toggles.dnd = false;
                    s.toggles.dnd_until = None;
                });
            }
            night
        }
        Message::Frame(now) => app.tick(now),
        Message::WallpaperLoaded(Some((seed, wall))) => {
            app.wallpaper_pending = false;
            log::info!(
                "wallpaper seed: hue {:.0} chroma {:.3} coverage {:.4}",
                seed.hue,
                seed.chroma,
                seed.coverage
            );
            app.seed = seed;
            let id = Arc::as_ptr(&wall) as usize;
            app.wallpaper = Some(wall);
            app.wallpaper_released = false;
            // Long enough for every output's wallpaper surface to have presented it.
            let release = Task::perform(tokio::time::sleep(Duration::from_secs(3)), move |()| {
                Message::WallpaperRelease(id)
            });
            Task::batch([app.recolour(), release])
        }
        Message::Trim => {
            // Safety: malloc_trim only releases free heap memory.
            unsafe {
                libc::malloc_trim(0);
            }
            Task::none()
        }
        Message::WallpaperRelease(id) => {
            if app
                .wallpaper
                .as_ref()
                .is_some_and(|w| Arc::as_ptr(w) as usize == id)
            {
                app.wallpaper = None;
                app.wallpaper_released = true;
                // Once the renderer's cached copy is gone too (next frame), the memory goes back.
                return Task::perform(tokio::time::sleep(Duration::from_millis(500)), |()| {
                    Message::Trim
                });
            }
            Task::none()
        }
        Message::WallpaperLoaded(None) => {
            // Unreadable: the plain background shows instead of nothing.
            app.wallpaper_pending = false;
            Task::none()
        }
        Message::Measured(kind, size) => {
            if let Some(h) = kind.height_for(app)
                && (h - size.height).abs() > 1.0
            {
                log::warn!(
                    "panel {kind:?}: worked-out height {h} but laid out {}",
                    size.height
                );
            }
            app.natural.insert(kind, size);
            if let Some(p) = &mut app.panel
                && p.kind == kind
                && p.open
            {
                // Always its real height at once: a glide to a new height (a list filtering,
                // a page opening) looked like a random wobble.
                p.h.snap(size.height);
                app.clock.reset();
                // Its own surface is sized to it; the first time it is laid out there (the
                // surface just made), its text field takes the focus.
                let first = app.panel_surface.is_some_and(|(_, _, k)| k & FRESH != 0);
                if let Some((_, _, k)) = &mut app.panel_surface {
                    *k &= !FRESH;
                }
                let sync = app.sync_panel_surface();
                return if first {
                    Task::batch([sync, panels::refocus(app, kind)])
                } else {
                    sync
                };
            }
            Task::none()
        }
        Message::MeasuredToasts(sz) => {
            app.toast_sheet.natural = sz.height;
            // Grows to it (the island swelling), unless already there.
            if !app.toasts.is_empty() {
                app.toast_sheet.h.to(sz.height, Spring::OPEN);
                app.clock.reset();
            }
            Task::none()
        }
        Message::MeasuredOsd(sz) => {
            app.osd_sheet.natural = sz.height;
            if app.osd.as_ref().is_some_and(|o| o.until > Instant::now()) {
                app.osd_sheet.h.snap(sz.height);
                app.clock.reset();
            }
            Task::none()
        }
        Message::OpenWallpaper => app.ipc("toggle wallpaper"),
        Message::ReloadWallpaper => load_wallpaper(app.config.wallpaper.clone()),
        Message::IslandMeasured(id, w) => {
            if let Some(o) = app.outputs.iter_mut().find(|o| o.id == id) {
                if o.island_w.value == 0.0 {
                    o.island_w.snap(w);
                } else {
                    // A glide, not a snap: the island and its neighbours move together smoothly
                    // when what it shows changes (a 70 ms snap read as a shake).
                    o.island_w.to(w, Spring::OPEN);
                }
                app.clock.reset();
            }
            Task::none()
        }
        Message::Breathe => Task::none(),
        Message::Expire(now) => {
            app.expire(now);
            Task::none()
        }
        _ => unreachable!("routed here by app::dispatch"),
    }
}
