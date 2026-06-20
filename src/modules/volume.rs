//! Sound: the output's level; opens the control centre's Sound page. Scrolling changes the
//! volume; right click mutes.

use crate::app::{App, Message, Output};
use crate::panels::{Message as PanelMessage, PanelKind, control};
use crate::theme::Scheme;
use iced::Element;

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let active = super::control_on(app, open, control::Page::Output);
    vec![
        super::icon_chip(s, control::level_icon(app.audio.sink()), s.ink, active)
            .on_press_at(move |r| Message::ControlAt(control::Page::Output, id, r))
            .on_right_press(Message::Panel(PanelMessage::Control(
                control::Message::SinkMute,
            )))
            .on_scroll(Message::VolumeStep)
            .into(),
    ]
}
