//! The system agents: password prompts, Wi-Fi secrets, Bluetooth pairing, the file chooser.

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Polkit(services::polkit::Event::Ask(r)) => {
            app.panels.auth.ask(r);
            show_prompt(app, true)
        }
        Message::WifiAsk(services::network::Ask::Done(id)) => {
            // Answered, or NetworkManager stopped waiting: the prompt goes.
            let o = app.panels.auth.ended_wifi(id);
            prompt_ended(app, o)
        }
        Message::WifiAsk(a) => {
            if !app.panels.auth.ask_wifi(a) {
                return Task::none();
            }
            show_prompt(app, true)
        }
        Message::BtAsk(services::bluetooth::Ask::Done(id)) => {
            // The question is over (answered, or the device gave up): its prompt goes.
            let o = app.panels.auth.ended_bt(id);
            prompt_ended(app, o)
        }
        Message::BtAsk(a) => {
            let pin = matches!(a, services::bluetooth::Ask::Pin { .. });
            if !app.panels.auth.ask_bt(a) {
                return Task::none();
            }
            show_prompt(app, pin)
        }
        Message::KeyringAsk(services::keyring::Ask::Done(id)) => {
            let o = app.panels.auth.ended_keyring(id);
            prompt_ended(app, o)
        }
        Message::KeyringAsk(services::keyring::Ask::Prompt(p)) => {
            let typing = p.kind == Some(services::keyring::Kind::Password);
            if !app.panels.auth.ask_keyring(p) {
                return Task::none();
            }
            show_prompt(app, typing)
        }
        Message::WindowClosed(w) => {
            if app
                .settings_window
                .is_some_and(|id| iced::window::Id::from(id) == w)
            {
                app.settings_gone();
            }
            Task::none()
        }
        Message::Polkit(services::polkit::Event::Done(cookie)) => {
            let o = app.panels.auth.ended_polkit(&cookie);
            prompt_ended(app, o)
        }
        Message::OpenWith(services::appchooser::Event::Update(id, choices)) => {
            panels::open_with::update_choices(&mut app.panels.open_with, id, choices);
            Task::none()
        }
        Message::OpenWith(services::appchooser::Event::Ask(a)) => {
            panels::open_with::begin(&mut app.panels.open_with, a);
            let Some(id) = app.focused_output() else {
                return Task::none();
            };
            let x = app.output(id).map_or(0.0, |o| o.size.width / 2.0);
            app.panel_keys = true;
            app.show(
                PanelKind::OpenWith,
                id,
                Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, tok::BAR)),
            )
        }
        Message::FileRequest(r) => {
            // An app's question replaces kestrel's own: nobody waits on that answer any more.
            if r.id != services::filechooser::OWN {
                app.file_answer = None;
            }
            let a = panels::files::begin(&mut app.panels.files, r);
            let begin =
                panels::action::perform(app, a, |m| Message::Panel(panels::Message::Files(m)));
            let Some(id) = app.focused_output() else {
                return begin;
            };
            let x = app.output(id).map_or(0.0, |o| o.size.width / 2.0);
            app.panel_keys = true;
            let show = app.show(
                PanelKind::Files,
                id,
                Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, tok::BAR)),
            );
            Task::batch([begin, show])
        }
        _ => unreachable!("routed here by app::dispatch"),
    }
}

/// A question arrived for the prompt: shown under the bar's centre on the focused screen, and
/// its field takes the keyboard (`focus`); already shown, only the keyboard moves.
fn show_prompt(app: &mut App, focus: bool) -> Task<Message> {
    let ask = if focus {
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
        Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, tok::BAR)),
    );
    Task::batch([show, ask])
}

/// A question ended elsewhere: the prompt goes when nothing is left to ask.
fn prompt_ended(app: &mut App, o: panels::auth::Outcome) -> Task<Message> {
    if o == panels::auth::Outcome::Close
        && app
            .panel
            .as_ref()
            .is_some_and(|p| p.kind == PanelKind::Auth)
    {
        app.panel = None;
    }
    Task::none()
}
