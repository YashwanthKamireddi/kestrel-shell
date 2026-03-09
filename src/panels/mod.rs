//! Panels: what hangs from the bar. Each panel is a view built only from the component kit.

pub mod calendar;
pub mod clipboard;
pub mod control;
pub mod emoji;
pub mod files;
pub mod island;
pub mod keys;
pub mod launcher;
pub mod media;
pub mod notifications;
pub mod overview;
pub mod pods;
pub mod power;
pub mod settings;
pub mod tray;
pub mod wallpaper;
pub mod weather;

use crate::app::{App, Message as AppMessage};
use crate::services::tray as tray_service;
use crate::theme::Scheme;
use iced::{Element, Subscription, Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelKind {
    Control,
    Media,
    Calendar,
    Power,
    Tray,
    Launcher,
    Wallpaper,
    Keys,
    Island,
    Overview,
    Clipboard,
    Emoji,
    Settings,
    Weather,
    Pods,
    Files,
}

impl PanelKind {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "control" | "control-centre" | "control-center" | "sound" | "mixer" => Self::Control,
            "media" => Self::Media,
            "calendar" => Self::Calendar,
            // Notifications live in the control centre.
            "notifications" => Self::Control,
            "power" => Self::Power,
            "launcher" => Self::Launcher,
            "wallpaper" => Self::Wallpaper,
            "keys" | "cheatsheet" => Self::Keys,
            "island" | "dashboard" => Self::Island,
            "overview" => Self::Overview,
            "clipboard" => Self::Clipboard,
            "emoji" => Self::Emoji,
            "settings" => Self::Settings,
            "weather" => Self::Weather,
            "airpods" | "pods" => Self::Pods,
            "files" => Self::Files,
            "tray" => Self::Tray,
            _ => return None,
        })
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Control => 392.0,
            Self::Media => media::WIDTH,
            Self::Calendar => 312.0,
            Self::Power => 232.0,
            Self::Tray => 260.0,
            Self::Launcher => 560.0,
            Self::Wallpaper => wallpaper::WIDTH,
            Self::Keys => 720.0,
            Self::Island => 400.0,
            Self::Overview => overview::WIDTH,
            Self::Clipboard => clipboard::WIDTH,
            Self::Emoji => emoji::WIDTH,
            Self::Settings => settings::WIDTH,
            Self::Weather => weather::WIDTH,
            Self::Pods => pods::WIDTH,
            Self::Files => files::WIDTH,
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
    pub clipboard: clipboard::State,
    pub emoji: emoji::State,
    pub settings: settings::State,
    pub files: files::State,
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
    Clipboard(clipboard::Message),
    Emoji(emoji::Message),
    Settings(settings::Message),
    Files(files::Message),
    /// A navigation key while a panel is open (Esc is the shell's).
    Key(Key),
    /// Left click on a tray icon.
    TrayActivate(tray_service::Item),
}

/// Navigation keys every panel may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Tab,
    BackTab,
    PageUp,
    PageDown,
    Home,
    End,
    Delete,
    Backspace,
    Enter,
    /// Enter with Shift held.
    ShiftEnter,
    Space,
}

/// A panel was just opened: reset its transient state and load what it needs.
pub fn opened(app: &mut App, kind: PanelKind) -> Task<AppMessage> {
    match kind {
        PanelKind::Calendar => app.panels.calendar = calendar::State::default(),
        PanelKind::Island => {
            app.panels.calendar = calendar::State::default();
            // The rings sweep in from empty.
            for g in &mut app.gauges {
                g.snap(0.0);
            }
            app.aim_gauges();
        }
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
        PanelKind::Clipboard => return clipboard::open(app),
        PanelKind::Emoji => return emoji::open(app),
        PanelKind::Settings => return settings::open(app),
        PanelKind::Tray | PanelKind::Weather | PanelKind::Pods | PanelKind::Files => {}
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
        Message::Clipboard(m) => clipboard::update(app, m),
        Message::Emoji(m) => emoji::update(app, m),
        Message::Settings(m) => settings::update(app, m),
        Message::Files(m) => files::update(app, m),
        Message::Key(k) => match app.panel.as_ref().filter(|p| p.open).map(|p| p.kind) {
            Some(PanelKind::Launcher) => launcher::key(app, k),
            Some(PanelKind::Clipboard) => clipboard::key(app, k),
            Some(PanelKind::Emoji) => emoji::key(app, k),
            Some(PanelKind::Overview) => overview::key(app, k),
            Some(PanelKind::Control) => control::key(app, k),
            Some(PanelKind::Power) => power::key(app, k),
            Some(PanelKind::Files) => files::key(app, k),
            Some(PanelKind::Wallpaper) => wallpaper::key(app, k),
            _ => Task::none(),
        },
        Message::TrayActivate(item) => {
            Task::future(tray_service::activate(item.bus, item.path, false)).discard()
        }
    }
}

pub fn subscription(app: &App) -> Subscription<Message> {
    let open = app.panel.as_ref().filter(|p| p.open).map(|p| p.kind);
    let mut subs = Vec::new();
    // The clipboard's scrollbar, while held: follow the pointer wherever it goes and end the
    // drag wherever the button is let go (inside the panel only, it stopped at its edge).
    if open == Some(PanelKind::Clipboard) && app.panels.clipboard.dragging {
        subs.push(iced::event::listen_raw(|e, _, _| match e {
            iced::event::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::Clipboard(clipboard::Message::DragMove(position.y)))
            }
            iced::event::Event::Mouse(iced::mouse::Event::ButtonReleased(_)) => {
                Some(Message::Clipboard(clipboard::Message::BarRelease))
            }
            _ => None,
        }));
    }
    // Media position is not signalled: read it while the panel shows it.
    if open == Some(PanelKind::Media) && media::current(app).is_some_and(|p| p.playing) {
        // Smooth progress: redrawn ten times a second from the last read, re-read every 2 s.
        subs.push(
            iced::time::every(std::time::Duration::from_secs(2))
                .map(|_| Message::Media(media::Message::Poll)),
        );
        subs.push(
            iced::time::every(std::time::Duration::from_millis(100))
                .map(|_| Message::Media(media::Message::Tick)),
        );
    }
    if open.is_some() {
        // Seen even when a text field has the keyboard (it would swallow Delete and Tab).
        subs.push(iced::event::listen_with(|e, _status, _id| {
            use iced::keyboard::key::Named as N;
            let iced::event::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(k),
                modifiers,
                ..
            }) = e
            else {
                return None;
            };
            let key = match k {
                N::ArrowUp => Key::Up,
                N::ArrowDown => Key::Down,
                N::ArrowLeft => Key::Left,
                N::ArrowRight => Key::Right,
                N::Tab if modifiers.shift() => Key::BackTab,
                N::Tab => Key::Tab,
                N::PageUp => Key::PageUp,
                N::PageDown => Key::PageDown,
                N::Home => Key::Home,
                N::End => Key::End,
                N::Delete => Key::Delete,
                N::Backspace => Key::Backspace,
                N::Enter if modifiers.shift() => Key::ShiftEnter,
                N::Enter => Key::Enter,
                N::Space => Key::Space,
                _ => return None,
            };
            Some(Message::Key(key))
        }));
    }
    Subscription::batch(subs)
}

pub fn view(app: &App, kind: PanelKind, s: Scheme) -> Element<'_, AppMessage> {
    // A panel that panics shows nothing (and stays off); the shell carries on.
    crate::guard::run(
        &format!("panel {kind:?}"),
        || view_unguarded(app, kind, s),
        || iced::widget::Space::new().into(),
    )
}

fn view_unguarded(app: &App, kind: PanelKind, s: Scheme) -> Element<'_, AppMessage> {
    match kind {
        PanelKind::Calendar => calendar::view(app, s),
        PanelKind::Power => power::view(app, s),
        PanelKind::Control => control::view(app, s),
        PanelKind::Media => media::view(app, s),
        PanelKind::Tray => tray::view(app, s),
        PanelKind::Launcher => launcher::view(app, s),
        PanelKind::Wallpaper => wallpaper::view(app, s),
        PanelKind::Keys => keys::view(app, s),
        PanelKind::Island => island::view(app, s),
        PanelKind::Overview => overview::view(app, s),
        PanelKind::Clipboard => clipboard::view(app, s),
        PanelKind::Emoji => emoji::view(app, s),
        PanelKind::Settings => settings::view(app, s),
        PanelKind::Weather => weather::view(app, s),
        PanelKind::Pods => pods::view(app, s),
        PanelKind::Files => files::view(app, s),
    }
}
