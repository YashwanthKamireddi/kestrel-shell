//! config.toml and Lua changing.

use super::super::*;

pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        // Read off the UI thread; applied (or its mistake reported) when it is in.
        Message::ConfigFile(crate::subs::ConfigFile::Toml) => app.read_config(ConfigFrom::File),
        Message::ConfigRead(r) => config_read(app, *r),
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
