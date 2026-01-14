//! Panels: what hangs from the bar. Each panel is a view built only from the component kit.

pub mod calendar;
pub mod control;
pub mod island;
pub mod keys;
pub mod launcher;
pub mod media;
pub mod notifications;
pub mod overview;
pub mod power;
pub mod tray;
pub mod wallpaper;

use crate::app::{App, Message as AppMessage};
use crate::services::tray as tray_service;
use crate::theme::Scheme;
use iced::{Element, Subscription, Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelKind {
    Control,
    Media,
    Calendar,
    Notifications,
    Power,
    Tray,
    Launcher,
    Wallpaper,
    Keys,
    Island,
    Overview,
}

impl PanelKind {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "control" | "control-centre" | "control-center" => Self::Control,
            "media" => Self::Media,
            "calendar" => Self::Calendar,
            "notifications" => Self::Notifications,
            "power" => Self::Power,
            "launcher" => Self::Launcher,
            "wallpaper" => Self::Wallpaper,
            "keys" | "cheatsheet" => Self::Keys,
            "island" | "dashboard" => Self::Island,
            "overview" => Self::Overview,
            _ => return None,
        })
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Control => 368.0,
            Self::Media => 380.0,
            Self::Calendar => 312.0,
            Self::Notifications => 392.0,
            Self::Power => 232.0,
            Self::Tray => 248.0,
            Self::Launcher => 560.0,
            Self::Wallpaper => 560.0,
            Self::Keys => 720.0,
            Self::Island => 400.0,
            Self::Overview => overview::WIDTH,
        }
    }
}

/// Per-panel UI state that outlives a single open.
#[derive(Default)]
pub struct State {
    pub calendar: calendar::State,
    pub power: power::State,
    pub control: control::State,
    pub media: media::State,
    pub tray: tray::State,
    pub launcher: launcher::State,
    pub wallpaper: wallpaper::State,
    pub keys: keys::State,
    pub overview: overview::State,
}

#[derive(Clone, Debug)]
pub enum Message {
    Calendar(calendar::Message),
    Power(power::Message),
    Control(control::Message),
    Media(media::Message),
    Notifications(notifications::Message),
    Tray(tray::Message),
    Launcher(launcher::Message),
    Wallpaper(wallpaper::Message),
    Keys(keys::Message),
    Overview(overview::Message),
    /// Left click on a tray icon.
    TrayActivate(tray_service::Item),
}

/// A panel was just opened: reset its transient state and load what it needs.
pub fn opened(app: &mut App, kind: PanelKind) -> Task<AppMessage> {
    match kind {
        PanelKind::Calendar | PanelKind::Island => app.panels.calendar = calendar::State::default(),
        PanelKind::Power => app.panels.power = power::State::default(),
        PanelKind::Control => {
            app.panels.control = control::State::default();
            app.brightness = crate::services::power::brightness();
        }
        PanelKind::Media => {
            app.panels.media.position = None;
            return Task::done(AppMessage::Panel(Message::Media(media::Message::Poll)));
        }
        PanelKind::Launcher => return launcher::open(app),
        PanelKind::Wallpaper => return wallpaper::open(app),
        PanelKind::Keys => return keys::open(app),
        PanelKind::Overview => return overview::open(app),
        PanelKind::Notifications | PanelKind::Tray => {}
    }
    Task::none()
}

pub fn update(app: &mut App, msg: Message) -> Task<AppMessage> {
    match msg {
        Message::Calendar(m) => {
            app.panels.calendar.update(m);
            Task::none()
        }
        Message::Power(m) => power::update(app, m),
        Message::Control(m) => control::update(app, m),
        Message::Media(m) => media::update(app, m),
        Message::Notifications(m) => notifications::update(app, m),
        Message::Tray(m) => tray::update(app, m),
        Message::Launcher(m) => launcher::update(app, m),
        Message::Wallpaper(m) => wallpaper::update(app, m),
        Message::Keys(m) => keys::update(app, m),
        Message::Overview(m) => overview::update(app, m),
        Message::TrayActivate(item) => {
            Task::future(tray_service::activate(item.bus, item.path, false)).discard()
        }
    }
}

pub fn subscription(app: &App) -> Subscription<Message> {
    let open = app.panel.as_ref().filter(|p| p.open).map(|p| p.kind);
    let mut subs = Vec::new();
    // Media position is not signalled; read it once a second while the panel shows it.
    if open == Some(PanelKind::Media) && media::current(app).is_some_and(|p| p.playing) {
        subs.push(
            iced::time::every(std::time::Duration::from_secs(1))
                .map(|_| Message::Media(media::Message::Poll)),
        );
    }
    if open == Some(PanelKind::Launcher) {
        subs.push(iced::keyboard::listen().filter_map(|e| match e {
            iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(k),
                ..
            } => match k {
                iced::keyboard::key::Named::ArrowDown => {
                    Some(Message::Launcher(launcher::Message::Move(1)))
                }
                iced::keyboard::key::Named::ArrowUp => {
                    Some(Message::Launcher(launcher::Message::Move(-1)))
                }
                _ => None,
            },
            _ => None,
        }));
    }
    Subscription::batch(subs)
}

pub fn view(app: &App, kind: PanelKind, s: Scheme) -> Element<'_, AppMessage> {
    match kind {
        PanelKind::Calendar => calendar::view(app, s),
        PanelKind::Power => power::view(app, s),
        PanelKind::Control => control::view(app, s),
        PanelKind::Media => media::view(app, s),
        PanelKind::Notifications => notifications::view(app, s),
        PanelKind::Tray => tray::view(app, s),
        PanelKind::Launcher => launcher::view(app, s),
        PanelKind::Wallpaper => wallpaper::view(app, s),
        PanelKind::Keys => keys::view(app, s),
        PanelKind::Island => island::view(app, s),
        PanelKind::Overview => overview::view(app, s),
    }
}
