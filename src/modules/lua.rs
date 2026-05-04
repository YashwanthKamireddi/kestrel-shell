//! A Lua widget (`kestrel.widget` in init.lua): an optional icon and a label; a click runs its
//! `click`.

use crate::app::Message;
use crate::services::icons;
use crate::theme::{Scheme, size, text};
use crate::ui::{self, Icon, icon, label};
use iced::widget::row;
use iced::{Alignment, Element};

pub fn chip<'a>(name: &str, c: &'a crate::lua::Chip, s: Scheme) -> Element<'a, Message> {
    let mut r = row![].spacing(6).align_y(Alignment::Center);
    // A name from kestrel's icon set, else an icon theme name or a path.
    if let Some(name) = c.icon.as_deref() {
        if let Some(i) = Icon::named(name) {
            r = r.push(icon(i, size::ICON, s.ink));
        } else if let Some(p) = icons::lookup(name) {
            r = r.push(ui::image_file(&p, size::ICON));
        }
    }
    if !c.text.is_empty() {
        r = r.push(label(
            ui::fit(&c.text, text::SMALL, 220.0),
            text::SMALL,
            s.ink,
        ));
    }
    super::chip(s, r, false)
        .on_press(Message::LuaClick(name.to_string()))
        .into()
}
