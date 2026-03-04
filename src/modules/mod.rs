//! Bar modules: every item in the bar is a module with an id. `[bar] left/center/right` in the
//! config lists which modules show and in what order; a module not listed is not shown and the
//! services only it needs are never started.

pub mod island;

use crate::app::{App, Message, Output};
use crate::config::BarConfig;
use crate::panels::{Message as PanelMessage, PanelKind, control, media};
use crate::services::{icons, tray as tray_service};
use crate::shell::bar::{SLOT, SLOT_GAP};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, icon, label, label_w};
use iced::widget::{Space, container, image, pin, row, stack};
use iced::{Alignment, Element};

/// Where a module lives in the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Home {
    Left,
    /// The middle, in the order of `MODULES` (so switching one never shuffles the others).
    Center,
    Right,
}

/// What the shell knows about a module: its id (as in config.toml), where it goes, and the
/// label of its switch in Settings (None: part of the bar's structure, always there).
pub struct Info {
    pub id: &'static str,
    pub home: Home,
    pub setting: Option<&'static str>,
}

/// Every built-in module, the one list the bar, Settings and the Lua layer read.
#[rustfmt::skip]
pub const MODULES: &[Info] = &[
    Info { id: "workspaces", home: Home::Left, setting: None },
    Info { id: "app", home: Home::Left, setting: Some("Focused app name") },
    Info { id: "media", home: Home::Center, setting: Some("Music as its own bubble") },
    Info { id: "island", home: Home::Center, setting: None },
    Info { id: "clock", home: Home::Center, setting: None },
    Info { id: "weather", home: Home::Center, setting: Some("Weather as its own circle") },
    Info { id: "airpods", home: Home::Center, setting: Some("AirPods battery circle") },
    Info { id: "tray", home: Home::Right, setting: Some("Tray icons") },
    Info { id: "system", home: Home::Right, setting: Some("CPU and memory") },
    Info { id: "updates", home: Home::Right, setting: Some("Pending updates") },
    Info { id: "privacy", home: Home::Right, setting: Some("Microphone and screen in use") },
    Info { id: "status", home: Home::Right, setting: Some("Network, sound and battery") },
    Info { id: "notifications", home: Home::Right, setting: None },
    Info { id: "power", home: Home::Right, setting: Some("Power") },
];

/// Whether `id` is a built-in module.
pub fn known(id: &str) -> bool {
    MODULES.iter().any(|m| m.id == id)
}

/// Where `id` sorts among its section's built-in modules.
pub fn rank(id: &str) -> usize {
    MODULES
        .iter()
        .position(|m| m.id == id)
        .unwrap_or(MODULES.len())
}

/// Which optional services the configured layout needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub media: bool,
    pub tray: bool,
    pub network: bool,
    pub system: bool,
    pub updates: bool,
    pub weather: bool,
    pub airpods: bool,
}

pub fn needs(bar: &BarConfig) -> Needs {
    let on = |id: &str| bar.contains(id);
    // The island shows what is live (music, games) and opens the dashboard (weather, system).
    let island = on("island");
    Needs {
        media: on("media") || island,
        tray: on("tray"),
        // The control centre (status) shows Wi-Fi, Bluetooth and VPN.
        network: on("status"),
        system: on("system") || island,
        updates: on("updates"),
        weather: on("weather") || island,
        airpods: on("airpods"),
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
    // A module that panics is switched off; the rest of the bar keeps drawing.
    crate::guard::run(
        &format!("module {id}"),
        || items_unguarded(id, app, o, s, open),
        Vec::new,
    )
}

fn items_unguarded<'a>(
    id: &str,
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    match id {
        "workspaces" => vec![workspaces(app, o, s)],
        // Nothing focused: no pill at all, not an empty one.
        "app" if app.hypr.focused_monitor != o.name || app.hypr.window_class.is_empty() => {
            Vec::new()
        }
        "app" => vec![app_name(app, o, s)],
        // The island is the time pill; a clock beside it would show the time twice.
        "clock" if app.config.bar.contains("island") => Vec::new(),
        "clock" => vec![clock(app, o, s, open)],
        "airpods" => pods(app, o, s, open).into_iter().collect(),
        "island" => island::items(app, o, s, open),
        // Pill style: the music is a round bubble (one thing, one circle).
        "media" if app.config.bar.style == crate::config::BarStyle::Pills => {
            island::bubble(app, o, s).into_iter().collect()
        }
        "media" => media_chip(app, o, s, open),
        "tray" => tray(app, o, s, open),
        "weather" => weather(app, o, s, open),
        "system" => system(app, o, s, open),
        "updates" => updates(app, o, s, open),
        "privacy" => privacy(app, o, s, open),
        "status" => status(app, o, s, open),
        // Notifications live in the control centre: their bell is in its pill (see `status`).
        "notifications" => Vec::new(),
        "power" => vec![power(o, s, open)],
        // Anything else is a Lua widget (shown once it has produced something).
        other => app
            .chips
            .get_key_value(other)
            .map(|(name, c)| vec![lua_chip(name, c, s)])
            .unwrap_or_default(),
    }
}

/// A Lua widget: an optional icon and a label; clicking runs its `click`.
fn lua_chip<'a>(name: &str, c: &'a crate::lua::Chip, s: Scheme) -> Element<'a, Message> {
    let mut r = row![].spacing(6).align_y(Alignment::Center);
    // A name from kestrel's icon set, else an icon theme name or a path.
    if let Some(name) = c.icon.as_deref() {
        if let Some(i) = Icon::named(name) {
            r = r.push(icon(i, size::ICON, s.ink));
        } else if let Some(p) = icons::lookup(name) {
            r = r.push(ui::image_file(&p, size::ICON));
        }
    }
    if !c.text.is_empty() {
        r = r.push(label(
            ui::fit(&c.text, text::SMALL, 220.0),
            text::SMALL,
            s.ink,
        ));
    }
    ui::control(r)
        .padding([0, 10])
        .height(size::CONTROL)
        .radius(size::CONTROL / 2.0)
        .bar(s, false)
        .on_press(Message::LuaClick(name.to_string()))
        .into()
}

fn power<'a>(o: &'a Output, s: Scheme, open: Option<PanelKind>) -> Element<'a, Message> {
    let id = o.id;
    ui::icon_button_in(
        s,
        Icon::Power,
        Message::Close,
        open == Some(PanelKind::Power),
        // Plain like the other status icons; the power menu itself asks before anything
        // destructive.
        s.ink,
    )
    .bar(s, open == Some(PanelKind::Power))
    .on_press_at(move |b| Message::Toggle(PanelKind::Power, id, b))
    .into()
}

pub fn tray_icon<'a>(s: Scheme, item: &tray_service::Item) -> Element<'a, Message> {
    tray_icon_at(s, item, 16.0)
}

/// A tray app's icon at `px`.
pub fn tray_icon_at<'a>(s: Scheme, item: &tray_service::Item, px: f32) -> Element<'a, Message> {
    match &item.icon {
        Some(tray_service::TrayIcon::Name(n)) => match icons::lookup(n) {
            Some(p) => ui::image_file_faded(&p, px, s.opacity),
            None => icon(Icon::AppWindow, px, s.ink_dim).into(),
        },
        Some(tray_service::TrayIcon::Rgba(p)) => image(crate::ui::pixels_at(p, px))
            .width(px)
            .height(px)
            .opacity(s.opacity)
            .into(),
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
    if let Some(p) = app.media.main() {
        let active = open == Some(PanelKind::Media);
        items.push(
            ui::control(media::chip(s, p, active))
                .padding([0, 10])
                .height(size::CONTROL)
                .radius(size::CONTROL / 2.0)
                .bar(s, active)
                .on_press_at(move |r| Message::Toggle(PanelKind::Media, id, r))
                .into(),
        );
    }

    items
}
/// Tray icons: click activates, right click opens the app's menu (with "Keep in the bar").
/// Apps not kept in the bar hide behind the arrow; the arrow slides them out inline, in the
/// bar itself (like macOS's Ice), and tucks them away again. No panel to open.
fn tray<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let tray_open = open == Some(PanelKind::Tray);
    let pins = &app.panels.tray.pins;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    let mut hidden = 0;
    // Idle ("passive") apps too: they live behind the arrow like the rest.
    for item in &app.tray.items {
        let pinned = pins.contains(&crate::panels::tray::pin_key(item));
        if !pinned {
            hidden += 1;
            if !app.tray_expanded {
                continue;
            }
        }
        let active = tray_open && crate::panels::tray::showing(app, Some(item));
        let menu = item.clone();
        let mut ctl =
            ui::control(iced::widget::container(tray_icon(s, item)).center(size::CONTROL))
                .radius(size::CONTROL / 2.0)
                .bar(s, active)
                .on_right_press(Message::TrayMenu(Some(menu), id, None));
        ctl = if item.item_is_menu {
            let it = item.clone();
            ctl.on_press_at(move |r| Message::TrayMenu(Some(it.clone()), id, Some(r)))
        } else {
            ctl.on_press(Message::Panel(PanelMessage::TrayActivate(item.clone())))
        };
        items.push(ctl.into());
    }
    if hidden > 0 {
        let out = app.tray_expanded;
        items.push(
            ui::control(
                iced::widget::container(icon(
                    if out {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronLeft
                    },
                    14.0,
                    s.ink_dim,
                ))
                .center(size::CONTROL),
            )
            .width(22)
            .radius(size::CONTROL / 2.0)
            .bar(s, false)
            .on_press(Message::TrayExpand)
            .into(),
        );
    }
    items
}
/// Weather now, as a circle; opens the weather panel.
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
    // Weather now: the condition's icon in its colour and the temperature in white, on the
    // same pill as everything else (one look across the bar, design call 2026-11-13).
    if let Some(w) = &app.weather {
        let active = open == Some(PanelKind::Weather);
        items.push(
            ui::control(
                row![
                    icon(
                        crate::ui::weather_icon(w),
                        size::ICON,
                        crate::ui::weather_tint(w)
                    ),
                    label_w(app.temp(w.temp), text::BODY, s.ink, theme::SEMIBOLD),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([0, 12])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .bar(s, active)
            .on_press_at(move |r| Message::Toggle(PanelKind::Weather, id, r))
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
                .bar(s, false)
                .on_press(Message::Dispatch(
                    crate::services::compositor::Action::Exec(String::from("kitty -e btop")),
                ))
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
    // Pending updates: click to run them in a terminal.
    if app.updates.total() > 0 {
        let u = app.updates;
        items.push(
            ui::control(
                row![
                    icon(Icon::ArrowDownToLine, 14.0, s.warn),
                    label(u.total().to_string(), text::SMALL, s.warn),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([0, 10])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .bar(s, false)
            .on_press(Message::Dispatch(crate::services::compositor::Action::Exec(String::from(
                "kitty --title Updates sh -c 'yay -Syu; echo; read -p \"Done. Press Enter to close.\" _'",
            ))))
            .into(),
        );
    }

    items
}
/// Shown while an app records from a microphone, or the screen is recorded or shared.
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
    // Recording (wf-recorder): its own pill, a red dot and the time it has run, like a
    // phone's; a click stops it and the island says where it was saved.
    if app.hypr.screencast && app.hypr.recording.is_some() {
        let secs = app
            .hypr
            .recording_since
            .and_then(|t| t.elapsed().ok())
            .map_or(0, |d| d.as_secs());
        let time = if secs >= 3600 {
            format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
        } else {
            format!("{}:{:02}", secs / 60, secs % 60)
        };
        let red = s.fade(iced::Color::from_rgb8(0xFF, 0x45, 0x3A));
        let dot = iced::widget::container(Space::new())
            .width(8)
            .height(8)
            .style(move |_| iced::widget::container::Style {
                background: Some(red.into()),
                border: iced::Border::default().rounded(4),
                ..iced::widget::container::Style::default()
            });
        // Digits keep their place as they count (a fixed width per length).
        let digits = time.len() as f32 * 7.6;
        items.push(
            ui::control(
                row![
                    dot,
                    iced::widget::container(label_w(time, text::BODY, s.ink, theme::SEMIBOLD))
                        .width(digits)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([0, 12])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .bar(s, true)
            .on_press(Message::Dispatch(
                crate::services::compositor::Action::Exec("pkill -INT -x wf-recorder".into()),
            ))
            .into(),
        );
    } else if app.hypr.screencast {
        // Shared (a call or a browser showing the screen): the accent mark.
        items.push(indicator(s, Icon::ScreenShare));
    }
    // Privacy: something is recording from a microphone.
    if !app.audio.recording.is_empty() {
        items.push(indicator(s, Icon::Mic));
    }

    items
}

/// A privacy dot: an accent circle with what is in use. No hover text (design call 2026-11-13).
fn indicator<'a>(s: Scheme, i: Icon) -> Element<'a, Message> {
    iced::widget::container(icon(i, 14.0, s.on_accent))
        .center(22)
        .style(move |_| iced::widget::container::Style {
            background: Some(s.accent.into()),
            border: iced::Border::default().rounded(11),
            ..iced::widget::container::Style::default()
        })
        .into()
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
    // Status cluster → control centre.
    let active = open == Some(PanelKind::Control);
    let fg = s.ink;
    let mut status = row![].spacing(10).align_y(Alignment::Center);
    // Plain white icons (design call): only a nearly empty battery turns red. What is in progress
    // breathes (charging, Wi-Fi connecting).
    let alive = |on: bool| if on { fg.scale_alpha(app.breath()) } else { fg };
    if app.net.available {
        status = status.push(icon(
            control::wifi_icon(&app.net),
            size::ICON,
            alive(app.net.connecting),
        ));
    }
    if app.bt.powered {
        let i = if app.bt.connected().next().is_some() {
            Icon::BluetoothConnected
        } else {
            Icon::Bluetooth
        };
        status = status.push(icon(i, size::ICON, fg));
    }
    status = status.push(icon(control::level_icon(app.audio.sink()), size::ICON, fg));
    if app.power.has_battery {
        let low = app.power.percent < 15.0 && !app.power.charging;
        let charging = app.power.charging && app.power.percent < 99.5;
        let c = if low { s.bad } else { alive(charging) };
        let mut battery = row![icon(control::battery_icon(&app.power), size::ICON, c)]
            .spacing(5)
            .align_y(Alignment::Center);
        if app.config.bar.battery_percent {
            battery = battery.push(label(
                format!("{:.0}%", app.power.percent),
                text::SMALL,
                if low { s.bad } else { fg },
            ));
        }
        status = status.push(battery);
    }
    // Notifications are part of the control centre: a bell joins its pill when something
    // arrived unseen (with the count), or while Do not disturb is on.
    if app.dnd {
        status = status.push(icon(Icon::BellOff, size::ICON, fg));
    } else if app.unseen > 0 {
        status = status.push(
            row![
                icon(Icon::BellDot, size::ICON, s.accent),
                label_w(
                    if app.unseen > 9 {
                        "9+".to_string()
                    } else {
                        app.unseen.to_string()
                    },
                    text::SMALL,
                    s.accent,
                    theme::SEMIBOLD
                ),
            ]
            .spacing(3)
            .align_y(Alignment::Center),
        );
    }
    items.push(
        ui::control(status)
            .padding([0, 10])
            .height(size::CONTROL)
            .radius(size::CONTROL / 2.0)
            .bar(s, active)
            .on_press_at(move |r| Message::Toggle(PanelKind::Control, id, r))
            .on_scroll(Message::VolumeStep)
            .into(),
    );

    items
}
/// How much wider the current workspace's capsule is than a slot.
const WS_EXTRA: f32 = 14.0;

/// Workspaces, GNOME-style: a dot for each empty one, the app's icon where something is open,
/// and the current one a wider accent capsule. As it moves, the slot it leaves narrows and the
/// one it reaches widens, both following one spring, so the row flows like liquid instead of
/// a highlight jumping over fixed slots.
fn workspaces<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let n = crate::shell::bar::slot_count(&app.hypr, app.config.workspaces);
    // Where the capsule is, in slots (0 = the first), between two while it moves.
    let p = (o.pill_x.value / (SLOT + SLOT_GAP)).clamp(0.0, n.saturating_sub(1) as f32);
    // How much of the capsule is on slot i (the two it is between share it).
    let on = |i: usize| (1.0 - (i as f32 - p).abs()).clamp(0.0, 1.0);
    let widths: Vec<f32> = (0..n).map(|i| SLOT + WS_EXTRA * on(i)).collect();
    let slots = row((1..=n as i32).map(|id| {
        let i = (id - 1) as usize;
        let over = on(i);
        // Dark on the capsule, white off it, changing as it passes.
        let ink = crate::theme::color::mix(s.ink, s.base, over);
        let ws = app
            .hypr
            .workspaces
            .iter()
            .find(|w| w.id == id && w.windows > 0);
        let icon_of = ws.filter(|w| !w.app.is_empty()).and_then(|w| {
            crate::services::icons::lookup(&crate::services::apps::icon_for_class(&w.app))
        });
        let content: Element<'a, Message> = match (icon_of, ws) {
            (Some(path), _) => ui::image_file(&path, 16.0),
            // Something open without an icon, or the current empty one: its number.
            (None, Some(_)) => label_w(id.to_string(), text::SMALL, ink, theme::SEMIBOLD).into(),
            (None, None) if over > 0.5 => {
                label_w(id.to_string(), text::SMALL, ink, theme::SEMIBOLD).into()
            }
            // Empty: a small dot, brighter as the capsule nears.
            (None, None) => {
                let dot = crate::theme::color::mix(s.ink_faint, s.base, over);
                container(Space::new())
                    .width(5)
                    .height(5)
                    .style(move |_| container::Style {
                        background: Some(dot.into()),
                        border: iced::Border::default().rounded(2.5),
                        ..container::Style::default()
                    })
                    .into()
            }
        };
        // No fill of its own on hover or press (the capsule is the only shape here).
        ui::control(container(content).center_x(widths[i]).center_y(SLOT))
            .radius(SLOT / 2.0)
            .on_press(Message::Dispatch(
                crate::services::compositor::Action::FocusWorkspace(id),
            ))
            .on_scroll(|lines| {
                Message::Dispatch(crate::services::compositor::Action::CycleWorkspace(
                    if lines > 0.0 { -1 } else { 1 },
                ))
            })
            .into()
    }))
    .spacing(SLOT_GAP);
    // The capsule's left edge: past the slots before it, part way into the one it leaves.
    let i0 = p.floor() as usize;
    let before: f32 = widths.iter().take(i0).map(|w| w + SLOT_GAP).sum();
    let x = before + (p - i0 as f32) * (widths.get(i0).copied().unwrap_or(SLOT) + SLOT_GAP);
    let capsule = container(Space::new())
        .width(SLOT + WS_EXTRA)
        .height(SLOT)
        .style(move |_| container::Style {
            // White, like GNOME's: clear on any look without adding a colour (the accent
            // capsule was "too much", design call 2026-11-13).
            background: Some(s.ink.into()),
            border: iced::Border::default().rounded(SLOT / 2.0),
            ..container::Style::default()
        });
    let width = n as f32 * (SLOT + SLOT_GAP) - SLOT_GAP + WS_EXTRA;
    stack![pin(capsule).x(x).width(width).height(SLOT), slots]
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
    container(label(name, text::SMALL, s.ink))
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
    let time = app.time_text();
    let date = app.now.format("%a %-d %b").to_string();
    let id = o.id;
    let mut r = row![label_w(time, text::BODY, s.ink, theme::SEMIBOLD)]
        .spacing(8)
        .align_y(Alignment::Center);
    if app.config.clock.date {
        r = r.push(label(date, text::SMALL, s.ink));
    }
    if let Some(v) = island::visualiser(app, s) {
        r = r.push(v);
    }
    ui::control(r)
        .padding([0, 12])
        .height(size::CONTROL)
        .radius(size::CONTROL / 2.0)
        .bar(s, active)
        .on_press_at(move |b| Message::Toggle(PanelKind::Calendar, id, b))
        .into()
}

/// AirPods, while connected: a circle whose edge is a ring filled to the lower bud's charge
/// (green while charging, amber then red when low), headphones inside. Opens their panel.
fn pods<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Option<Element<'a, Message>> {
    crate::panels::pods::device(app)?;
    let id = o.id;
    let d = size::BAR - 6.0;
    let active = open == Some(PanelKind::Pods);
    let charge = app.pods.as_ref().and_then(|p| {
        Some(crate::services::airpods::Charge {
            level: p.level()?,
            charging: p.charging(),
        })
    });
    // The circle's edge is the gauge: a thin ring filled to the charge, headphones inside.
    let (value, tint) = match charge {
        Some(c) => (f32::from(c.level) / 100.0, crate::panels::pods::tint(s, c)),
        None => (0.0, s.ink_faint),
    };
    Some(
        // Inset from the circle's edge: a band of the pill's black keeps the ring apart from
        // the wallpaper (a ring on the very edge melted into a wallpaper of the same colour).
        ui::control(stack![
            container(ui::ring::ring_on(
                value,
                d - 6.0,
                3.0,
                tint,
                s.ink.scale_alpha(0.16 * s.opacity),
                crate::theme::surface(s),
            ))
            .center(d),
            container(icon(Icon::Headphones, 14.0, s.ink)).center(d),
        ])
        .width(d)
        .height(d)
        .center_x()
        .radius(d / 2.0)
        .bar(s, active)
        .on_press_at(move |r| Message::Toggle(PanelKind::Pods, id, r))
        .into(),
    )
}
