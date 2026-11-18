//! Applying a changed config.toml or init.lua without restarting.

use super::*;

/// Why config.toml was read, which decides what a mistake in it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigFrom {
    /// The file was saved (by hand or by kestrel): a mistake is reported.
    File,
    /// Settings changed something: a mistake elsewhere in the file is left to the watcher.
    Settings,
    /// The Lua layer has a new bar: with a mistake, the running settings take it.
    Lua,
}

/// config.toml as read, and why.
#[derive(Clone, Debug)]
pub(crate) struct ConfigRead {
    pub from: ConfigFrom,
    /// Which read this is (`App::config_reads`): an older one answering late is dropped.
    pub seq: u64,
    pub config: Result<Config, String>,
    /// `[plugins]` as text.
    pub plugins: String,
}

impl App {
    /// Reads config.toml off the UI thread, after every change already queued for it
    /// (`crate::disk`), and applies it when it is in (`ConfigRead`).
    pub(crate) fn read_config(&mut self, from: ConfigFrom) -> Task<Message> {
        self.config_reads += 1;
        let seq = self.config_reads;
        Task::perform(
            crate::disk::run(move || ConfigRead {
                from,
                seq,
                config: config::read(),
                plugins: config::plugin_settings(),
            }),
            |r| match r {
                Some(r) => Message::ConfigRead(Box::new(r)),
                None => Message::Batch(Vec::new()),
            },
        )
    }
}

/// config.toml, as read: applied as each reason asks (what the handlers did when they read it
/// in place).
pub(crate) fn config_read(app: &mut App, r: ConfigRead) -> Task<Message> {
    if r.seq <= app.config_applied {
        return Task::none();
    }
    app.config_applied = r.seq;
    match (r.from, r.config) {
        (_, Ok(c)) => {
            app.config_error = None;
            // Plugin settings live in config.toml: plugins reload when they change (whichever
            // read sees it first: an older read answering late is dropped).
            if r.plugins != app.plugin_settings {
                app.plugin_settings = r.plugins;
                crate::lua::reload();
            }
            apply_config(app, c)
        }
        // A mistake while editing keeps everything as it was and says where it is (once per
        // distinct mistake), instead of falling back to the defaults.
        (ConfigFrom::File, Err(e)) => {
            if app.config_error.as_deref() != Some(e.as_str()) {
                app.config_error = Some(e.clone());
                app.notify(notifications::Event::Added(own_notification(
                    "Config not applied",
                    &e,
                )));
            }
            Task::none()
        }
        // A hand-made mistake elsewhere in the file: the watcher says where.
        (ConfigFrom::Settings, Err(_)) => Task::none(),
        (ConfigFrom::Lua, Err(_)) => {
            let mut c = app.config.clone();
            app.lua_bar.apply(&mut c.bar);
            apply_config(app, c)
        }
    }
}

/// Use a freshly read config (with the Lua layer's bar on top), updating only what changed.
pub(crate) fn apply_config(app: &mut App, mut c: Config) -> Task<Message> {
    crate::theme::apply(&c);
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
        || c.look.for_apps() != app.config.look.for_apps()
        || c.theme_apps != app.config.theme_apps;
    // Quiet hours and night light's schedule follow their settings at once, however they
    // changed (Settings or a hand edit).
    let quiet_changed = c.quiet_from != app.config.quiet_from || c.quiet_to != app.config.quiet_to;
    let night_changed = c.night_light_schedule != app.config.night_light_schedule;
    let motion_changed = c.motion != app.config.motion;
    app.config = c;
    let mut tasks = Vec::new();
    // Apps hear "reduced motion" as the shell has it (a recolour tells them anyway).
    if motion_changed && !accent_changed {
        app.tell_appearance();
    }
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
            app.read_config(ConfigFrom::Lua)
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
                until: Instant::now() + tok::HOLD_FLASH,
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
