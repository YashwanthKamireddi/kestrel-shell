//! The control centre's main page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn main_page<'a>(cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    // Header: battery summary and session buttons.
    let p = &cx.power;
    let summary: Element<'_, Message> = if p.has_battery {
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
                tok::ICON,
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
    // Lined up with the tiles under it: the battery with their icon circles, the lock glyph's
    // right edge as far in from their right edge (its round button pads the glyph).
    let header = row![
        container(summary)
            .width(Length::Fill)
            .padding(iced::Padding {
                left: ui::TILE_INSET,
                ..iced::Padding::ZERO
            }),
        icon_button(s, Icon::Settings, Message::Toggle("settings"), false),
        icon_button(s, Icon::Camera, Message::Toggle("capture"), false),
        icon_button(s, Icon::Image, Message::Toggle("wallpaper"), false),
        icon_button(s, Icon::Lock, Message::Lock, false),
    ]
    .spacing(tok::XXS)
    .padding(iced::Padding {
        right: ui::TILE_INSET - (tok::BAR_ITEM - tok::ICON) / 2.0,
        ..iced::Padding::ZERO
    })
    .align_y(Alignment::Center);

    let tiles: Vec<Element<'_, Message>> = tiles(cx)
        .into_iter()
        .map(|t| {
            tile(
                s,
                ui::Tile {
                    icon: t.icon,
                    title: t.title,
                    status: t.status,
                    on: t.on,
                    press: t.press,
                    details: t.details,
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

    let mut levels = column![].spacing(tok::M);
    let sink = cx.audio.sink();
    if let Some(d) = sink {
        let top = cx.boost_top();
        levels = levels.push(level_row_to(
            s,
            level_icon(Some(d)),
            if d.muted { 0.0 } else { d.volume.min(top) },
            top,
            Message::SinkVolume,
            Message::SinkMute,
            Trail::Open(Box::new(Message::Page(Page::Output))),
        ));
    }
    if let Some(d) = cx.audio.source() {
        levels = levels.push(level_row(
            s,
            if d.muted { Icon::MicOff } else { Icon::Mic },
            if d.muted { 0.0 } else { d.volume.min(1.0) },
            Message::SourceVolume,
            Message::SourceMute,
            // The Sound page has the microphones too.
            Trail::Open(Box::new(Message::Page(Page::Output))),
        ));
    }
    // Brightness: the laptop panel's backlight while that screen is on; otherwise (docked to
    // a monitor kestrel cannot set) the picture is dimmed instead, with night light's tool.
    // Brightness: the laptop panel's backlight while that screen is on; otherwise an external
    // monitor's own (DDC/CI), or the picture dimmed instead, with night light's tool.
    let (level, set): (f32, fn(f32) -> Message) = match cx.light {
        Light::Backlight(v) => (v, Message::Brightness),
        Light::Monitor(v) => (v, Message::DdcBrightness),
        Light::Dim(v) => (v, Message::Dim),
    };
    levels = levels.push(level_row(
        s,
        Icon::Sun,
        level,
        set,
        Message::BrightnessDone,
        Trail::Room,
    ));

    let mut page = column![header, grid, card(s, levels)].spacing(tok::L);
    // Apps running in the background (the system tray), here with the rest of the system: a
    // click opens the app, a right click its menu.
    if !cx.tray.items.is_empty() {
        let mut icons = row![].spacing(tok::XXS).align_y(Alignment::Center);
        for item in &cx.tray.items {
            let menu = item.clone();
            let mut c = ui::control(
                container(crate::modules::tray_icon_at(s, item, tok::ICON_L))
                    .center(tok::BAR_ITEM + 4.0),
            )
            .radius((tok::BAR_ITEM + 4.0) / 2.0)
            .kind(s, crate::ui::control::Kind::Ghost);
            c = c.on_right_press(Message::TrayMenu(menu));
            c = c.on_press(Message::TrayActivate(item.clone()));
            icons = icons.push(c);
        }
        // A section like Notifications under it: its heading, then the icons on the tiles'
        // surface (bare, a lone icon floated in the panel and read as something stray).
        let h = tok::BAR_ITEM + 12.0;
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
    // Notifications live at the island now (their own panel under it, his call 2026-11-16).
    page.into()
}

/// The button at a level row's start (mute), and the room its on-fill copy keeps for it.
const ICON_BOX: f32 = 32.0;

/// What sits at a level row's end: nothing (the Sound page's full-width rows), the room a
/// button takes (so rows with and without one line up), or the button opening its page.
pub(super) enum Trail {
    Bare,
    Room,
    Open(Box<Message>),
}

pub(super) fn level_row<'a>(
    s: Scheme,
    i: Icon,
    value: f32,
    on_change: fn(f32) -> Message,
    on_icon: Message,
    details: Trail,
) -> Element<'a, Message> {
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
) -> Element<'a, Message> {
    // The plain 0–100 slider until the volume is boosted (keys can go past 100 %); then the
    // track runs to the maximum with the boost in red.
    let top = if value > 1.0 { top.max(1.0) } else { 1.0 };
    let on_track = (value / top).clamp(0.0, 1.0);
    // The value in two colours, split where the fill's edge crosses it (the slider draws each
    // half from its own copy): dark on the fill, light past it (red when boosted).
    let boosted = value > 1.0 + 1e-4;
    let words = format!("{:.0}%", value * 100.0);
    let pct = label(
        words.clone(),
        text::SMALL,
        if boosted { s.bad } else { s.ink_dim },
    );
    let on_fill = row![
        Space::new().width(ICON_BOX),
        Space::new().width(Length::Fill),
        label(words, text::SMALL, s.on_accent),
        Space::new().width(tok::L),
    ]
    .align_y(Alignment::Center);
    let inner = row![
        ui::control(container(icon(i, tok::ICON, s.on_accent)).center(ICON_BOX))
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::OnAccent)
            .on_press(on_icon),
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
        move |v| on_change(v * top),
    )
    .on_fill(on_fill)
    .boost(1.0 / top, s.bad);
    let mut r = row![bar].spacing(tok::XS).align_y(Alignment::Center);
    // Every row keeps the details button's room, so the tracks all end at one line.
    match details {
        Trail::Bare => {}
        Trail::Room => r = r.push(Space::new().width(tok::BAR_ITEM)),
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
    pub(super) press: Message,
    pub(super) details: Option<Message>,
}

/// Every tile, in order. The view draws these and the keyboard acts on them, so the two
/// always agree.
pub(super) fn tiles(cx: &Cx<'_>) -> Vec<Spec> {
    let n = &cx.net;
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
    let b = &cx.bt;
    let bt_busy = b.devices.iter().find_map(|d| cx.in_progress(&d.path));
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
            press: Message::Wifi(!n.wifi_enabled),
            details: n.has_wifi.then_some(Message::Page(Page::Wifi)),
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
            press: Message::Bluetooth(!b.powered),
            details: b.available.then_some(Message::Page(Page::Bluetooth)),
        },
        Spec {
            icon: Icon::Plane,
            title: "Airplane mode".into(),
            status: on_off(crate::state::get(|s| s.toggles.airplane.is_some())),
            on: crate::state::get(|s| s.toggles.airplane.is_some()),
            press: Message::Airplane,
            details: None,
        },
        Spec {
            icon: if cx.dnd { Icon::BellOff } else { Icon::Bell },
            title: "Do not disturb".into(),
            status: if !cx.dnd {
                "Off".into()
            } else {
                match crate::state::get(|s| s.toggles.dnd_until) {
                    Some(t) => format!("Until {}", until_text(t)),
                    None => "On".into(),
                }
            },
            on: cx.dnd,
            press: Message::Dnd,
            details: Some(Message::Page(Page::Quiet)),
        },
        Spec {
            icon: Icon::Sunset,
            title: "Night light".into(),
            status: if cx.night_light {
                format!("{} K", cx.config.night_light_temperature)
            } else {
                "Off".into()
            },
            on: cx.night_light,
            press: Message::NightLight,
            details: Some(Message::Page(Page::NightLight)),
        },
        Spec {
            icon: Icon::Coffee,
            title: "Caffeine".into(),
            status: if cx.caffeine { "Staying awake" } else { "Off" }.into(),
            on: cx.caffeine,
            press: Message::Caffeine,
            details: None,
        },
    ];
    if let Some(prof) = cx.power.profile {
        v.push(Spec {
            icon: match prof {
                power::Profile::PowerSaver => Icon::Leaf,
                power::Profile::Balanced => Icon::Gauge,
                power::Profile::Performance => Icon::Rocket,
            },
            title: "Power mode".into(),
            status: prof.label().to_string(),
            on: prof != power::Profile::Balanced,
            press: Message::Profile,
            details: Some(Message::Page(Page::Power)),
        });
    }
    v.push(Spec {
        icon: Icon::Gamepad2,
        title: "Game mode".into(),
        status: if cx.game_mode { "Effects off" } else { "Off" }.into(),
        on: cx.game_mode,
        press: Message::GameMode,
        details: None,
    });
    if let Some(on) = cx.vpn.warp {
        v.push(Spec {
            icon: Icon::Cloud,
            title: "WARP".into(),
            status: if on { "Connected" } else { "Off" }.into(),
            on,
            press: Message::Warp(!on),
            details: Some(Message::Page(Page::Warp)),
        });
    }
    if let Some(on) = cx.vpn.tailscale {
        v.push(Spec {
            icon: Icon::Network,
            title: "Tailscale".into(),
            status: on_off(on),
            on,
            press: Message::Tailscale(!on),
            details: Some(Message::Page(Page::Tailscale)),
        });
    }
    v
}
