//! config.toml and Lua changing.

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::ConfigFile(crate::subs::ConfigFile::Toml) => match config::read() {
            Ok(c) => {
                app.config_error = None;
                // Plugin settings live in config.toml: plugins reload when they change.
                let plugins = config::plugin_settings();
                if plugins != app.plugin_settings {
                    app.plugin_settings = plugins;
                    crate::lua::reload();
                }
                apply_config(app, c)
            }
            // A mistake while editing keeps everything as it was and says where it is (once
            // per distinct mistake), instead of falling back to the defaults.
            Err(e) => {
                if app.config_error.as_deref() != Some(e.as_str()) {
                    app.config_error = Some(e.clone());
                    app.notify(notifications::Event::Added(own_notification(
                        "Config not applied",
                        &e,
                    )));
                }
                Task::none()
            }
        },
        Message::ConfigFile(crate::subs::ConfigFile::Lua) => {
            crate::lua::reload();
            Task::none()
        }
        Message::Lua(e) => lua_event(app, e),
        Message::LuaClick(name) => {
            crate::lua::click(&name);
            Task::none()
        }
        _ => unreachable!("routed here by app::dispatch"),
    }
}
