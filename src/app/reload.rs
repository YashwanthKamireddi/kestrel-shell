//! Applying a changed config.toml or init.lua without restarting.

use super::*;

/// Use a freshly read config (with the Lua layer's bar on top), updating only what changed.
pub(crate) fn apply_config(app: &mut App, mut c: Config) -> Task<Message> {
    crate::theme::set_glass(c.material != config::Material::Black);
    app.lua_bar.apply(&mut c.bar);
    c.bar.normalize();
    if c == app.config {
        return Task::none();
    }
    for id in c.bar.left.iter().chain(&c.bar.center).chain(&c.bar.right) {
        if !crate::modules::known(id) && !app.chips.contains_key(id) {
            log::info!("bar: {id:?} is not a module (yet); a Lua widget may define it");
        }
    }
    let strips_changed = c.frame != app.config.frame;
    let wallpaper_changed = c.wallpaper != app.config.wallpaper;
    // The accent, or the apps' fonts (Qt gets them with the colours), change the colours
    // written for apps.
    let accent_changed = c.accent != app.config.accent
        || c.look != app.config.look
        || c.theme_apps != app.config.theme_apps;
    // Quiet hours and night light's schedule follow their settings at once, however they
    // changed (Settings or a hand edit).
    let quiet_changed = c.quiet_from != app.config.quiet_from || c.quiet_to != app.config.quiet_to;
    let night_changed = c.night_light_schedule != app.config.night_light_schedule;
    app.config = c;
    let mut tasks = Vec::new();
    if quiet_changed {
        app.quiet_auto = None;
        app.quiet_schedule();
    }
    if night_changed {
        app.night_auto = None;
        tasks.push(app.night_schedule());
    }
    if accent_changed {
        tasks.push(app.recolour());
    }
    if wallpaper_changed {
        tasks.push(load_wallpaper(app.config.wallpaper.clone()));
    }
    // Only a new frame size needs new surfaces; style and the rest change in place.
    if strips_changed {
        tasks.push(app.rebuild_outputs());
    } else {
        tasks.push(app.apply_frame());
    }
    log::info!("config: applied changes");
    Task::batch(tasks)
}

pub(crate) fn lua_event(app: &mut App, e: crate::lua::Event) -> Task<Message> {
    use crate::lua::Event;
    match e {
        Event::Loaded { bar, widgets } => {
            app.chips.retain(|name, _| widgets.contains(name));
            app.lua_bar = bar;
            match config::read() {
                Ok(c) => apply_config(app, c),
                Err(_) => {
                    let mut c = app.config.clone();
                    app.lua_bar.apply(&mut c.bar);
                    apply_config(app, c)
                }
            }
        }
        Event::Widget(name, Some(chip)) => {
            app.chips.insert(name, chip);
            Task::none()
        }
        Event::Widget(name, None) => {
            app.chips.remove(&name);
            Task::none()
        }
        Event::Error(e) => {
            log::warn!("lua: {e}");
            app.notify(notifications::Event::Added(own_notification(
                "init.lua", &e,
            )))
        }
        Event::Notify(title, body) => {
            app.notify(notifications::Event::Added(own_notification(&title, &body)))
        }
        Event::Island(icon, title, detail) => {
            app.flash = Some(Flash {
                icon: icon
                    .as_deref()
                    .and_then(Icon::named)
                    .unwrap_or(Icon::Sparkles),
                title,
                detail,
                thumb: None,
                until: Instant::now() + Duration::from_millis(FLASH_MS),
                action: None,
            });
            app.clock.reset();
            Task::none()
        }
        Event::Exec(cmd) => Task::done(Message::Dispatch(
            crate::services::compositor::Action::Exec(String::from(&cmd)),
        )),
    }
}
