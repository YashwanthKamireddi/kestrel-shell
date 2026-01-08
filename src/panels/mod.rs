//! Panels: what hangs from the bar. Each panel is a view built only from the component kit.

pub mod calendar;
pub mod control;
pub mod keys;
pub mod launcher;
pub mod media;
pub mod notifications;
pub mod power;
pub mod tray;
pub mod wallpaper;

use crate::app::{App, Message as AppMessage, Output};
use crate::services::{icons, tray as tray_service};
use crate::theme::{Scheme, size, text};
use crate::ui::{self, Icon, icon, icon_button, label};
use iced::widget::{image, row};
use iced::{Alignment, Color, Element, Subscription, Task};

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
    /// Left click on a tray icon.
    TrayActivate(tray_service::Item),
}

/// A panel was just opened: reset its transient state and load what it needs.
pub fn opened(app: &mut App, kind: PanelKind) -> Task<AppMessage> {
    match kind {
        PanelKind::Calendar => app.panels.calendar = calendar::State::default(),
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
    }
}

fn tray_icon<'a>(s: Scheme, item: &tray_service::Item) -> Element<'a, AppMessage> {
    let px = 16.0;
    match &item.icon {
        Some(tray_service::TrayIcon::Name(n)) => match icons::lookup(n) {
            Some(p) => ui::image_file(&p, px),
            None => icon(Icon::AppWindow, px, s.ink_dim).into(),
        },
        Some(tray_service::TrayIcon::Rgba(p)) => {
            image(p.handle.clone()).width(px).height(px).into()
        }
        None => icon(Icon::AppWindow, px, s.ink_dim).into(),
    }
}

/// Bar buttons contributed by panels (right side, before the power button).
pub fn bar_items<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, AppMessage>> {
    let id = o.id;
    let mut items: Vec<Element<'a, AppMessage>> = Vec::new();
    let pill = |active: bool| {
        (
            if active {
                s.accent_soft
            } else {
                Color::TRANSPARENT
            },
            if active { s.accent_soft } else { s.raised_hi },
            s.raised_press,
        )
    };

    if let Some(p) = app.media.main() {
        let active = open == Some(PanelKind::Media);
        let (a, b, c) = pill(active);
        items.push(
            ui::control(media::chip(s, p, active))
                .padding([0, 10])
                .height(size::CONTROL)
                .radius(size::CONTROL / 2.0)
                .colors(a, b, c)
                .on_press_at(move |r| AppMessage::Toggle(PanelKind::Media, id, r))
                .into(),
        );
    }

    for item in app.tray.items.iter().filter(|i| !i.passive) {
        let active = open == Some(PanelKind::Tray)
            && app
                .panels
                .tray
                .item
                .as_ref()
                .is_some_and(|t| t.key == item.key);
        let (a, b, c) = pill(active);
        let it = item.clone();
        let it2 = item.clone();
        let menu_first = item.item_is_menu;
        let mut ctl =
            ui::control(iced::widget::container(tray_icon(s, item)).center(size::CONTROL))
                .radius(size::CONTROL / 2.0)
                .colors(a, b, c)
                .on_right_press(AppMessage::TrayMenu(it2, id, None));
        ctl = if menu_first {
            let it3 = item.clone();
            ctl.on_press_at(move |r| AppMessage::TrayMenu(it3.clone(), id, Some(r)))
        } else {
            ctl.on_press(AppMessage::Panel(Message::TrayActivate(it)))
        };
        items.push(ctl.into());
    }

    // System load: CPU, memory, temperature. Click for the full monitor.
    {
        let sys = app.system;
        let warn = |hot: bool| if hot { s.warn } else { s.ink_dim };
        let mut r = row![
            icon(Icon::Cpu, 14.0, warn(sys.cpu >= 85)),
            label(format!("{}%", sys.cpu), text::SMALL, warn(sys.cpu >= 85)),
            icon(Icon::MemoryStick, 14.0, warn(sys.mem >= 85)),
            label(format!("{}%", sys.mem), text::SMALL, warn(sys.mem >= 85)),
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        if let Some(t) = sys.temp {
            r = r
                .push(icon(Icon::Thermometer, 14.0, warn(t >= 85)))
                .push(label(format!("{t}°"), text::SMALL, warn(t >= 85)));
        }
        items.push(
            ui::control(r)
                .padding([0, 10])
                .height(size::CONTROL)
                .radius(size::CONTROL / 2.0)
                .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
                .on_press(AppMessage::Dispatch(crate::services::apps::launch_lua(
                    "kitty -e btop",
                )))
                .into(),
        );
    }

    // Privacy: something is recording from a microphone.
    if !app.audio.recording.is_empty() {
        let who = app.audio.recording.join(", ");
        items.push(
            iced::widget::tooltip(
                iced::widget::container(icon(Icon::Mic, 14.0, s.base))
                    .center(22)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(s.warn.into()),
                        border: iced::Border::default().rounded(11),
                        ..iced::widget::container::Style::default()
                    }),
                iced::widget::container(label(
                    format!("Microphone in use: {who}"),
                    text::SMALL,
                    s.ink,
                ))
                .padding([6, 10])
                .style(move |_| iced::widget::container::Style {
                    background: Some(s.raised.into()),
                    border: iced::Border::default().rounded(size::CONTROL_RADIUS),
                    ..iced::widget::container::Style::default()
                }),
                iced::widget::tooltip::Position::Left,
            )
            .into(),
        );
    }

    // Status cluster → control centre.
    let active = open == Some(PanelKind::Control);
    let fg = if active { s.accent } else { s.ink };
    let mut status = row![].spacing(10).align_y(Alignment::Center);
    if app.net.available {
        status = status.push(icon(control::wifi_icon(&app.net), size::ICON, fg));
    }
    if app.bt.powered {
        let i = if app.bt.connected().next().is_some() {
            Icon::BluetoothConnected
        } else {
            Icon::Bluetooth
        };
        status = status.push(icon(i, size::ICON, fg));
    }
    status = status.push(icon(control::volume_icon(app.audio.sink()), size::ICON, fg));
    if app.power.has_battery {
        let low = app.power.percent < 15.0 && !app.power.charging;
        status = status.push(
            row![
                icon(
                    control::battery_icon(&app.power),
                    size::ICON,
                    if low { s.bad } else { fg }
                ),
                label(
                    format!("{:.0}%", app.power.percent),
                    text::SMALL,
                    if low { s.bad } else { fg }
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    let (a, b, c) = pill(active);
    items.push(
        ui::control(status)
            .padding([0, 10])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .colors(a, b, c)
            .on_press_at(move |r| AppMessage::Toggle(PanelKind::Control, id, r))
            .on_scroll(|lines| AppMessage::VolumeStep(lines))
            .into(),
    );

    let bell = if app.dnd {
        Icon::BellOff
    } else if app.unread {
        Icon::BellDot
    } else {
        Icon::Bell
    };
    items.push(
        icon_button(
            s,
            bell,
            AppMessage::Close,
            open == Some(PanelKind::Notifications),
        )
        .on_press_at(move |r| AppMessage::Toggle(PanelKind::Notifications, id, r))
        .into(),
    );
    items
}
