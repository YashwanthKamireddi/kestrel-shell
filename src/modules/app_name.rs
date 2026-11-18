//! The focused window's app name (on the focused monitor only).

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::tok;
use crate::theme::{Scheme, text};
use crate::ui::label;
use iced::Element;
use iced::widget::container;

/// The focused window's app name (on the focused monitor only).
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    _open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    // Nothing focused here: no item at all, not an empty one.
    if app.hypr.focused_monitor != o.name || app.hypr.window_class.is_empty() {
        return Vec::new();
    }
    vec![name(app, o, s)]
}

fn name<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let name = if app.hypr.focused_monitor == o.name {
        crate::services::apps::name_for_class(&app.hypr.window_class)
    } else {
        String::new()
    };
    container(label(name, text::SMALL, s.ink))
        .padding([tok::NONE, tok::S])
        .into()
}
