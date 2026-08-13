//! Panels: what hangs from the bar. Each panel is a view built only from the component kit.

pub mod action;
pub mod auth;
pub mod calendar;
pub mod capture;
pub mod clipboard;
pub mod control;
pub mod cx;
pub mod emoji;
pub mod files;
pub mod island;
pub mod keys;
pub mod launcher;
pub mod media;
pub mod monitor;
pub mod notifications;
pub mod overview;
pub mod pods;
pub mod power;
pub mod privacy;
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
    /// The password prompt for administrator rights (kestrel's polkit agent).
    Auth,
    /// Screenshots and recordings: region, window or screen, now or later.
    Capture,
    /// System monitor: cores, memory, busiest processes.
    Monitor,
    /// What is using the camera, microphone and screen (under the privacy indicator).
    Privacy,
    /// The notifications, at the island (the bell opens it).
    Notifications,
}

impl PanelKind {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "control" | "control-centre" | "control-center" | "sound" | "mixer" | "bluetooth"
            | "wifi" | "network" => Self::Control,
            "media" => Self::Media,
            "calendar" => Self::Calendar,
            "notifications" => Self::Notifications,
            "power" => Self::Power,
            "capture" | "screenshot" => Self::Capture,
            "system" | "monitor" => Self::Monitor,
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
            "privacy" => Self::Privacy,
            _ => return None,
        })
    }

    /// The panel's width for what it shows now (the overview fits its cards).
    pub fn width_for(self, app: &App) -> f32 {
        match self {
            Self::Overview => overview::width(app),
            // Nothing playing: only as wide as its one line saying so.
            Self::Media if media::current(&app.panels.media, &app.media).is_none() => {
                media::EMPTY_WIDTH
            }
            _ => self.width(),
        }
    }

    /// The panel's height when it can be worked out before drawing (the overview, from its
    /// rows); others are measured.
    pub fn height_for(self, app: &App) -> Option<f32> {
        match self {
            Self::Overview => Some(overview::height(app)),
            _ => None,
        }
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Control => crate::theme::tok::PANEL_M,
            Self::Media => media::WIDTH,
            Self::Calendar => crate::theme::tok::PANEL_M,
            Self::Power => crate::theme::tok::PANEL_S,
            Self::Capture => capture::WIDTH,
            Self::Monitor => monitor::WIDTH,
            Self::Tray => crate::theme::tok::PANEL_S,
            Self::Privacy => privacy::WIDTH,
            Self::Notifications => crate::theme::tok::PANEL_M,
            Self::Launcher => crate::theme::tok::PANEL_L,
            Self::Wallpaper => wallpaper::WIDTH,
            Self::Keys => crate::theme::tok::PANEL_XL,
            Self::Island => crate::theme::tok::PANEL_M,
            Self::Overview => overview::WIDTH,
            Self::Clipboard => clipboard::WIDTH,
            Self::Emoji => emoji::WIDTH,
            Self::Settings => settings::WIDTH,
            Self::Weather => weather::WIDTH,
            Self::Pods => pods::WIDTH,
            Self::Files => files::WIDTH,
            Self::Auth => auth::WIDTH,
        }
    }
}

/// Per-panel UI state that outlives a single open.
#[derive(Default)]
pub struct State {
    pub calendar: calendar::State,
    pub power: power::State,
    pub capture: capture::State,
    pub monitor: monitor::State,
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
    pub auth: auth::State,
}

#[derive(Clone, Debug)]
pub enum Message {
    Calendar(calendar::Message),
    Power(power::Message),
    Capture(capture::Message),
    Monitor(monitor::Message),
    Control(control::Message),
    Media(media::Message),
    Notifications(notifications::Message),
    Tray(tray::Message),
    Launcher(launcher::Message),
    Wallpaper(wallpaper::Message),
    Keys(keys::Message),
    Pods(pods::Message),
    Overview(overview::Message),
    Clipboard(clipboard::Message),
    Emoji(emoji::Message),
    Settings(settings::Message),
    Files(files::Message),
    Auth(auth::Message),
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

/// The open panel's text field takes the focus again: its own surface has just been made
/// (the focus asked for on open ran before the surface existed and went nowhere).
pub fn refocus(app: &App, kind: PanelKind) -> Task<AppMessage> {
    let id = match kind {
        PanelKind::Launcher => launcher::INPUT_ID.clone(),
        PanelKind::Clipboard => clipboard::INPUT_ID.clone(),
        PanelKind::Emoji => emoji::INPUT_ID.clone(),
        PanelKind::Auth => auth::INPUT_ID.clone(),
        PanelKind::Files if app.panels.files.req.as_ref().is_some_and(|r| r.save) => {
            files::NAME_ID.clone()
        }
        PanelKind::Files => files::SEARCH_ID.clone(),
        _ => return Task::none(),
    };
    Task::Iced(iced::widget::focus(id))
}

/// A panel was just opened: reset its transient state and load what it needs.
pub fn opened(app: &mut App, kind: PanelKind) -> Task<AppMessage> {
    match kind {
        PanelKind::Calendar => app.panels.calendar = calendar::State::default(),
        PanelKind::Island => app.panels.calendar = calendar::State::default(),
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
        PanelKind::Keys => {
            return action::perform(app, keys::open(), |m| AppMessage::Panel(Message::Keys(m)));
        }
        PanelKind::Overview => return overview::open(app),
        PanelKind::Clipboard => return clipboard::open(app),
        PanelKind::Emoji => return emoji::open(app),
        PanelKind::Settings => return settings::open(app),
        PanelKind::Tray
        | PanelKind::Notifications
        | PanelKind::Privacy
        | PanelKind::Weather
        | PanelKind::Pods
        | PanelKind::Files
        | PanelKind::Auth
        | PanelKind::Capture
        | PanelKind::Monitor => {}
    }
    Task::none()
}

pub fn update(app: &mut App, msg: Message) -> Task<AppMessage> {
    match msg {
        Message::Calendar(m) => {
            app.panels.calendar.update(m);
            Task::none()
        }
        Message::Power(m) => {
            let a = power::update(&mut app.panels.power, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Power(m)))
        }
        Message::Pods(m) => action::perform(app, pods::update(m), |m| {
            AppMessage::Panel(Message::Pods(m))
        }),
        Message::Capture(m) => capture::update(app, m),
        Message::Monitor(m) => {
            let a = monitor::update(&mut app.panels.monitor, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Monitor(m)))
        }
        Message::Control(m) => control::update(app, m),
        Message::Media(m) => media::update(app, m),
        Message::Notifications(m) => notifications::update(app, m),
        Message::Tray(m) => tray::update(app, m),
        Message::Launcher(m) => launcher::update(app, m),
        Message::Wallpaper(m) => wallpaper::update(app, m),
        Message::Keys(m) => {
            let a = keys::update(&mut app.panels.keys, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Keys(m)))
        }
        Message::Overview(m) => overview::update(app, m),
        Message::Clipboard(m) => clipboard::update(app, m),
        Message::Emoji(m) => emoji::update(app, m),
        Message::Settings(m) => settings::update(app, m),
        Message::Files(m) => files::update(app, m),
        Message::Auth(m) => match app.panels.auth.update(m) {
            auth::Outcome::Close => Task::done(AppMessage::Close),
            auth::Outcome::Stay => Task::none(),
        },
        Message::Key(k) => match app.panel.as_ref().filter(|p| p.open).map(|p| p.kind) {
            Some(PanelKind::Launcher) => launcher::key(app, k),
            Some(PanelKind::Clipboard) => clipboard::key(app, k),
            Some(PanelKind::Emoji) => emoji::key(app, k),
            Some(PanelKind::Overview) => overview::key(app, k),
            Some(PanelKind::Control) => control::key(app, k),
            Some(PanelKind::Power) => {
                let a = power::key(&mut app.panels.power, k);
                action::perform(app, a, |m| AppMessage::Panel(Message::Power(m)))
            }
            Some(PanelKind::Capture) => capture::key(app, k),
            Some(PanelKind::Files) => files::key(app, k),
            Some(PanelKind::Wallpaper) => wallpaper::key(app, k),
            Some(PanelKind::Media) => media::key(app, k),
            Some(PanelKind::Keys) => {
                action::perform(app, keys::key(k), |m| AppMessage::Panel(Message::Keys(m)))
            }
            // The calendar: months back and forth, Home for today.
            Some(PanelKind::Calendar | PanelKind::Island) => {
                let m = match k {
                    Key::Left | Key::PageUp | Key::Up => Some(calendar::Message::Shift(-1)),
                    Key::Right | Key::PageDown | Key::Down => Some(calendar::Message::Shift(1)),
                    Key::Home => Some(calendar::Message::Today),
                    _ => None,
                };
                if let Some(m) = m {
                    app.panels.calendar.update(m);
                }
                Task::none()
            }
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
    // The system monitor's readings while it is open.
    if open == Some(PanelKind::Monitor) {
        subs.push(
            iced::Subscription::run(crate::services::system::detail)
                .map(|d| Message::Monitor(monitor::Message::Detail(d))),
        );
    }
    // Recording a shortcut's keys in Settings: the next combination pressed.
    if settings::recording(&app.panels.settings) {
        subs.push(iced::event::listen_with(|e, _status, _id| match e {
            iced::event::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                ..
            }) => settings::combo(&key, modifiers)
                .map(|k| Message::Settings(settings::Message::Recorded(k))),
            _ => None,
        }));
    }
    // The display settings' "keep?" countdown moves while it runs.
    if settings::display_countdown(&app.panels.settings).is_some() {
        subs.push(
            iced::time::every(std::time::Duration::from_secs(1))
                .map(|_| Message::Settings(settings::Message::DisplayTick)),
        );
    }
    // Searching for Bluetooth devices while the Bluetooth page is open (stopped when it
    // closes, however it closes).
    let settings_bt =
        app.settings_window.is_some() && app.panels.settings.page == settings::Page::Bluetooth;
    if app.bt.powered
        && (settings_bt
            || open == Some(PanelKind::Control)
                && app.panels.control.page == control::Page::Bluetooth)
    {
        subs.push(
            crate::subs::bt_search()
                .map(|()| Message::Control(control::Message::Page(control::Page::Bluetooth))),
        );
    }
    // Settings' Bluetooth page open: this computer can be found by other devices.
    if app.bt.powered && settings_bt {
        subs.push(
            crate::subs::bt_visible()
                .map(|()| Message::Control(control::Message::Page(control::Page::Bluetooth))),
        );
    }
    // The overview's pictures move with their windows while it is open (a playing video
    // plays): the compositor hands over a new picture each time a window changes.
    if open == Some(PanelKind::Overview) {
        subs.push(
            Subscription::run_with(overview::live_jobs(app), |j| {
                crate::services::previews::live(j.clone())
            })
            .map(|f| Message::Overview(overview::Message::Live(f))),
        );
    }
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
    if open == Some(PanelKind::Media)
        && media::current(&app.panels.media, &app.media).is_some_and(|p| p.playing)
    {
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
    let (p, cx) = cx::of(app);
    match kind {
        PanelKind::Calendar => calendar::view(&p.calendar, &p.media, &cx).map(AppMessage::Panel),
        PanelKind::Power => {
            power::view(&p.power, &cx).map(|m| AppMessage::Panel(Message::Power(m)))
        }
        PanelKind::Capture => capture::view(app, s),
        PanelKind::Monitor => {
            monitor::view(&p.monitor, &cx).map(|m| AppMessage::Panel(Message::Monitor(m)))
        }
        PanelKind::Control => control::view(app, s),
        PanelKind::Media => media::view(app, s),
        PanelKind::Tray => tray::view(app, s),
        PanelKind::Privacy => privacy::view(&cx).map(AppMessage::Panel),
        PanelKind::Notifications => notifications::view(app, s),
        PanelKind::Launcher => launcher::view(app, s),
        PanelKind::Wallpaper => wallpaper::view(app, s),
        PanelKind::Keys => keys::view(&p.keys, &cx).map(|m| AppMessage::Panel(Message::Keys(m))),
        PanelKind::Island => island::view(&p.calendar, &p.media, &cx).map(AppMessage::Panel),
        PanelKind::Overview => overview::view(app, s),
        PanelKind::Clipboard => clipboard::view(app, s),
        PanelKind::Emoji => emoji::view(app, s),
        PanelKind::Settings => settings::view(app, s),
        PanelKind::Weather => weather::view(&cx).map(AppMessage::Panel),
        PanelKind::Pods => pods::view(&cx).map(|m| AppMessage::Panel(Message::Pods(m))),
        PanelKind::Files => files::view(app, s),
        PanelKind::Auth => auth::view(&app.panels.auth, s, |m| AppMessage::Panel(Message::Auth(m))),
    }
}
