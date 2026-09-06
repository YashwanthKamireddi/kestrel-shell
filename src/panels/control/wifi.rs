//! The control centre's wifi page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn wifi_page<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let n = &cx.net;
    let header = subpage_header(
        s,
        "Wi-Fi",
        Message::Page(Page::Main),
        switch(s, n.wifi_enabled, Message::Wifi(!n.wifi_enabled)),
    );
    let mut list = column![].spacing(tok::XXS);
    if !n.wifi_enabled {
        list = list.push(empty(s, Icon::WifiOff));
    } else if n.networks.is_empty() {
        list = list.push(ui::waiting(s, Icon::Search, "Looking for networks…"));
    }
    let pw = &st.password;
    // In the order they had when the page opened (`keep_order`).
    let order = &st.wifi_order;
    let mut shown: Vec<&network::Network> = n.networks.iter().collect();
    shown.sort_by_key(|net| {
        order
            .iter()
            .position(|s| *s == net.ssid)
            .unwrap_or(usize::MAX)
    });
    for net in shown.into_iter().filter(|_| n.wifi_enabled).take(12) {
        let busy = cx.in_progress(&net.ssid);
        let sub = if let Some(want) = busy {
            Some(
                if want {
                    "Connecting…"
                } else {
                    "Disconnecting…"
                }
                .to_string(),
            )
        } else if net.active {
            Some("Connected".to_string())
        } else if let Some(e) = cx.net_error(&net.ssid) {
            Some(e.to_string())
        } else if net.known {
            Some("Saved".to_string())
        } else {
            None
        };
        let trailing: Element<'_, Message> = if net.active || net.known {
            info_button(s, Message::NetworkInfo(net.ssid.clone()))
        } else if net.secure {
            icon(Icon::Lock, tok::ICON_S, s.ink_faint).into()
        } else {
            Space::new().into()
        };
        list = list.push(list_row(
            s,
            ui::breathe::icon_or(
                strength_icon(net.strength),
                tok::ICON,
                if busy.is_some() || net.active {
                    s.accent
                } else {
                    s.ink
                },
                busy.is_some(),
            ),
            net.ssid.clone(),
            sub,
            trailing,
            net.active,
            Some(Message::Join(net.clone())),
        ));
        if let Some((ssid, _, text_value)) = pw
            && *ssid == net.ssid
        {
            list = list.push(
                container(
                    container(
                        text_input("Password", text_value)
                            .id(PASSWORD_ID.clone())
                            .secure(true)
                            .on_input(Message::Password)
                            .on_submit(Message::SubmitPassword)
                            .padding([tok::M, tok::L])
                            .size(text::BODY)
                            .font(theme::FONT)
                            .style(crate::ui::input_style(s)),
                    )
                    // The same quiet capsule as every other field (no accent outline on focus).
                    .style(move |_| ui::field_style(s)),
                )
                .padding([tok::XS, tok::NONE]),
            );
        }
    }
    if n.wifi_enabled {
        // Saved networks out of range, so they can be looked at and forgotten too.
        let away: Vec<&network::Saved> = n
            .saved
            .iter()
            .filter(|k| !n.networks.iter().any(|w| w.ssid == k.ssid))
            .collect();
        if !away.is_empty() {
            list = list.push(section(s, "Saved"));
            for k in away {
                list = list.push(list_row(
                    s,
                    icon(Icon::Wifi, tok::ICON, s.ink_faint).into(),
                    k.ssid.clone(),
                    None,
                    info_button(s, Message::NetworkInfo(k.ssid.clone())),
                    false,
                    Some(Message::NetworkInfo(k.ssid.clone())),
                ));
            }
        }
        list = list.push(list_row(
            s,
            icon(Icon::Plus, tok::ICON, s.ink_dim).into(),
            "Other network…",
            None,
            Space::new().into(),
            false,
            Some(Message::Page(Page::Hidden)),
        ));
    }
    column![header, list].spacing(tok::M).into()
}

/// A small heading over a part of a list.
pub(super) fn section<'a>(s: Scheme, t: &'a str) -> Element<'a, Message> {
    // Room above it, a little below: it belongs to the rows under it.
    container(ui::section(s, t, tok::L))
        .padding(iced::Padding {
            top: tok::L,
            bottom: tok::XS,
            ..iced::Padding::ZERO
        })
        .into()
}

/// The round "i" at the end of a row: opens its details.
pub(super) fn info_button<'a>(s: Scheme, m: Message) -> Element<'a, Message> {
    crate::ui::control(icon(Icon::Info, tok::ICON_M, s.ink_dim))
        .padding(tok::S)
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(m)
        .into()
}

/// One line of details: what it is, and its value (selectable by copying: a click copies it).
pub(super) fn detail_row<'a>(s: Scheme, what: &'a str, value: String) -> Element<'a, Message> {
    crate::ui::control(
        row![
            label(what, text::SMALL, s.ink_dim).width(Length::Fill),
            label(value.clone(), text::SMALL, s.ink),
        ]
        .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::L])
    .height(32)
    .width(Length::Fill)
    .radius(tok::R_ROW)
    .kind(s, crate::ui::control::Kind::Ghost)
    .no_press_scale()
    .on_press(Message::CopyIp(value))
    .into()
}

/// A Wi-Fi network's page: its connection in detail (when it is the one in use; a click on a
/// value copies it), joining by itself, disconnect, and forget (asked once).
pub(super) fn network_page<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let n = &cx.net;
    let ssid = st.network.clone().unwrap_or_default();
    let header = subpage_header(
        s,
        ssid.clone(),
        Message::Page(Page::Wifi),
        Space::new().into(),
    );
    let mut body = column![].spacing(tok::XXS);
    let active = n.ssid.as_deref() == Some(ssid.as_str());
    if active && let Some(d) = &n.details {
        if let Some(ip) = &d.ipv4 {
            body = body.push(detail_row(s, "IP address", ip.clone()));
        }
        if let Some(ip) = &d.ipv6 {
            body = body.push(detail_row(s, "IPv6 address", ip.clone()));
        }
        if let Some(g) = &d.gateway {
            body = body.push(detail_row(s, "Router", g.clone()));
        }
        if !d.dns.is_empty() {
            body = body.push(detail_row(s, "DNS", d.dns.join(", ")));
        }
        if d.frequency > 0 {
            let band = if d.frequency >= 5900 {
                "6 GHz"
            } else if d.frequency >= 4900 {
                "5 GHz"
            } else {
                "2.4 GHz"
            };
            body = body.push(detail_row(s, "Band", band.to_string()));
        }
        if d.speed > 0 {
            body = body.push(detail_row(s, "Speed", format!("{} Mbit/s", d.speed)));
        }
        body = body.push(detail_row(s, "Security", d.security.clone()));
        if !d.mac.is_empty() {
            body = body.push(detail_row(s, "Hardware address", d.mac.clone()));
        }
    }
    let saved = n.saved.iter().find(|k| k.ssid == ssid);
    if let Some(k) = saved {
        body = body.push(
            container(
                row![
                    label("Join automatically", text::BODY, s.ink).width(Length::Fill),
                    switch(s, k.auto, Message::AutoJoin(!k.auto)),
                ]
                .align_y(Alignment::Center),
            )
            .padding([tok::L, tok::L]),
        );
    }
    let mut buttons = row![Space::new().width(Length::Fill)].spacing(tok::M);
    if active {
        buttons = buttons.push(
            crate::ui::control(label_w("Disconnect", text::SMALL, s.ink, theme::MEDIUM))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(Message::Disconnect),
        );
    }
    if saved.is_some() {
        buttons = buttons.push(if st.forget_net {
            crate::ui::control(label_w("Forget?", text::SMALL, s.base, theme::MEDIUM))
                .padding([tok::S, tok::L])
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Danger)
                .on_press(Message::ForgetNetwork)
        } else {
            crate::ui::control(label_w("Forget", text::SMALL, s.ink, theme::MEDIUM))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(Message::ForgetNetwork)
        });
    }
    column![header, body, container(buttons).padding([tok::M, tok::XS])]
        .spacing(tok::M)
        .into()
}

/// Joining a network that does not announce its name: its name, its password (if any), Join.
pub(super) fn hidden_page<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let (name, pw) = &st.hidden;
    let header = subpage_header(
        s,
        "Other network",
        Message::Page(Page::Wifi),
        Space::new().into(),
    );
    let field = |id: &'static str,
                 placeholder: &'static str,
                 value: &str,
                 secure: bool,
                 on: fn(String) -> Message| {
        container(
            text_input(placeholder, value)
                .id(iced::widget::Id::new(id))
                .secure(secure)
                .on_input(on)
                .on_submit(Message::JoinHidden)
                .padding([tok::M, tok::L])
                .size(text::BODY)
                .font(theme::FONT)
                .style(crate::ui::input_style(s)),
        )
        .style(move |_| ui::field_style(s))
    };
    let join = crate::ui::control(label_w("Join", text::SMALL, s.on_accent, theme::SEMIBOLD))
        .pill(s, crate::ui::control::Pill::Primary)
        .on_press_maybe((!name.trim().is_empty()).then_some(Message::JoinHidden));
    column![
        header,
        field(
            "hidden-name",
            "Network name",
            name,
            false,
            Message::HiddenName
        ),
        field(
            "hidden-password",
            "Password",
            pw,
            true,
            Message::HiddenPassword
        ),
        container(row![Space::new().width(Length::Fill), join]).padding([tok::M, tok::XS]),
    ]
    .spacing(tok::M)
    .into()
}
