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
pub mod open_with;
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
    /// "Open with…": which app opens a file or link, when an app asks.
    OpenWith,
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
            Self::Overview => overview::width(&app.hypr),
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
            Self::Overview => Some(overview::height(&app.hypr)),
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
            Self::Weather => weather::WIDTH,
            Self::Pods => pods::WIDTH,
            Self::Files => files::WIDTH,
            Self::OpenWith => open_with::WIDTH,
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
    pub open_with: open_with::State,
    pub auth: auth::State,
    pub notifications: notifications::State,
}

impl State {
    /// The Wi-Fi networks or nearby devices changed: every list of them keeps its rows in
    /// place (the control centre's and Settings').
    pub fn lists_changed(
        &mut self,
        net: &crate::services::network::State,
        bt: &crate::services::bluetooth::State,
    ) {
        self.control.lists_changed(net, bt);
        self.settings.lists_changed(net, bt);
    }
}

/// Settings opened: it reads its pages.
pub fn open_settings(app: &mut App) -> Task<AppMessage> {
    let (p, cx) = cx::split(app);
    let a = settings::open(&mut p.settings, &cx);
    action::perform(app, a, |m| AppMessage::Panel(Message::Settings(m)))
}

/// Settings' own window.
pub fn settings_window(app: &App) -> Element<'_, AppMessage> {
    let (p, cx) = cx::of(app);
    settings::window(&p.settings, &cx).map(|m| AppMessage::Panel(Message::Settings(m)))
}

/// The open panel's content: the shared keyboard moves only inside it (never onto the bar's
/// buttons, which are controls too).
pub static ROOT: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("kestrel-panel-root"));

/// The shared keyboard inside `root`: Tab and Shift+Tab take the next or previous control (iced's
/// own focus order), the arrows the nearest control that way (`ui::focus`), as in GTK; scoped
/// to that part of the window.
pub fn focus_step<M: Send + 'static>(root: iced::widget::Id, k: Key) -> Task<M> {
    use iced::advanced::widget::operation::{focusable, scope};
    let action = match k {
        Key::Tab => iced_runtime::Action::widget(scope(root, focusable::focus_next::<()>())),
        Key::BackTab => {
            iced_runtime::Action::widget(scope(root, focusable::focus_previous::<()>()))
        }
        Key::Up | Key::Down | Key::Left | Key::Right => {
            use crate::ui::focus::{Dir, toward};
            let dir = match k {
                Key::Up => Dir::Up,
                Key::Down => Dir::Down,
                Key::Left => Dir::Left,
                _ => Dir::Right,
            };
            iced_runtime::Action::widget(scope(root, toward(dir)))
        }
        _ => return Task::none(),
    };
    Task::Iced(iced_runtime::task::effect(action))
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
    OpenWith(open_with::Message),
    Auth(auth::Message),
    /// A navigation key while a panel is open (Esc is the shell's), and the window it was
    /// typed in.
    Key(iced::window::Id, Key),
    /// Tab or Shift+Tab in a window of its own (Settings): its id, and which.
    WindowKey(iced::window::Id, Key),
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
        PanelKind::OpenWith => open_with::FIRST.clone(),
        _ => return Task::none(),
    };
    Task::Iced(iced::widget::focus(id))
}

/// A panel was asked to open. One that opens only once what it shows is in (the monitor's
/// first reading) starts reading it here and opens itself when it arrives (`Action::Open`):
/// `Some` is that work. `None`: open it now.
pub fn prepare(
    app: &mut App,
    kind: PanelKind,
    output: iced::OutputId,
    at: iced::Rectangle,
) -> Option<Task<AppMessage>> {
    // Others open at once; the monitor too when it is open already (moved to another screen):
    // it has its reading.
    if kind != PanelKind::Monitor
        || app
            .panel
            .as_ref()
            .is_some_and(|p| p.open && p.kind == PanelKind::Monitor)
    {
        return None;
    }
    let a = monitor::prepare(&mut app.panels.monitor, output, at);
    Some(action::perform(app, a, |m| {
        AppMessage::Panel(Message::Monitor(m))
    }))
}

/// A panel asked for but still waiting for its content does not open after all (closed, or
/// another panel opened meanwhile).
pub fn cancel(app: &mut App) {
    monitor::cancel(&mut app.panels.monitor);
}

/// A panel was just opened: reset its transient state and load what it needs.
pub fn opened(app: &mut App, kind: PanelKind) -> Task<AppMessage> {
    // Another panel opened (by any route): a monitor still waiting for its reading does not
    // open over it.
    if kind != PanelKind::Monitor {
        cancel(app);
    }
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
        PanelKind::Launcher => {
            let a = launcher::open(&mut app.panels.launcher);
            return action::perform(app, a, |m| AppMessage::Panel(Message::Launcher(m)));
        }
        PanelKind::Wallpaper => {
            let (p, cx) = cx::split(app);
            let a = wallpaper::open(&mut p.wallpaper, &cx);
            return action::perform(app, a, |m| AppMessage::Panel(Message::Wallpaper(m)));
        }
        PanelKind::Keys => {
            return action::perform(app, keys::open(), |m| AppMessage::Panel(Message::Keys(m)));
        }
        PanelKind::Overview => {
            let a = overview::open(&mut app.panels.overview);
            return action::perform(app, a, |m| AppMessage::Panel(Message::Overview(m)));
        }
        PanelKind::Clipboard => {
            let a = clipboard::open(&mut app.panels.clipboard);
            return action::perform(app, a, |m| AppMessage::Panel(Message::Clipboard(m)));
        }
        PanelKind::Emoji => {
            let a = emoji::open(&mut app.panels.emoji);
            return action::perform(app, a, |m| AppMessage::Panel(Message::Emoji(m)));
        }
        PanelKind::Tray
        | PanelKind::Notifications
        | PanelKind::Privacy
        | PanelKind::Weather
        | PanelKind::Pods
        | PanelKind::Files
        | PanelKind::OpenWith
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
        Message::Capture(m) => {
            let a = capture::update(&mut app.panels.capture, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Capture(m)))
        }
        Message::Monitor(m) => {
            let a = monitor::update(&mut app.panels.monitor, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Monitor(m)))
        }
        Message::Control(m) => {
            let (p, cx) = cx::split(app);
            let a = control::update(&mut p.control, m, &cx);
            action::perform(app, a, |m| AppMessage::Panel(Message::Control(m)))
        }
        Message::Media(m) => {
            let (p, cx) = cx::split(app);
            let a = media::update(&mut p.media, m, &cx);
            action::perform(app, a, |m| AppMessage::Panel(Message::Media(m)))
        }
        Message::Notifications(m) => {
            let a = notifications::update(&mut app.panels.notifications, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Notifications(m)))
        }
        Message::Tray(m) => {
            let a = tray::update(&mut app.panels.tray, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Tray(m)))
        }
        Message::Launcher(m) => {
            let a = launcher::update(&mut app.panels.launcher, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Launcher(m)))
        }
        Message::Wallpaper(m) => {
            let (p, cx) = cx::split(app);
            let a = wallpaper::update(&mut p.wallpaper, m, &cx);
            action::perform(app, a, |m| AppMessage::Panel(Message::Wallpaper(m)))
        }
        Message::Keys(m) => {
            let a = keys::update(&mut app.panels.keys, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Keys(m)))
        }
        Message::Overview(m) => {
            let (p, cx) = cx::split(app);
            let a = overview::update(&mut p.overview, m, &cx);
            action::perform(app, a, |m| AppMessage::Panel(Message::Overview(m)))
        }
        Message::Clipboard(m) => {
            let a = clipboard::update(&mut app.panels.clipboard, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Clipboard(m)))
        }
        Message::Emoji(m) => {
            let a = emoji::update(&mut app.panels.emoji, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Emoji(m)))
        }
        Message::Settings(m) => {
            let (p, cx) = cx::split(app);
            let a = settings::own_window(settings::update(&mut p.settings, m, &cx));
            action::perform(app, a, |m| AppMessage::Panel(Message::Settings(m)))
        }
        Message::Files(m) => {
            let a = files::update(&mut app.panels.files, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::Files(m)))
        }
        Message::OpenWith(m) => {
            let a = open_with::update(&mut app.panels.open_with, m);
            action::perform(app, a, |m| AppMessage::Panel(Message::OpenWith(m)))
        }
        Message::Auth(m) => match app.panels.auth.update(m) {
            auth::Outcome::Close => Task::done(AppMessage::Close),
            auth::Outcome::Stay => Task::none(),
        },
        // Typed in Settings' window: its own (a panel opened with the mouse leaves the keyboard
        // with the app in front; Space in Settings' search played or paused the music panel,
        // the arrows changed its track).
        Message::Key(w, _) if in_settings(app, w) => Task::none(),
        Message::Key(_, k) => match app.panel.as_ref().filter(|p| p.open).map(|p| p.kind) {
            Some(PanelKind::Launcher) => {
                let a = launcher::key(&mut app.panels.launcher, k);
                action::perform(app, a, |m| AppMessage::Panel(Message::Launcher(m)))
            }
            Some(PanelKind::Clipboard) => {
                let a = clipboard::key(&mut app.panels.clipboard, k);
                action::perform(app, a, |m| AppMessage::Panel(Message::Clipboard(m)))
            }
            Some(PanelKind::Emoji) => {
                let a = emoji::key(&mut app.panels.emoji, k);
                action::perform(app, a, |m| AppMessage::Panel(Message::Emoji(m)))
            }
            Some(PanelKind::Overview) => {
                let (p, cx) = cx::split(app);
                let a = overview::key(&mut p.overview, k, &cx);
                action::perform(app, a, |m| AppMessage::Panel(Message::Overview(m)))
            }
            Some(PanelKind::Control) => {
                let (p, cx) = cx::split(app);
                match control::key(&mut p.control, k, &cx) {
                    Some(a) => action::perform(app, a, |m| AppMessage::Panel(Message::Control(m))),
                    None => focus_step(ROOT.clone(), k),
                }
            }
            Some(PanelKind::Capture) => {
                let a = capture::key(&mut app.panels.capture, k);
                action::perform(app, a, |m| AppMessage::Panel(Message::Capture(m)))
            }
            Some(PanelKind::Files) => {
                let a = files::key(&mut app.panels.files, k);
                action::perform(app, a, |m| AppMessage::Panel(Message::Files(m)))
            }
            Some(PanelKind::Wallpaper) => {
                let (p, cx) = cx::split(app);
                let a = wallpaper::key(&mut p.wallpaper, k, &cx);
                action::perform(app, a, |m| AppMessage::Panel(Message::Wallpaper(m)))
            }
            Some(PanelKind::Media) => {
                let (p, cx) = cx::split(app);
                let a = media::key(&mut p.media, k, &cx);
                action::perform(app, a, |m| AppMessage::Panel(Message::Media(m)))
            }
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
            // Every other panel: the shared keyboard (Tab and Shift+Tab move through its
            // controls; Enter and Space press the one with the ring).
            _ => focus_step(ROOT.clone(), k),
        },
        Message::TrayActivate(item) => {
            Task::future(tray_service::activate(item.bus, item.path, false)).discard()
        }
        Message::WindowKey(w, k) => {
            if in_settings(app, w) {
                focus_step(settings::ROOT.clone(), k)
            } else {
                Task::none()
            }
        }
    }
}

/// Whether window `w` is Settings' (its keys are its own, never the open panel's).
pub fn in_settings(app: &App, w: iced::window::Id) -> bool {
    app.settings_window
        .is_some_and(|id| iced::window::Id::from(id) == w)
}

pub fn subscription(app: &App) -> Subscription<Message> {
    let open = app.panel.as_ref().filter(|p| p.open).map(|p| p.kind);
    let mut subs = Vec::new();
    // The system monitor's readings while it is open.
    if open == Some(PanelKind::Monitor) {
        subs.push(
            iced::Subscription::run(|| {
                crate::services::stream::forever(crate::services::system::detail)
            })
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
            Subscription::run_with(overview::live_jobs(&app.hypr), |j| {
                let j = j.clone();
                crate::services::stream::forever(move || crate::services::previews::live(j.clone()))
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
    // The file picker: Ctrl and Shift change what a click picks.
    if open == Some(PanelKind::Files) {
        subs.push(iced::event::listen_with(|e, _, _| match e {
            iced::event::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => {
                Some(Message::Files(files::Message::Mods(m)))
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
    // The bar editor's drag: let go anywhere (in or out of a lane) ends it; Esc cancels.
    if app.settings_window.is_some() && settings::dragging(&app.panels.settings) {
        subs.push(iced::event::listen_with(|e, _, _| match e {
            iced::event::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )) => Some(Message::Settings(settings::Message::BarDrop)),
            iced::event::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => Some(Message::Settings(settings::Message::BarCancel)),
            _ => None,
        }));
    }
    // Settings' window: Tab and Shift+Tab, and the arrows, move through its controls.
    if app.settings_window.is_some() {
        subs.push(iced::event::listen_with(|e, status, w| {
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
                N::Tab if modifiers.shift() => Key::BackTab,
                N::Tab => Key::Tab,
                // Arrows a text field used (its cursor) stay with it.
                _ if status == iced::event::Status::Captured => return None,
                N::ArrowUp => Key::Up,
                N::ArrowDown => Key::Down,
                N::ArrowLeft => Key::Left,
                N::ArrowRight => Key::Right,
                _ => return None,
            };
            Some(Message::WindowKey(w, key))
        }));
    }
    if open.is_some() {
        // Seen even when a text field has the keyboard (it would swallow Delete and Tab).
        subs.push(iced::event::listen_with(|e, _status, w| {
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
            Some(Message::Key(w, key))
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
        PanelKind::Capture => {
            capture::view(&p.capture, &cx).map(|m| AppMessage::Panel(Message::Capture(m)))
        }
        PanelKind::Monitor => {
            monitor::view(&p.monitor, &cx).map(|m| AppMessage::Panel(Message::Monitor(m)))
        }
        PanelKind::Control => {
            control::view(&p.control, &cx).map(|m| AppMessage::Panel(Message::Control(m)))
        }
        PanelKind::Media => {
            media::view(&p.media, &cx).map(|m| AppMessage::Panel(Message::Media(m)))
        }
        PanelKind::Tray => tray::view(&p.tray, &cx).map(|m| AppMessage::Panel(Message::Tray(m))),
        PanelKind::Privacy => privacy::view(&cx).map(AppMessage::Panel),
        PanelKind::Notifications => notifications::view(&p.notifications, &cx)
            .map(|m| AppMessage::Panel(Message::Notifications(m))),
        PanelKind::Launcher => {
            launcher::view(&p.launcher, &cx).map(|m| AppMessage::Panel(Message::Launcher(m)))
        }
        PanelKind::Wallpaper => {
            wallpaper::view(&p.wallpaper, &cx).map(|m| AppMessage::Panel(Message::Wallpaper(m)))
        }
        PanelKind::Keys => keys::view(&p.keys, &cx).map(|m| AppMessage::Panel(Message::Keys(m))),
        PanelKind::Island => island::view(&p.calendar, &p.media, &cx).map(AppMessage::Panel),
        PanelKind::Overview => {
            overview::view(&p.overview, &cx).map(|m| AppMessage::Panel(Message::Overview(m)))
        }
        PanelKind::Clipboard => {
            clipboard::view(&p.clipboard, &cx).map(|m| AppMessage::Panel(Message::Clipboard(m)))
        }
        PanelKind::Emoji => {
            emoji::view(&p.emoji, &cx).map(|m| AppMessage::Panel(Message::Emoji(m)))
        }
        PanelKind::Weather => weather::view(&cx).map(AppMessage::Panel),
        PanelKind::Pods => pods::view(&cx).map(|m| AppMessage::Panel(Message::Pods(m))),
        PanelKind::Files => {
            files::view(&p.files, &cx).map(|m| AppMessage::Panel(Message::Files(m)))
        }
        PanelKind::Auth => auth::view(&app.panels.auth, s, |m| AppMessage::Panel(Message::Auth(m))),
        PanelKind::OpenWith => {
            open_with::view(&p.open_with, &cx).map(|m| AppMessage::Panel(Message::OpenWith(m)))
        }
    }
}

/// A panel closed (or another took its place) without its question answered: the app or
/// service waiting on it hears "cancelled". The one place this happens, for every panel that
/// holds a question (the file picker, "Open with", the password prompt).
pub fn closed(app: &mut App, kind: PanelKind) {
    match kind {
        PanelKind::Files => files::closed(&mut app.panels.files),
        PanelKind::OpenWith => open_with::closed(&mut app.panels.open_with),
        PanelKind::Auth => app.panels.auth.closed(),
        // Its numbers go with it: opened again, it reads afresh, never shows old ones.
        PanelKind::Monitor => app.panels.monitor = monitor::State::default(),
        _ => {}
    }
}

impl PanelKind {
    /// Whether it takes the keyboard when it opens: a field to type in, or a question to
    /// answer with the keys (Esc and the arrows expected there).
    pub fn takes_keyboard(self) -> bool {
        matches!(
            self,
            PanelKind::Launcher
                | PanelKind::Clipboard
                | PanelKind::Emoji
                | PanelKind::Overview
                | PanelKind::Files
                | PanelKind::Auth
                | PanelKind::OpenWith
        )
    }
}
