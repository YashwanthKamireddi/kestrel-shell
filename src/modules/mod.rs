//! Bar modules: every item in the bar is a module with an id. `[bar] left/center/right` in the
//! config lists which modules show and in what order; a module not listed is not shown and the
//! services only it needs are never started.

use crate::app::{App, Message, Output};
use crate::config::BarConfig;
use crate::panels::{Message as PanelMessage, PanelKind, control, media};
use crate::services::{hyprland, icons, tray as tray_service};
use crate::shell::bar::{SLOT, SLOT_GAP};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, icon, icon_button, label, label_w};
use iced::widget::{Space, container, image, pin, row, stack};
use iced::{Alignment, Color, Element};

/// Every module the bar knows, in their default order.
pub const KNOWN: &[&str] = &[
    "workspaces",
    "app",
    "clock",
    "media",
    "tray",
    "nba",
    "weather",
    "system",
    "updates",
    "privacy",
    "status",
    "notifications",
    "power",
];

/// Which optional services the configured layout needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub media: bool,
    pub tray: bool,
    pub network: bool,
    pub system: bool,
    pub updates: bool,
    pub weather: bool,
    pub nba: bool,
}

pub fn needs(bar: &BarConfig) -> Needs {
    let on = |id: &str| bar.contains(id);
    Needs {
        media: on("media"),
        tray: on("tray"),
        // The control centre (status) shows Wi-Fi, Bluetooth and VPN.
        network: on("status"),
        system: on("system"),
        updates: on("updates"),
        weather: on("weather"),
        nba: on("nba"),
    }
}

/// The bar items for module `id` (empty when it has nothing to show right now).
pub fn items<'a>(
    id: &str,
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    match id {
        "workspaces" => vec![workspaces(app, o, s)],
        "app" => vec![app_name(app, o, s)],
        "clock" => vec![clock(app, o, s, open)],
        "media" => media_chip(app, o, s, open),
        "tray" => tray(app, o, s, open),
        "nba" => nba(app, o, s, open),
        "weather" => weather(app, o, s, open),
        "system" => system(app, o, s, open),
        "updates" => updates(app, o, s, open),
        "privacy" => privacy(app, o, s, open),
        "status" => status(app, o, s, open),
        "notifications" => bell(app, o, s, open),
        "power" => vec![power(o, s, open)],
        other => {
            log::warn!(
                "bar: unknown module {other:?} (known: {})",
                KNOWN.join(", ")
            );
            Vec::new()
        }
    }
}

fn power<'a>(o: &'a Output, s: Scheme, open: Option<PanelKind>) -> Element<'a, Message> {
    let id = o.id;
    icon_button(
        s,
        Icon::Power,
        Message::Close,
        open == Some(PanelKind::Power),
    )
    .on_press_at(move |b| Message::Toggle(PanelKind::Power, id, b))
    .into()
}

fn tray_icon<'a>(s: Scheme, item: &tray_service::Item) -> Element<'a, Message> {
    let px = 16.0;
    match &item.icon {
        Some(tray_service::TrayIcon::Name(n)) => match icons::lookup(n) {
            Some(p) => ui::image_file(&p, px),
            None => icon(Icon::AppWindow, px, s.ink_dim).into(),
        },
        Some(tray_service::TrayIcon::Rgba(p)) => {
            image(crate::ui::pixels(p)).width(px).height(px).into()
        }
        None => icon(Icon::AppWindow, px, s.ink_dim).into(),
    }
}

/// Now playing: cover and title; opens the media panel.
fn media_chip<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
    if let Some(p) = app.media.main() {
        let active = open == Some(PanelKind::Media);
        let (a, b, c) = pill(active);
        items.push(
            ui::control(media::chip(s, p, active))
                .padding([0, 10])
                .height(size::CONTROL)
                .radius(size::CONTROL / 2.0)
                .colors(a, b, c)
                .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r))
                .into(),
        );
    }

    items
}
/// Tray icons: click activates, right click opens the app's menu.
fn tray<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
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
                .on_right_press(Message::TrayMenu(it2, id, None));
        ctl = if menu_first {
            let it3 = item.clone();
            ctl.on_press_at(move |r| Message::TrayMenu(it3.clone(), id, Some(r)))
        } else {
            ctl.on_press(Message::Panel(PanelMessage::TrayActivate(it)))
        };
        items.push(ctl.into());
    }

    items
}
/// A live NBA game (nothing when none is live).
fn nba<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
    // A live NBA game.
    if let Some(g) = app
        .nba
        .iter()
        .find(|g| g.phase == crate::services::web::Phase::Live)
    {
        items.push(
            ui::control(
                row![
                    icon(Icon::Trophy, 14.0, s.ink_dim),
                    label(
                        format!(
                            "{} {}–{} {}  {}",
                            g.away, g.away_score, g.home_score, g.home, g.detail
                        ),
                        text::SMALL,
                        s.ink
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([0, 10])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .on_press_at(move |r| Message::Toggle(PanelKind::Calendar, id, r))
            .into(),
        );
    }

    items
}
/// Weather now; the forecast is in the calendar.
fn weather<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
    // Weather now; the forecast is in the calendar.
    if let Some(w) = &app.weather {
        items.push(
            ui::control(
                row![
                    icon(crate::ui::weather_icon(w), 14.0, s.ink_dim),
                    label(format!("{:.0}°", w.temp), text::SMALL, s.ink_dim),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([0, 10])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .on_press_at(move |r| Message::Toggle(PanelKind::Calendar, id, r))
            .into(),
        );
    }

    items
}
/// CPU, memory and temperature; click for btop.
fn system<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
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
                .on_press(Message::Dispatch(crate::services::apps::launch_lua(
                    "kitty -e btop",
                )))
                .into(),
        );
    }

    items
}
/// Pending updates (nothing when there are none); click to run them.
fn updates<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
    // Pending updates: click to run them in a terminal.
    if app.updates.total() > 0 {
        let u = app.updates;
        items.push(
            ui::control(
                row![
                    icon(Icon::ArrowDownToLine, 14.0, s.ink),
                    label(u.total().to_string(), text::SMALL, s.ink),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([0, 10])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .on_press(Message::Dispatch(crate::services::apps::launch_lua(
                "kitty --title Updates sh -c 'yay -Syu; echo; read -p \"Done. Press Enter to close.\" _'",
            )))
            .into(),
        );
    }

    items
}
/// Shown while an app records from a microphone.
fn privacy<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
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

    items
}
/// Network, Bluetooth, volume and battery; opens the control centre.
fn status<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
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
            .on_press_at(move |r| Message::Toggle(PanelKind::Control, id, r))
            .on_scroll(Message::VolumeStep)
            .into(),
    );

    items
}
/// Notifications bell; opens the notification centre.
fn bell<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (&app, &o, &open);
    let id = o.id;
    let _ = id;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
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
    let _ = pill(false);
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
            Message::Close,
            open == Some(PanelKind::Notifications),
        )
        .on_press_at(move |r| Message::Toggle(PanelKind::Notifications, id, r))
        .into(),
    );
    items
}
/// Workspace slots: app icon or faint number, with a gliding highlight on the active one.
fn workspaces<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let n = crate::shell::bar::slot_count(&app.hypr, app.config.workspaces);
    let active = app.hypr.active_on(&o.name);
    // One slot per workspace: the icon of the app last used there, or a faint number when the
    // workspace is empty. A soft highlight glides to the active one.
    let slots = row((1..=n as i32).map(|id| {
        let ws = app
            .hypr
            .workspaces
            .iter()
            .find(|w| w.id == id && w.windows > 0);
        let content: Element<'a, Message> = match ws {
            Some(w) if !w.app.is_empty() => {
                match crate::services::icons::lookup(&crate::services::apps::icon_for_class(&w.app))
                {
                    Some(p) => ui::image_file(&p, 16.0),
                    None => label(id.to_string(), text::SMALL, s.ink).into(),
                }
            }
            Some(_) => label(id.to_string(), text::SMALL, s.ink).into(),
            None => label(
                id.to_string(),
                text::CAPTION,
                if active == Some(id) {
                    s.ink_dim
                } else {
                    s.ink_faint
                },
            )
            .into(),
        };
        ui::control(container(content).center(SLOT))
            .radius(size::CONTROL_RADIUS)
            .colors(
                iced::Color::TRANSPARENT,
                s.raised.scale_alpha(0.8),
                s.raised_press,
            )
            .on_press(Message::Dispatch(hyprland::focus_workspace(id)))
            .on_scroll(|lines| {
                Message::Dispatch(hyprland::cycle_workspace(if lines > 0.0 { -1 } else { 1 }))
            })
            .into()
    }))
    .spacing(SLOT_GAP);
    let highlight = container(Space::new())
        .width(SLOT)
        .height(SLOT)
        .style(move |_| container::Style {
            background: Some(s.raised_hi.into()),
            border: iced::Border::default().rounded(size::CONTROL_RADIUS),
            ..container::Style::default()
        });
    let width = n as f32 * (SLOT + SLOT_GAP) - SLOT_GAP;
    stack![
        pin(highlight).x(o.pill_x.value).width(width).height(SLOT),
        slots,
    ]
    .width(width)
    .height(SLOT)
    .into()
}

/// The focused window's app name (on the focused monitor only).
fn app_name<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let name = if app.hypr.focused_monitor == o.name {
        crate::services::apps::name_for_class(&app.hypr.window_class)
    } else {
        String::new()
    };
    container(label(name, text::SMALL, s.ink_dim))
        .padding([0, 6])
        .into()
}

/// Time and date; opens the calendar.
fn clock<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Element<'a, Message> {
    let active = open == Some(PanelKind::Calendar);
    let time = app.now.format("%H:%M").to_string();
    let date = app.now.format("%a %-d %b").to_string();
    let id = o.id;
    ui::control(
        row![
            label_w(
                time,
                text::BODY,
                if active { s.accent } else { s.ink },
                theme::SEMIBOLD
            ),
            label(date, text::SMALL, s.ink_dim),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .height(size::CONTROL)
    .radius(size::CONTROL / 2.0)
    .colors(
        if active {
            s.accent_soft
        } else {
            iced::Color::TRANSPARENT
        },
        if active { s.accent_soft } else { s.raised_hi },
        s.raised_press,
    )
    .on_press_at(move |b| Message::Toggle(PanelKind::Calendar, id, b))
    .into()
}
