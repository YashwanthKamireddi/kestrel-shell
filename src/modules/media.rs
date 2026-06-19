//! Now playing: cover and title; opens the media panel.

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
    let id = o.id;
    // Beside the island it is the island's (`policy`).
    if crate::policy::island_hosts(app, "media") {
        return Vec::new();
    }
    // A browser playing something with no picture (a hover preview) shows nothing.
    let Some(p) = app.media.main().filter(|p| !media::is_bare_browser(p)) else {
        return Vec::new();
    };
    let active = open == Some(PanelKind::Media);
    vec![
        super::chip(s, media::chip(s, p, active), active)
            .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r))
            .into(),
    ]
}
