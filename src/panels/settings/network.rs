//! Settings › network: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn network_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    use crate::panels::control::Message as C;
    let st = &app.panels.settings;
    let n = &app.net;
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    if !n.available {
        out.push(crate::ui::empty(s, Icon::WifiOff));
        return out;
    }
    let airplane = !n.wifi_enabled && !app.bt.powered;
    let mut top = vec![row_of(
        s,
        "Airplane mode",
        None,
        ui::switch(s, airplane, snd(C::Airplane)),
    )];
    if n.has_wifi {
        top.push(row_of(
            s,
            "Wi-Fi",
            None,
            ui::switch(s, n.wifi_enabled, snd(C::Wifi(!n.wifi_enabled))),
        ));
    }
    out.extend(group(s, None, top));

    // Wi-Fi: the network in use with its details, then the others in range.
    if n.has_wifi && n.wifi_enabled && !n.hotspot {
        // The network in use (with its details) and the others in range, apart.
        let mut current = Vec::new();
        let mut rows = Vec::new();
        let pw = &app.panels.control.password;
        for net in n.networks.iter().take(14) {
            if !matches(&net.ssid) && !matches("wi-fi") {
                continue;
            }
            let busy = crate::panels::control::in_progress(app, &net.ssid);
            let failed = crate::panels::control::st_net_error(app, &net.ssid).map(|(_, e)| e);
            let state: String = match busy {
                Some(true) => "Connecting…".into(),
                Some(false) => "Disconnecting…".into(),
                None if net.active => "Connected".into(),
                None if failed.is_some() => failed.clone().unwrap_or_default(),
                None if net.known => "Saved".into(),
                None => String::new(),
            };
            let lead = icon(
                crate::panels::control::strength_icon(net.strength),
                16.0,
                if net.active { s.accent } else { s.ink_dim },
            );
            let trailing: Element<'_, AppMessage> = if net.active {
                control(label("Disconnect", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(snd(C::Disconnect))
                    .into()
            } else if net.secure && !net.known {
                icon(Icon::Lock, 14.0, s.ink_faint).into()
            } else {
                iced::widget::Space::new().into()
            };
            let body = row![
                lead,
                {
                    let mut words =
                        column![label(net.ssid.clone(), text::BODY, s.ink)].spacing(tok::XXS);
                    if !state.is_empty() {
                        let ink = if failed.is_some() && busy.is_none() && !net.active {
                            s.bad
                        } else {
                            s.ink_dim
                        };
                        words = words.push(label(state.clone(), text::CAPTION, ink));
                    }
                    words.width(Length::Fill)
                },
                trailing,
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center);
            let mut c = control(body)
                .padding([tok::NONE, tok::XL])
                .height(if state.is_empty() { 46 } else { 54 })
                .width(Length::Fill)
                .kind(s, crate::ui::control::Kind::Ghost)
                .no_press_scale();
            if !net.active {
                c = c.on_press(msg(Message::JoinNet(net.clone())));
            }
            if net.active {
                current.push(Some(c.into()));
            } else {
                rows.push(Some(c.into()));
            }
            if net.active
                && let Some(d) = &n.details
            {
                let rows = &mut current;
                rows.extend(detail_rows(s, d));
                let band = match d.frequency {
                    f if f >= 5900 => "6 GHz",
                    f if f >= 4900 => "5 GHz",
                    f if f > 0 => "2.4 GHz",
                    _ => "",
                };
                if !band.is_empty() {
                    rows.push(copy_row(s, "Band", band.to_string()));
                }
                if d.speed > 0 {
                    rows.push(copy_row(s, "Speed", format!("{} Mb/s", d.speed)));
                }
                if !d.security.is_empty() {
                    rows.push(copy_row(s, "Security", d.security.clone()));
                }
                if !d.mac.is_empty() {
                    rows.push(copy_row(s, "Hardware address", d.mac.clone()));
                }
            }
            if let Some((ssid, _, typed)) = pw
                && *ssid == net.ssid
            {
                rows.push(row_of(
                    s,
                    "Password",
                    None,
                    field(
                        s,
                        "net-password",
                        "Password",
                        typed,
                        |t| snd(C::Password(t)),
                        Some(snd(C::SubmitPassword)),
                        true,
                    ),
                ));
            }
        }
        if n.networks.is_empty() {
            rows.push(Some(ui::waiting(s, Icon::Search, "Looking for networks…")));
        }
        // A network that does not announce its name.
        if st.hidden_open {
            let (name, pass) = &app.panels.control.hidden;
            rows.push(row_of(
                s,
                "Network name",
                None,
                field(
                    s,
                    "hidden-name",
                    "Name",
                    name,
                    |t| snd(C::HiddenName(t)),
                    None,
                    false,
                ),
            ));
            rows.push(row_of(
                s,
                "Password",
                None,
                field(
                    s,
                    "hidden-password",
                    "Password",
                    pass,
                    |t| snd(C::HiddenPassword(t)),
                    Some(AppMessage::Batch(vec![
                        snd(C::JoinHidden),
                        msg(Message::HiddenOpen(false)),
                    ])),
                    true,
                ),
            ));
        } else {
            rows.push(row_of(
                s,
                "Other network…",
                None,
                control(label("Join", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(msg(Message::HiddenOpen(true)))
                    .into(),
            ));
        }
        out.extend(group(s, Some("Wi-Fi"), current));
        out.extend(group(s, Some("Other networks"), rows));

        // Saved networks: join by themselves or not, forget. The switches' meaning is said
        // once, over them.
        let mut saved: Vec<Option<Element<'_, AppMessage>>> = vec![Some(
            container(
                row![
                    iced::widget::Space::new().width(Length::Fill),
                    label("Join automatically", text::CAPTION, s.ink_dim),
                    iced::widget::Space::new().width(74),
                ]
                .align_y(Alignment::Center),
            )
            .padding(iced::Padding {
                top: 10.0,
                right: 16.0,
                bottom: 2.0,
                left: 16.0,
            })
            .into(),
        )];
        let rows: Vec<Option<Element<'_, AppMessage>>> = n
            .saved
            .iter()
            .filter(|k| matches(&k.ssid) || matches("saved"))
            .map(|k| {
                let asking = st.forget_net.as_deref() == Some(k.ssid.as_str());
                Some(
                    container(
                        row![
                            label(k.ssid.clone(), text::BODY, s.ink).width(Length::Fill),
                            ui::switch(
                                s,
                                k.auto,
                                msg(Message::AutoJoinNet(k.ssid.clone(), !k.auto))
                            ),
                            control(label(
                                if asking { "Forget?" } else { "Forget" },
                                text::SMALL,
                                if asking { s.bad } else { s.ink },
                            ))
                            .pill(s, crate::ui::control::Pill::Plain)
                            .on_press(msg(Message::ForgetNet(k.ssid.clone()))),
                        ]
                        .spacing(tok::L)
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::XL])
                    .height(46)
                    .align_y(Alignment::Center)
                    .into(),
                )
            })
            .collect();
        if rows.is_empty() {
            saved.clear();
        }
        saved.extend(rows);
        out.extend(group(s, Some("Saved networks"), saved));
    }

    // Ethernet: the cable, the link, its addresses.
    if let Some(w) = &n.wired {
        let status = if w.connected {
            match w.speed {
                0 => "Connected".to_string(),
                sp if sp >= 1000 && sp % 1000 == 0 => format!("Connected · {} Gb/s", sp / 1000),
                sp => format!("Connected · {sp} Mb/s"),
            }
        } else if w.plugged {
            "Cable in, not connected".to_string()
        } else {
            "Cable unplugged".to_string()
        };
        let mut rows = vec![copy_row(s, "Ethernet", status)];
        if let Some(d) = &w.details {
            rows.extend(detail_rows(s, d));
        }
        if !w.mac.is_empty() {
            rows.push(copy_row(s, "Hardware address", w.mac.clone()));
        }
        out.extend(group(s, Some("Ethernet"), rows));
    }

    // IP settings of the connection in use.
    if let Some((_, name)) = &st.ip_for {
        let e = &st.ip_edit;
        // Manual starts from the address and router in use now (edited, not typed out).
        let now = n
            .wired
            .as_ref()
            .and_then(|w| w.details.as_ref())
            .filter(|_| n.link == crate::services::network::Link::Wired)
            .or(n.details.as_ref());
        let (now_ip, now_gw) = (
            now.and_then(|d| d.ipv4.clone()).unwrap_or_default(),
            now.and_then(|d| d.gateway.clone()).unwrap_or_default(),
        );
        let set = |f: &dyn Fn(&mut IpEdit)| {
            let mut x = e.clone();
            f(&mut x);
            msg(Message::IpEdit(x))
        };
        let mut rows = vec![
            copy_row(s, "Connection", name.clone()),
            row_of(
                s,
                "IP address",
                None,
                segments(
                    s,
                    &[
                        ("Automatic", !e.manual, set(&|x| x.manual = false)),
                        (
                            "Manual",
                            e.manual,
                            set(&|x| {
                                x.manual = true;
                                if x.address.is_empty() {
                                    x.address.clone_from(&now_ip);
                                }
                                if x.router.is_empty() {
                                    x.router.clone_from(&now_gw);
                                }
                            }),
                        ),
                    ],
                ),
            ),
        ];
        if e.manual {
            let base = e.clone();
            rows.push(row_of(
                s,
                "Address",
                None,
                field(
                    s,
                    "ip-address",
                    "192.168.1.50/24",
                    &e.address,
                    move |t| {
                        let mut x = base.clone();
                        x.address = t;
                        msg(Message::IpEdit(x))
                    },
                    Some(msg(Message::IpApply)),
                    false,
                ),
            ));
            let base = e.clone();
            rows.push(row_of(
                s,
                "Router",
                None,
                field(
                    s,
                    "ip-router",
                    "192.168.1.1",
                    &e.router,
                    move |t| {
                        let mut x = base.clone();
                        x.router = t;
                        msg(Message::IpEdit(x))
                    },
                    Some(msg(Message::IpApply)),
                    false,
                ),
            ));
        }
        rows.push(row_of(
            s,
            "DNS",
            None,
            segments(
                s,
                &[
                    ("Automatic", !e.custom_dns, set(&|x| x.custom_dns = false)),
                    ("Custom", e.custom_dns, set(&|x| x.custom_dns = true)),
                ],
            ),
        ));
        if e.custom_dns {
            let base = e.clone();
            rows.push(row_of(
                s,
                "Servers",
                None,
                field(
                    s,
                    "ip-dns",
                    "1.1.1.1, 9.9.9.9",
                    &e.dns,
                    move |t| {
                        let mut x = base.clone();
                        x.dns = t;
                        msg(Message::IpEdit(x))
                    },
                    Some(msg(Message::IpApply)),
                    false,
                ),
            ));
        }
        if *e != IpEdit::from(&st.ip_saved) {
            rows.push(Some(
                container(
                    row![
                        iced::widget::Space::new().width(Length::Fill),
                        control(label("Revert", text::SMALL, s.ink))
                            .pill(s, crate::ui::control::Pill::Plain)
                            .on_press(msg(Message::IpEdit(IpEdit::from(&st.ip_saved)))),
                        control(label("Apply", text::SMALL, s.on_accent))
                            .pill(s, crate::ui::control::Pill::Primary)
                            .on_press(msg(Message::IpApply)),
                    ]
                    .spacing(tok::M)
                    .align_y(Alignment::Center),
                )
                .padding([tok::M, tok::XL])
                .into(),
            ));
        }
        out.extend(group(s, Some("IP settings"), rows));
    }

    // Hotspot: shares Ethernet over Wi-Fi (the one Wi-Fi radio cannot join a network and host
    // one at once).
    if n.has_wifi {
        let wired_up = n.wired.as_ref().is_some_and(|w| w.connected);
        let mut rows = Vec::new();
        if n.hotspot {
            rows.push(row_of(
                s,
                "Hotspot",
                None,
                ui::switch(s, true, msg(Message::HotspotOn(false))),
            ));
            if let Some(h) = &st.hotspot {
                rows.push(copy_row(s, "Name", h.name.clone()));
                rows.push(copy_row(s, "Password", h.password.clone()));
            }
        } else if wired_up {
            rows.push(row_of(
                s,
                "Hotspot",
                None,
                ui::switch(s, false, msg(Message::HotspotOn(true))),
            ));
            let (name, pass) = st.hotspot_edit.clone();
            let p2 = pass.clone();
            rows.push(row_of(
                s,
                "Name",
                None,
                field(
                    s,
                    "hotspot-name",
                    "Name",
                    &name,
                    move |t| msg(Message::HotspotEdit(t, p2.clone())),
                    None,
                    false,
                ),
            ));
            rows.push(row_of(
                s,
                "Password",
                None,
                field(
                    s,
                    "hotspot-password",
                    "8 characters or more",
                    &pass,
                    move |t| msg(Message::HotspotEdit(name.clone(), t)),
                    Some(msg(Message::HotspotOn(true))),
                    false,
                ),
            ));
        } else {
            rows.push(row_of(
                s,
                "Hotspot",
                Some("Shares Ethernet over Wi-Fi: plug in a cable first"),
                iced::widget::Space::new().into(),
            ));
        }
        out.extend(group(s, Some("Hotspot"), rows));
    }

    // VPNs kestrel knows (installed ones only).
    let mut vpn = Vec::new();
    if let Some(on) = app.vpn.warp {
        vpn.push(row_of(
            s,
            "Cloudflare WARP",
            None,
            ui::switch(s, on, snd(C::Warp(!on))),
        ));
    }
    if let Some(on) = app.vpn.tailscale {
        vpn.push(row_of(
            s,
            "Tailscale",
            None,
            ui::switch(s, on, snd(C::Tailscale(!on))),
        ));
    }
    out.extend(group(s, Some("VPN"), vpn));
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
