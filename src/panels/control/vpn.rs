//! The control centre's vpn page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A section title inside a page.
pub(super) fn heading<'a>(s: Scheme, title: &'a str) -> Element<'a, Message> {
    container(ui::section(s, title, tok::S))
        .padding([tok::S, tok::NONE])
        .into()
}

/// A plain "name — value" line.
pub(super) fn fact<'a>(
    s: Scheme,
    name: impl Into<String>,
    value: impl Into<String>,
) -> Element<'a, Message> {
    row![
        label(name.into(), text::SMALL, s.ink_dim).width(Length::Fill),
        label(value.into(), text::SMALL, s.ink),
    ]
    .padding([tok::XS, tok::S])
    .into()
}

/// A setting with a switch.
pub(super) fn setting<'a>(s: Scheme, name: &'a str, on: bool, m: Message) -> Element<'a, Message> {
    row![
        label(name, text::BODY, s.ink).width(Length::Fill),
        switch(s, on, m)
    ]
    .padding([tok::S, tok::S])
    .align_y(Alignment::Center)
    .into()
}

/// A big status line: icon, state, and what it means.
pub(super) fn summary<'a>(
    s: Scheme,
    i: Icon,
    on: bool,
    title: String,
    sub: String,
) -> Element<'a, Message> {
    row![
        container(icon(
            i,
            tok::ICON_XL,
            if on { s.on_accent } else { s.ink_dim }
        ))
        .center(44)
        .style(move |_| container::Style {
            background: Some(
                if on {
                    s.accent
                } else {
                    s.ink.scale_alpha(tok::FILL * s.opacity)
                }
                .into()
            ),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        }),
        column![
            label_w(title, text::BODY, s.ink, theme::SEMIBOLD),
            label(sub, text::CAPTION, s.ink_dim),
        ]
        .spacing(tok::XXS),
    ]
    .spacing(tok::L)
    .padding([tok::XS, tok::XS])
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn warp_page<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let on = cx.vpn.warp.unwrap_or(false);
    let header = subpage_header(
        s,
        "Cloudflare WARP",
        Message::Page(Page::Main),
        switch(s, on, Message::Warp(!on)),
    );
    let w = st.warp.clone().filter(|_| on);
    let Some(w) = w else {
        return column![
            header,
            summary(
                s,
                Icon::Cloud,
                false,
                if on {
                    "Starting…".into()
                } else {
                    "Off".into()
                },
                "Your connection is direct".into()
            )
        ]
        .spacing(tok::L)
        .into();
    };
    let connected = w.status.starts_with("Connected");
    let mut sub = if connected {
        "Traffic goes through Cloudflare".to_string()
    } else {
        "Your connection is direct".to_string()
    };
    if !w.reason.is_empty() && !connected {
        sub = w.reason.clone();
    }
    let mut col = column![
        header,
        summary(s, Icon::Cloud, connected, w.status.clone(), sub)
    ]
    .spacing(tok::S);
    // Modes: what WARP carries.
    col = col.push(heading(s, "Mode"));
    let mut modes = column![].spacing(tok::XXS);
    for (m, name, note) in [
        (
            "warp+doh",
            "WARP and DNS",
            "Everything through Cloudflare, DNS encrypted",
        ),
        ("warp", "WARP", "Everything through Cloudflare"),
        (
            "tunnel_only",
            "Tunnel only",
            "Through Cloudflare, your own DNS",
        ),
        ("doh", "DNS only", "Only DNS, encrypted (1.1.1.1)"),
    ] {
        let active = w.mode == m;
        modes = modes.push(list_row(
            s,
            icon(
                if active { Icon::CircleDot } else { Icon::Globe },
                tok::ICON,
                if active { s.accent } else { s.ink_dim },
            )
            .into(),
            name.to_string(),
            Some(note.to_string()),
            if active {
                icon(Icon::Check, tok::ICON, s.accent).into()
            } else {
                Space::new().into()
            },
            active,
            Some(Message::WarpMode(m.to_string())),
        ));
    }
    col = col.push(modes);
    let mut facts = column![].spacing(tok::NONE);
    if !w.account.is_empty() {
        facts = facts.push(fact(s, "Account", w.account.clone()));
    }
    for (k, v) in w.stats.iter().take(5) {
        facts = facts.push(fact(s, k.clone(), ui::fit(v, text::SMALL, 200.0)));
    }
    if !w.account.is_empty() || !w.stats.is_empty() {
        col = col.push(heading(s, "Details")).push(facts);
    }
    col.into()
}

pub(super) fn tailscale_page<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let on = cx.vpn.tailscale.unwrap_or(false);
    let header = subpage_header(
        s,
        "Tailscale",
        Message::Page(Page::Main),
        switch(s, on, Message::Tailscale(!on)),
    );
    let Some(net) = st.tailnet.as_ref().filter(|_| on) else {
        return column![
            header,
            summary(
                s,
                Icon::Network,
                false,
                if on {
                    "Starting…".into()
                } else {
                    "Off".into()
                },
                "Not on your tailnet".into()
            )
        ]
        .spacing(tok::L)
        .into();
    };
    let ts = |args: &[&str]| Message::Tailscale2(args.iter().map(|a| a.to_string()).collect());
    let running = net.state == "Running";
    let mut col = column![header].spacing(tok::S);
    if net.state == "NeedsLogin" {
        col = col.push(summary(
            s,
            Icon::Lock,
            false,
            "Sign in needed".into(),
            "Opens the Tailscale sign-in page".into(),
        ));
        col = col.push(
            ui::control(label_w(
                "Sign in",
                text::SMALL,
                s.on_accent,
                theme::SEMIBOLD,
            ))
            .pill(s, crate::ui::control::Pill::Primary)
            .on_press(Message::Login),
        );
        return col.into();
    }
    let sub = if net.tailnet.is_empty() {
        net.state.clone()
    } else {
        format!(
            "{} · {}",
            if running {
                "Connected"
            } else {
                net.state.as_str()
            },
            net.tailnet
        )
    };
    col = col.push(summary(s, Icon::Network, running, net.name.clone(), sub));
    if let Some(e) = &st.ts_error {
        col = col
            .push(container(label(e.clone(), text::CAPTION, s.warn)).padding([tok::XXS, tok::S]));
    }
    // This device.
    let mut me = column![].spacing(tok::NONE);
    if !net.ip.is_empty() {
        me = me.push(
            ui::control(row![
                label("IPv4", text::SMALL, s.ink_dim).width(Length::Fill),
                label(net.ip.clone(), text::SMALL, s.ink),
            ])
            .padding([tok::XS, tok::S])
            .radius(tok::R_CONTROL())
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(Message::CopyIp(net.ip.clone())),
        );
    }
    if !net.ip6.is_empty() {
        me = me.push(fact(s, "IPv6", ui::fit(&net.ip6, text::SMALL, 220.0)));
    }
    col = col.push(me);
    col = col
        .push(heading(s, "Settings"))
        .push(setting(
            s,
            "MagicDNS",
            net.magic_dns,
            ts(&["set", &format!("--accept-dns={}", !net.magic_dns)]),
        ))
        .push(setting(
            s,
            "Tailscale SSH",
            net.ssh,
            ts(&["set", &format!("--ssh={}", !net.ssh)]),
        ))
        .push(setting(
            s,
            "Shields up",
            net.shields_up,
            ts(&["set", &format!("--shields-up={}", !net.shields_up)]),
        ));
    // Exit node: where internet traffic leaves from.
    let exits: Vec<&crate::services::vpn::Peer> =
        net.peers.iter().filter(|p| p.exit_option).collect();
    if !exits.is_empty() {
        col = col.push(heading(s, "Exit node"));
        let none = !exits.iter().any(|p| p.exit_node);
        let mut list = column![list_row(
            s,
            icon(Icon::Laptop, tok::ICON, if none { s.accent } else { s.ink }).into(),
            "None".to_string(),
            Some("Internet goes out from here".into()),
            if none {
                icon(Icon::Check, tok::ICON, s.accent).into()
            } else {
                Space::new().into()
            },
            none,
            Some(ts(&["set", "--exit-node="])),
        )]
        .spacing(tok::XXS);
        for p in exits {
            list = list.push(list_row(
                s,
                icon(
                    Icon::Server,
                    tok::ICON,
                    if p.exit_node { s.accent } else { s.ink },
                )
                .into(),
                p.name.clone(),
                Some(if p.online { "Online" } else { "Offline" }.to_string()),
                if p.exit_node {
                    icon(Icon::Check, tok::ICON, s.accent).into()
                } else {
                    Space::new().into()
                },
                p.exit_node,
                p.online
                    .then(|| ts(&["set", &format!("--exit-node={}", p.ip)])),
            ));
        }
        col = col.push(list);
    }
    // Devices.
    col = col.push(heading(s, "Devices"));
    let mut list = column![].spacing(tok::XXS);
    for p in net.peers.iter().take(8) {
        let i = match p.os.as_str() {
            "iOS" | "android" => Icon::Smartphone,
            "linux" | "windows" | "macOS" => Icon::Laptop,
            _ => Icon::Server,
        };
        let mut sub = if p.online {
            "Online".to_string()
        } else if p.last_seen.is_empty() {
            "Offline".to_string()
        } else {
            format!("Last seen {}", p.last_seen)
        };
        sub.push_str(&format!(" · {}", p.ip));
        let trailing: Element<'_, Message> = match st.pings.get(&p.ip) {
            Some(r) => label(r.clone(), text::CAPTION, s.ink_dim).into(),
            None if p.online => ui::control(label("Ping", text::CAPTION, s.ink_dim))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(Message::Ping(p.ip.clone()))
                .into(),
            None => Space::new().into(),
        };
        list = list.push(list_row(
            s,
            icon(i, tok::ICON, if p.online { s.ink } else { s.ink_faint }).into(),
            p.name.clone(),
            Some(sub),
            trailing,
            false,
            Some(Message::CopyIp(p.ip.clone())),
        ));
    }
    col = col.push(list);
    col = col.push(
        row![
            Space::new().width(Length::Fill),
            ui::control(label("Log out", text::SMALL, s.ink_dim))
                .pill(s, crate::ui::control::Pill::Quiet)
                .on_press(ts(&["logout"])),
        ]
        .padding([tok::XS, tok::NONE]),
    );
    ui::smooth(
        iced::widget::scrollable(col)
            .height(Length::Shrink)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(0)
                    .scroller_width(0),
            )),
    )
    .into()
}
