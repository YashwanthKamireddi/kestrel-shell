//! The system agents: password prompts, Wi-Fi secrets, Bluetooth pairing, the file chooser.

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Polkit(services::polkit::Event::Ask(r)) => {
            app.panels.auth.ask(r);
            let ask = panels::auth::focus();
            if app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Auth)
            {
                return ask;
            }
            let Some(id) = app.focused_output() else {
                return ask;
            };
            let x = app.output(id).map_or(0.0, |o| o.size.width / 2.0);
            app.panel_keys = true;
            let show = app.show(
                PanelKind::Auth,
                id,
                Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, size::BAR)),
            );
            Task::batch([show, ask])
        }
        Message::WifiAsk(services::network::Ask::Done(id)) => {
            // Answered, or NetworkManager stopped waiting: the prompt goes.
            if app.panels.auth.ended_wifi(id) == panels::auth::Outcome::Close
                && app
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.kind == PanelKind::Auth)
            {
                app.panel = None;
            }
            Task::none()
        }
        Message::WifiAsk(a) => {
            if !app.panels.auth.ask_wifi(a) {
                return Task::none();
            }
            let ask = panels::auth::focus();
            if app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Auth)
            {
                return ask;
            }
            let Some(id) = app.focused_output() else {
                return ask;
            };
            let x = app.output(id).map_or(0.0, |o| o.size.width / 2.0);
            app.panel_keys = true;
            let show = app.show(
                PanelKind::Auth,
                id,
                Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, size::BAR)),
            );
            Task::batch([show, ask])
        }
        Message::BtAsk(services::bluetooth::Ask::Done(id)) => {
            // The question is over (answered, or the device gave up): its prompt goes.
            if app.panels.auth.ended_bt(id) == panels::auth::Outcome::Close
                && app
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.kind == PanelKind::Auth)
            {
                app.panel = None;
            }
            Task::none()
        }
        Message::BtAsk(a) => {
            let pin = matches!(a, services::bluetooth::Ask::Pin { .. });
            if !app.panels.auth.ask_bt(a) {
                return Task::none();
            }
            let ask = if pin {
                panels::auth::focus()
            } else {
                Task::none()
            };
            if app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Auth)
            {
                return ask;
            }
            let Some(id) = app.focused_output() else {
                return ask;
            };
            let x = app.output(id).map_or(0.0, |o| o.size.width / 2.0);
            app.panel_keys = true;
            let show = app.show(
                PanelKind::Auth,
                id,
                Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, size::BAR)),
            );
            Task::batch([show, ask])
        }
        Message::WindowClosed(w) => {
            if app
                .settings_window
                .is_some_and(|id| iced::window::Id::from(id) == w)
            {
                app.settings_window = None;
            }
            Task::none()
        }
        Message::Polkit(services::polkit::Event::Done(cookie)) => {
            if app.panels.auth.ended_polkit(&cookie) == panels::auth::Outcome::Close
                && app
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.kind == PanelKind::Auth)
            {
                app.panel = None;
            }
            Task::none()
        }
        Message::FileRequest(r) => {
            let begin = panels::files::begin(app, r);
            let Some(id) = app.focused_output() else {
                return begin;
            };
            let x = app.output(id).map_or(0.0, |o| o.size.width / 2.0);
            app.panel_keys = true;
            let show = app.show(
                PanelKind::Files,
                id,
                Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, size::BAR)),
            );
            Task::batch([begin, show])
        }
        _ => unreachable!("routed here by app::dispatch"),
    }
}
