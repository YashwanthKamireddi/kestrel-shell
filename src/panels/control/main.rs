//! The control centre's main page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn main_page(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    // Header: battery summary and session buttons.
    let p = &app.power;
    let summary: Element<'_, AppMessage> = if p.has_battery {
        let detail = if p.full {
            "Fully charged".to_string()
        } else if p.time_left > 0 {
            if p.charging {
                format!("{} until full", duration(p.time_left))
            } else {
                format!("{} left", duration(p.time_left))
            }
        } else if p.charging {
            "Charging".to_string()
        } else if p.held {
            "Charging on hold".to_string()
        } else {
            "On battery".to_string()
        };
        row![
            icon(
                battery_icon(p),
                size::ICON,
                if p.percent < 15.0 && !p.charging && !p.held {
                    s.bad
                } else {
                    s.ink
                }
            ),
            label_w(
                format!("{:.0}%", p.percent),
                text::BODY,
                s.ink,
                theme::MEDIUM
            ),
            label(detail, text::SMALL, s.ink_dim),
        ]
        .spacing(tok::M)
        .align_y(Alignment::Center)
        .into()
    } else {
        label_w("Quick settings", text::TITLE, s.ink, theme::MEDIUM).into()
    };
    // Lined up with the tiles under it: the battery with their icon circles (10 px in), the
    // lock glyph's right edge 10 px in from their right edge (its button pads it by 7).
    let header = row![
        container(summary)
            .width(Length::Fill)
            .padding(iced::Padding {
                left: 10.0,
                ..iced::Padding::ZERO
            }),
        icon_button(
            s,
            Icon::Settings,
            AppMessage::Ipc("toggle settings".into()),
            false
        ),
        icon_button(
            s,
            Icon::Camera,
            AppMessage::Ipc("toggle capture".into()),
            false
        ),
        icon_button(s, Icon::Image, AppMessage::OpenWallpaper, false),
        icon_button(
            s,
            Icon::Lock,
            AppMessage::Panel(PanelMessage::Power(crate::panels::power::Message::Press(
                crate::panels::power::Action::Lock
            ))),
            false
        ),
    ]
    .spacing(tok::XXS)
    .padding(iced::Padding {
        right: 3.0,
        ..iced::Padding::ZERO
    })
    .align_y(Alignment::Center);

    let focus = app.panels.control.focus;
    let tiles: Vec<Element<'_, AppMessage>> = tiles(app)
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            tile(
                s,
                ui::Tile {
                    icon: t.icon,
                    title: t.title,
                    status: t.status,
                    on: t.on,
                    press: t.press,
                    details: t.details,
                    focused: focus == Some(i),
                },
            )
        })
        .collect();
    let mut grid = column![].spacing(tok::M);
    let mut it = tiles.into_iter();
    while let Some(first) = it.next() {
        let second = it
            .next()
            .unwrap_or_else(|| Space::new().width(Length::Fill).into());
        grid = grid.push(row![first, second].spacing(tok::M));
    }

    let levels = levels(app, s);

    let mut page = column![header, grid, card(s, levels)].spacing(tok::L);
    // Apps running in the background (the system tray), here with the rest of the system: a
    // click opens the app, a right click its menu.
    if !app.tray.items.is_empty() {
        let out = app.panel.as_ref().map(|p| p.output);
        let mut icons = row![].spacing(tok::XXS).align_y(Alignment::Center);
        for item in &app.tray.items {
            let menu = item.clone();
            let mut c = ui::control(
                container(crate::modules::tray_icon_at(s, item, 20.0)).center(size::CONTROL + 4.0),
            )
            .radius((size::CONTROL + 4.0) / 2.0)
            .kind(s, crate::ui::control::Kind::Ghost);
            if let Some(o) = out {
                c = c.on_right_press(AppMessage::TrayMenu(Some(menu), o, None));
            }
            c = c.on_press(AppMessage::Panel(PanelMessage::TrayActivate(item.clone())));
            icons = icons.push(c);
        }
        // A section like Notifications under it: its heading, then the icons on the tiles'
        // surface (bare, a lone icon floated in the panel and read as something stray).
        let h = size::CONTROL + 12.0;
        page = page.push(
            column![
                container(label_w(
                    "Running in the background",
                    text::BODY,
                    s.ink,
                    theme::SEMIBOLD
                ))
                .padding([tok::NONE, tok::XS]),
                container(icons)
                    .padding([tok::XS, tok::XS])
                    .width(Length::Fill)
                    .height(h)
                    .align_y(Alignment::Center)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
                        border: iced::Border::default().rounded(h / 2.0),
                        ..iced::widget::container::Style::default()
                    }),
            ]
            .spacing(tok::M),
        );
    }
    // The tiles and sliders stay; only the notifications scroll, in what is left of the
    // screen under them (ii, Noctalia: the list is the part that moves).
    let room = app
        .panel
        .as_ref()
        .and_then(|p| app.output(p.output))
        .map_or(f32::INFINITY, |o| {
            o.size.height - crate::theme::size::BAR - 2.0 * tok::XL - 2.0 * tok::L
        });
    let list_room = (room - app.control_tiles_h - tok::L).max(160.0);
    column![
        crate::ui::measured::measured(page, AppMessage::ControlTiles),
        crate::panels::notifications::section(app, s, list_room),
    ]
    .spacing(tok::L)
    .into()
}

/// How far the output's volume may go (1.5 = 150 %): `[sound] max`, at least 100 %.
pub(super) fn boost_top(app: &App) -> f32 {
    (app.config.sound.max as f32 / 100.0).max(1.0)
}

/// What sits at a level row's end: nothing (the Sound page's full-width rows), the room a
/// button takes (so rows with and without one line up), or the button opening its page.
pub(super) enum Trail {
    Bare,
    Room,
    Open(Box<AppMessage>),
}

pub(super) fn level_row<'a>(
    s: Scheme,
    i: Icon,
    value: f32,
    on_change: fn(f32) -> Message,
    on_icon: Message,
    details: Trail,
) -> Element<'a, AppMessage> {
    level_row_to(s, i, value, 1.0, on_change, on_icon, details)
}

/// `level_row` whose track runs to `top` (1.5: 150 %), the boost past 100 % in the
/// danger colour, when the volume may go above 100 %.
pub(super) fn level_row_to<'a>(
    s: Scheme,
    i: Icon,
    value: f32,
    top: f32,
    on_change: fn(f32) -> Message,
    on_icon: Message,
    details: Trail,
) -> Element<'a, AppMessage> {
    // The plain 0–100 slider until the volume is boosted (keys can go past 100 %); then the
    // track runs to the maximum with the boost in red.
    let top = if value > 1.0 { top.max(1.0) } else { 1.0 };
    let on_track = (value / top).clamp(0.0, 1.0);
    // Once the fill reaches the value text, the text switches to the on-accent colour.
    let covered = on_track > 0.86;
    let pct = label(
        format!("{:.0}%", value * 100.0),
        text::SMALL,
        if covered { s.on_accent } else { s.ink_dim },
    );
    let inner = row![
        ui::control(container(icon(i, size::ICON, s.on_accent)).center(32))
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::OnAccent)
            .on_press(msg(on_icon)),
        Space::new().width(Length::Fill),
        pct,
        Space::new().width(tok::L),
    ]
    .align_y(Alignment::Center);
    let bar = slider(
        inner,
        on_track,
        32.0,
        (s.ink.scale_alpha(tok::TONAL * s.opacity), s.accent),
        move |v| msg(on_change(v * top)),
    )
    .boost(1.0 / top, s.bad);
    let mut r = row![bar].spacing(tok::XS).align_y(Alignment::Center);
    // Every row keeps the details button's room, so the tracks all end at one line.
    match details {
        Trail::Bare => {}
        Trail::Room => r = r.push(Space::new().width(size::CONTROL)),
        Trail::Open(d) => r = r.push(icon_button(s, Icon::ChevronRight, *d, false)),
    }
    r.into()
}

/// A quick-settings tile: what it shows and what it does.
pub(super) struct Spec {
    pub(super) icon: Icon,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) on: bool,
    pub(super) press: AppMessage,
    pub(super) details: Option<AppMessage>,
}

/// Every tile, in order. The view draws these and the keyboard acts on them, so the two
/// always agree.
pub(super) fn tiles(app: &App) -> Vec<Spec> {
    let n = &app.net;
    let (wifi_title, wifi_status) = match n.link {
        network::Link::Wired => ("Ethernet".to_string(), "Connected".to_string()),
        _ if !n.wifi_enabled => ("Wi-Fi".to_string(), "Off".to_string()),
        _ => (
            "Wi-Fi".to_string(),
            n.ssid.clone().unwrap_or_else(|| {
                if n.connecting {
                    "Connecting…".into()
                } else {
                    "Not connected".into()
                }
            }),
        ),
    };
    let b = &app.bt;
    let bt_busy = b.devices.iter().find_map(|d| in_progress(app, &d.path));
    let bt_status = if !b.powered {
        "Off".to_string()
    } else if let Some(want) = bt_busy {
        if want {
            "Connecting…"
        } else {
            "Disconnecting…"
        }
        .to_string()
    } else {
        let names: Vec<&str> = b.connected().map(|d| d.name.as_str()).collect();
        match names.len() {
            0 => "On".into(),
            1 => names[0].to_string(),
            k => format!("{k} devices"),
        }
    };
    let on_off = |on: bool| if on { "On" } else { "Off" }.to_string();
    let mut v = vec![
        Spec {
            icon: wifi_icon(n),
            title: wifi_title,
            status: wifi_status,
            on: n.wifi_enabled || n.link == network::Link::Wired,
            press: msg(Message::Wifi(!n.wifi_enabled)),
            details: n.has_wifi.then(|| msg(Message::Page(Page::Wifi))),
        },
        Spec {
            icon: if b.powered {
                Icon::Bluetooth
            } else {
                Icon::BluetoothOff
            },
            title: "Bluetooth".into(),
            status: bt_status,
            on: b.powered,
            press: msg(Message::Bluetooth(!b.powered)),
            details: b.available.then(|| msg(Message::Page(Page::Bluetooth))),
        },
        Spec {
            icon: Icon::Plane,
            title: "Airplane mode".into(),
            status: on_off(crate::state::get(|s| s.toggles.airplane.is_some())),
            on: crate::state::get(|s| s.toggles.airplane.is_some()),
            press: msg(Message::Airplane),
            details: None,
        },
        Spec {
            icon: if app.dnd { Icon::BellOff } else { Icon::Bell },
            title: "Do not disturb".into(),
            status: if !app.dnd {
                "Off".into()
            } else {
                match crate::state::get(|s| s.toggles.dnd_until) {
                    Some(t) => format!("Until {}", until_text(t)),
                    None => "On".into(),
                }
            },
            on: app.dnd,
            press: msg(Message::Dnd),
            details: Some(msg(Message::Page(Page::Quiet))),
        },
        Spec {
            icon: Icon::Sunset,
            title: "Night light".into(),
            status: if app.night_light {
                format!("{} K", app.config.night_light_temperature)
            } else {
                "Off".into()
            },
            on: app.night_light,
            press: msg(Message::NightLight),
            details: Some(msg(Message::Page(Page::NightLight))),
        },
        Spec {
            icon: Icon::Coffee,
            title: "Caffeine".into(),
            status: if app.caffeine { "Staying awake" } else { "Off" }.into(),
            on: app.caffeine,
            press: msg(Message::Caffeine),
            details: None,
        },
    ];
    if let Some(prof) = app.power.profile {
        v.push(Spec {
            icon: match prof {
                power::Profile::PowerSaver => Icon::Leaf,
                power::Profile::Balanced => Icon::Gauge,
                power::Profile::Performance => Icon::Rocket,
            },
            title: "Power mode".into(),
            status: prof.label().to_string(),
            on: prof != power::Profile::Balanced,
            press: msg(Message::Profile),
            details: Some(msg(Message::Page(Page::Power))),
        });
    }
    v.push(Spec {
        icon: Icon::Gamepad2,
        title: "Game mode".into(),
        status: if app.game_mode { "Effects off" } else { "Off" }.into(),
        on: app.game_mode,
        press: msg(Message::GameMode),
        details: None,
    });
    if let Some(on) = app.vpn.warp {
        v.push(Spec {
            icon: Icon::Cloud,
            title: "WARP".into(),
            status: if on { "Connected" } else { "Off" }.into(),
            on,
            press: msg(Message::Warp(!on)),
            details: Some(msg(Message::Page(Page::Warp))),
        });
    }
    if let Some(on) = app.vpn.tailscale {
        v.push(Spec {
            icon: Icon::Network,
            title: "Tailscale".into(),
            status: on_off(on),
            on,
            press: msg(Message::Tailscale(!on)),
            details: Some(msg(Message::Page(Page::Tailscale))),
        });
    }
    v
}

/// The level sliders: the output's volume, the microphone, the brightness. Shared by the
/// control centre and the island (hovering a volume or brightness change there grows into
/// these).
pub fn levels(app: &App, s: Scheme) -> iced::widget::Column<'_, AppMessage> {
    let mut levels = column![].spacing(tok::M);
    let sink = app.audio.sink();
    if let Some(d) = sink {
        let top = boost_top(app);
        levels = levels.push(level_row_to(
            s,
            level_icon(Some(d)),
            if d.muted { 0.0 } else { d.volume.min(top) },
            top,
            Message::SinkVolume,
            Message::SinkMute,
            Trail::Open(Box::new(msg(Message::Page(Page::Output)))),
        ));
    }
    if let Some(d) = app.audio.source() {
        levels = levels.push(level_row(
            s,
            if d.muted { Icon::MicOff } else { Icon::Mic },
            if d.muted { 0.0 } else { d.volume.min(1.0) },
            Message::SourceVolume,
            Message::SourceMute,
            // The Sound page has the microphones too.
            Trail::Open(Box::new(msg(Message::Page(Page::Output)))),
        ));
    }
    // Brightness: the laptop panel's backlight while that screen is on; otherwise (docked to
    // a monitor kestrel cannot set) the picture is dimmed instead, with night light's tool.
    match app
        .brightness
        .filter(|_| app.outputs.iter().any(|o| o.name.starts_with("eDP")))
    {
        Some(v) => {
            levels = levels.push(level_row(
                s,
                Icon::Sun,
                v,
                Message::Brightness,
                Message::BrightnessDone,
                Trail::Room,
            ));
        }
        // An external monitor that answers DDC/CI: its real backlight.
        None if app.ddc_target().is_some() => {
            let bus = app.ddc_target().unwrap_or_default();
            levels = levels.push(level_row(
                s,
                Icon::Sun,
                app.ddc_level.get(&bus).copied().unwrap_or(0.5),
                Message::DdcBrightness,
                Message::BrightnessDone,
                Trail::Room,
            ));
        }
        None => {
            levels = levels.push(level_row(
                s,
                Icon::Sun,
                app.dim,
                Message::Dim,
                Message::BrightnessDone,
                Trail::Room,
            ));
        }
    }
    levels
}
