//! The prompt process (`kestrel prompt`, its own systemd service): kestrel's polkit
//! authentication agent, NetworkManager secret agent and BlueZ pairing agent, with their window,
//! apart from the shell. A shell that crashes, freezes or restarts never cancels a password,
//! Wi-Fi or pairing question, and a question never waits on the shell
//! (docs/ROADMAP-v2.md, rule 3).
//!
//! It shows panels::auth (the same component the shell shows when there is no prompt process)
//! in a window of its own, under the bar's centre, made when a question arrives and gone when
//! it is answered. It reads the shell's colours from state.toml and never writes there.

use crate::panels::auth;
use crate::services::{bluetooth, keyring, network, polkit};
use crate::theme::{Scheme, Seed, tok};
use iced::task::{destroy_layer_surface, new_layer_surface, set_size};
use iced::{
    Anchor, Element, KeyboardInteractivity, Layer, LayerShellSettings, Size, Subscription,
    SurfaceId, Task, Theme,
};

pub struct App {
    auth: auth::State,
    scheme: Scheme,
    frosted: bool,
    /// The window, while something is asked, and the height last given to it.
    window: Option<(SurfaceId, u32)>,
    /// The field takes the keyboard once the window is laid out (asked earlier, it went
    /// nowhere).
    focus_pending: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Polkit(polkit::Event),
    Wifi(network::Ask),
    Bt(bluetooth::Ask),
    Keyring(keyring::Ask),
    Auth(auth::Message),
    Measured(Size),
    Alive,
}

/// The shell's colours and material as they are now.
fn look() -> (Scheme, bool) {
    let l = crate::state::read_fresh().look;
    let seed = if l.chroma == 0.0 && l.source.is_none() {
        Seed::default()
    } else {
        Seed {
            hue: l.hue,
            chroma: l.chroma,
            coverage: l.coverage,
            source: l.source,
        }
    };
    let frosted = crate::config::load().material != crate::config::Material::Black;
    crate::theme::set_glass(frosted);
    (Scheme::dark(seed), frosted)
}

pub fn boot() -> (App, Task<Message>) {
    let (scheme, frosted) = look();
    (
        App {
            auth: auth::State::default(),
            scheme,
            frosted,
            window: None,
            focus_pending: false,
        },
        preview(),
    )
}

/// `KESTREL_PREVIEW=<which>` (devtools builds): a made-up question instead of the real agents,
/// to see the window in the rig.
#[cfg(feature = "devtools")]
fn preview_name() -> Option<String> {
    std::env::var("KESTREL_PREVIEW").ok()
}

#[cfg(not(feature = "devtools"))]
fn preview_name() -> Option<String> {
    None
}

#[cfg(feature = "devtools")]
fn preview() -> Task<Message> {
    use auth::Preview;
    match preview_name().as_deref().and_then(auth::preview) {
        Some(Preview::Polkit(r)) => Task::done(Message::Polkit(polkit::Event::Ask(r))),
        Some(Preview::Wifi(a)) => Task::done(Message::Wifi(a)),
        Some(Preview::Bt(a)) => Task::done(Message::Bt(a)),
        Some(Preview::Keyring(p)) => Task::done(Message::Keyring(keyring::Ask::Prompt(p))),
        None => Task::none(),
    }
}

#[cfg(not(feature = "devtools"))]
fn preview() -> Task<Message> {
    Task::none()
}

impl App {
    /// A question arrived: the window is made (with the shell's colours as they are now) if it
    /// is not there.
    fn open(&mut self, focus: bool) -> Task<Message> {
        if self.window.is_some() {
            return if focus { auth::focus() } else { Task::none() };
        }
        (self.scheme, self.frosted) = look();
        self.focus_pending = focus;
        let (id, task) = new_layer_surface(LayerShellSettings {
            anchor: Anchor::TOP,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            exclusive_zone: -1,
            size: Some((tok::PANEL_M as u32, 1)),
            margin: ((tok::BAR + crate::ui::shape::FLOAT_GAP) as i32, 0, 0, 0),
            namespace: "kestrel-panel".into(),
            output: None,
        });
        self.window = Some((id, 1));
        task
    }

    fn close(&mut self) -> Task<Message> {
        match self.window.take() {
            Some((id, _)) => destroy_layer_surface(id),
            None => Task::none(),
        }
    }

    fn outcome(&mut self, o: auth::Outcome) -> Task<Message> {
        match o {
            auth::Outcome::Close => self.close(),
            auth::Outcome::Stay => Task::none(),
        }
    }
}

pub fn update(app: &mut App, m: Message) -> Task<Message> {
    match m {
        Message::Polkit(polkit::Event::Ask(r)) => {
            app.auth.ask(r);
            app.open(true)
        }
        Message::Polkit(polkit::Event::Done(cookie)) => {
            let o = app.auth.ended_polkit(&cookie);
            app.outcome(o)
        }
        Message::Wifi(network::Ask::Done(id)) => {
            let o = app.auth.ended_wifi(id);
            app.outcome(o)
        }
        Message::Wifi(a) => {
            if app.auth.ask_wifi(a) {
                app.open(true)
            } else {
                Task::none()
            }
        }
        Message::Bt(bluetooth::Ask::Done(id)) => {
            let o = app.auth.ended_bt(id);
            app.outcome(o)
        }
        Message::Bt(a) => {
            let pin = matches!(a, bluetooth::Ask::Pin { .. });
            if app.auth.ask_bt(a) {
                app.open(pin)
            } else {
                Task::none()
            }
        }
        Message::Keyring(keyring::Ask::Done(id)) => {
            let o = app.auth.ended_keyring(id);
            app.outcome(o)
        }
        Message::Keyring(keyring::Ask::Prompt(p)) => {
            let typing = p.kind == Some(keyring::Kind::Password);
            if app.auth.ask_keyring(p) {
                app.open(typing)
            } else {
                Task::none()
            }
        }
        Message::Auth(m) => {
            let o = app.auth.update(m);
            app.outcome(o)
        }
        Message::Measured(sz) => {
            let Some((id, h)) = app.window else {
                return Task::none();
            };
            let mut tasks = Vec::new();
            let want = sz.height.ceil().max(1.0) as u32;
            if want != h {
                app.window = Some((id, want));
                tasks.push(set_size(id, (tok::PANEL_M as u32, want)));
            }
            if std::mem::take(&mut app.focus_pending) {
                tasks.push(auth::focus());
            }
            Task::batch(tasks)
        }
        Message::Alive => {
            crate::watchdog::notify("WATCHDOG=1");
            Task::none()
        }
    }
}

pub fn view(app: &App, id: SurfaceId) -> Element<'_, Message> {
    use iced::widget::container;
    if app.window.map(|w| w.0) != Some(id) {
        return iced::widget::Space::new().into();
    }
    let s = app.scheme;
    let frosted = app.frosted;
    let card = container(auth::view(&app.auth, s, Message::Auth))
        .width(iced::Length::Fill)
        .style(move |_| container::Style {
            background: Some(if frosted {
                s.base.scale_alpha(0.42).into()
            } else {
                s.base.into()
            }),
            border: iced::Border::default().rounded(tok::R_PANEL),
            ..container::Style::default()
        });
    let card: Element<'_, Message> = if frosted {
        iced::blur_container(card)
            .style(move |_| container::Style {
                border: iced::Border::default().rounded(tok::R_PANEL),
                ..container::Style::default()
            })
            .into()
    } else {
        card.into()
    };
    crate::ui::reveal(card, tok::PANEL_M, app.window.map_or(1, |w| w.1) as f32)
        .on_measure(Message::Measured)
        .into()
}

pub fn subscription(_app: &App) -> Subscription<Message> {
    let agents = if preview_name().is_some() {
        Subscription::none()
    } else {
        Subscription::batch([
            crate::subs::polkit().map(Message::Polkit),
            crate::subs::wifi_agent().map(Message::Wifi),
            crate::subs::bt_agent().map(Message::Bt),
            crate::subs::keyring().map(Message::Keyring),
        ])
    };
    Subscription::batch([
        agents,
        match crate::watchdog::interval() {
            Some(every) => iced::time::every(every).map(|_| Message::Alive),
            None => Subscription::none(),
        },
    ])
}

pub fn theme(_app: &App, _id: SurfaceId) -> Theme {
    Theme::Dark
}

/// Runs the prompt process.
pub fn run() -> anyhow::Result<()> {
    iced::application(boot, update, view)
        .subscription(subscription)
        .theme(theme)
        .default_font(crate::theme::FONT)
        .font(crate::ui::icon::BYTES)
        .font(crate::theme::RUBIK_MEDIUM)
        .font(crate::theme::RUBIK_SEMIBOLD)
        .font(crate::theme::RUBIK_BOLD)
        .antialiasing(true)
        .layer_shell(LayerShellSettings {
            layer: Layer::Background,
            size: Some((1, 1)),
            anchor: Anchor::TOP | Anchor::LEFT,
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: "kestrel-prompt-root".into(),
            ..LayerShellSettings::default()
        })
        .run()?;
    Ok(())
}
