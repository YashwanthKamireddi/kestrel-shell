//! Now playing: cover and title (in the pill style, a round bubble); opens the media panel.

use crate::app::{App, Message, Output};
use crate::panels::{PanelKind, media};
use crate::theme::Scheme;
use iced::Element;

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    // Pill style: the music is a round bubble (one thing, one circle).
    if app.config.bar.style == crate::config::BarStyle::Pills {
        return super::island::bubble(app, o, s).into_iter().collect();
    }
    let id = o.id;
    let Some(p) = app.media.main() else {
        return Vec::new();
    };
    let active = open == Some(PanelKind::Media);
    vec![
        super::chip(s, media::chip(s, p, active), active)
            .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r))
            .into(),
    ]
}
