//! Pending updates, while there are any: a package and the count; opens Settings › Updates.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::tok;
use crate::theme::{Scheme, size, text};
use crate::ui::{Icon, icon, label};
use iced::widget::row;
use iced::{Alignment, Element};

pub fn view<'a>(
    app: &'a App,
    _o: &'a Output,
    s: Scheme,
    _open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let n = app.updates.total();
    if n == 0 {
        return Vec::new();
    }
    let content = row![
        icon(Icon::Cube, size::ICON, s.ink),
        label(n.to_string(), text::SMALL, s.ink),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    vec![
        super::chip(s, content, false)
            .on_press(Message::SettingsAt(crate::panels::settings::Page::Updates))
            .into(),
    ]
}
