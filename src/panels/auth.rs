//! The system's questions, one panel: the password prompt for administrator rights (kestrel's
//! own polkit agent: what is asking, whose password, a password field, Cancel and
//! Authenticate; a wrong password asks again with one quiet line), and Bluetooth pairing
//! (kestrel's own BlueZ agent: confirm the code, type a PIN, or type a code on a keyboard).
//! Opened by the agents; the keyboard goes straight to it.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{bluetooth, network, polkit};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{column, container, row, text_input};
use iced::{Alignment, Element, Length, Task};

pub const WIDTH: f32 = crate::theme::size::PANEL_M;

#[derive(Default)]
pub struct State {
    pub request: Option<polkit::Request>,
    password: String,
    /// The password was sent; waiting for polkit's answer.
    checking: bool,
    /// A Bluetooth pairing question (when there is no password request).
    pub bt: Option<bluetooth::Ask>,
    /// A Wi-Fi password NetworkManager needs (when nothing else is asked).
    pub wifi: Option<network::Ask>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Password(String),
    Submit,
    Cancel,
}

/// A pairing question arrived: shown (a PIN with the field focused).
pub fn ask_bt(app: &mut App, a: bluetooth::Ask) -> Task<AppMessage> {
    let st = &mut app.panels.auth;
    st.password.clear();
    let focus = matches!(a, bluetooth::Ask::Pin { .. });
    st.bt = Some(a);
    if focus {
        Task::Iced(iced::widget::focus(INPUT_ID.clone()))
    } else {
        Task::none()
    }
}

/// A Wi-Fi password question arrived: shown with the field focused.
pub fn ask_wifi(app: &mut App, a: network::Ask) -> Task<AppMessage> {
    let st = &mut app.panels.auth;
    st.password.clear();
    st.wifi = Some(a);
    Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

/// The Wi-Fi question's id, if one is shown.
pub fn wifi_id(st: &State) -> Option<u64> {
    match st.wifi.as_ref()? {
        network::Ask::Password { id, .. } => Some(*id),
        network::Ask::Done(_) => None,
    }
}

/// The pairing question's id, if one is shown.
pub fn bt_id(st: &State) -> Option<u64> {
    match st.bt.as_ref()? {
        bluetooth::Ask::Confirm { id, .. }
        | bluetooth::Ask::Pin { id, .. }
        | bluetooth::Ask::Show { id, .. } => Some(*id),
        bluetooth::Ask::Done(_) => None,
    }
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Auth(m))
}

pub(super) static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("auth-password"));

/// A request (or a retry) arrived: show it with an empty field focused.
pub fn ask(app: &mut App, r: polkit::Request) -> Task<AppMessage> {
    let st = &mut app.panels.auth;
    st.password.clear();
    st.checking = false;
    st.request = Some(r);
    Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.auth;
    if st.request.is_none()
        && st.bt.is_none()
        && let Some(id) = wifi_id(st)
    {
        match m {
            Message::Password(p) => st.password = p,
            // WPA passwords are 8 to 63 characters: shorter is never sent.
            Message::Submit if st.password.chars().count() >= 8 => {
                network::answer(id, Some(std::mem::take(&mut st.password)));
                st.wifi = None;
                return Task::done(AppMessage::Close);
            }
            Message::Submit => {}
            Message::Cancel => {
                network::answer(id, None);
                st.wifi = None;
                return Task::done(AppMessage::Close);
            }
        }
        return Task::none();
    }
    if st.request.is_none()
        && let Some(a) = st.bt.clone()
    {
        match (m, a) {
            (Message::Password(p), _) => st.password = p,
            (Message::Submit, bluetooth::Ask::Confirm { id, .. }) => {
                bluetooth::answer(id, true);
                st.bt = None;
                return Task::done(AppMessage::Close);
            }
            (Message::Submit, bluetooth::Ask::Pin { id, .. }) if !st.password.is_empty() => {
                bluetooth::answer_pin(id, std::mem::take(&mut st.password));
                st.bt = None;
                return Task::done(AppMessage::Close);
            }
            (Message::Cancel, a) => {
                bluetooth::answer(a.id(), false);
                st.bt = None;
                return Task::done(AppMessage::Close);
            }
            _ => {}
        }
        return Task::none();
    }
    match m {
        Message::Password(p) => st.password = p,
        Message::Submit => {
            if let Some(r) = &st.request
                && !st.password.is_empty()
                && !st.checking
            {
                st.checking = true;
                polkit::respond(&r.cookie, std::mem::take(&mut st.password));
            }
        }
        Message::Cancel => {
            if let Some(r) = st.request.take() {
                polkit::cancel(&r.cookie);
            }
            return Task::done(AppMessage::Close);
        }
    }
    Task::none()
}

/// The panel closed without an answer: the app hears "cancelled".
pub fn closed(app: &mut App) {
    if let Some(r) = app.panels.auth.request.take() {
        polkit::cancel(&r.cookie);
    }
    // A pairing question closed without an answer: declined.
    if let Some(id) = bt_id(&app.panels.auth) {
        bluetooth::answer(id, false);
    }
    app.panels.auth.bt = None;
    // A Wi-Fi password closed without one: cancelled.
    if let Some(id) = wifi_id(&app.panels.auth) {
        network::answer(id, None);
    }
    app.panels.auth.wifi = None;
    app.panels.auth.password.clear();
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.auth;
    let Some(r) = &st.request else {
        return match (&st.bt, &st.wifi) {
            (Some(a), _) => bt_view(st, a, s),
            (None, Some(network::Ask::Password { ssid, wrong, .. })) => {
                wifi_view(st, ssid, *wrong, s)
            }
            _ => iced::widget::Space::new().into(),
        };
    };
    let head = row![
        container(icon(Icon::Lock, 20.0, s.ink))
            .center(40)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            }),
        column![
            label_w(
                "Authentication required",
                text::TITLE,
                s.ink,
                theme::SEMIBOLD
            ),
            label(format!("Password for {}", r.user), text::SMALL, s.ink_dim),
        ]
        .spacing(tok::XXS),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);
    let message = iced::widget::text(r.message.clone())
        .size(text::BODY)
        .color(s.ink)
        .font(theme::FONT)
        .line_height(iced::widget::text::LineHeight::Relative(1.35))
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph);
    let field = container(
        text_input("Password", &st.password)
            .id(INPUT_ID.clone())
            .secure(true)
            .on_input(|p| msg(Message::Password(p)))
            .on_submit(msg(Message::Submit))
            .padding([tok::L, tok::L])
            .size(text::BODY)
            .font(theme::FONT)
            .style(crate::ui::input_style(s)),
    )
    .style(move |_| ui::field_style(s));
    let mut body = column![head, message, field].spacing(tok::L);
    if r.retry && !st.checking {
        body = body.push(label("That password was not right", text::SMALL, s.bad));
    }
    let buttons = row![
        iced::widget::Space::new().width(Length::Fill),
        control(label_w("Cancel", text::SMALL, s.ink, theme::MEDIUM))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::Cancel)),
        control(label_w(
            if st.checking {
                "Checking…"
            } else {
                "Authenticate"
            },
            text::SMALL,
            s.on_accent,
            theme::SEMIBOLD
        ))
        .pill(s, crate::ui::control::Pill::Primary)
        .on_press_maybe((!st.password.is_empty() && !st.checking).then(|| msg(Message::Submit))),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}

/// A pairing question: the device, the code (large, to compare or type), and the answer.
fn bt_view<'a>(st: &'a State, a: &'a bluetooth::Ask, s: Scheme) -> Element<'a, AppMessage> {
    let (title, code, pin, primary): (String, Option<&str>, bool, Option<&str>) = match a {
        bluetooth::Ask::Confirm { device, code, .. } => (
            format!("Pair with {device}?"),
            code.as_deref(),
            false,
            Some("Pair"),
        ),
        bluetooth::Ask::Pin { device, .. } => {
            (format!("PIN for {device}"), None, true, Some("Pair"))
        }
        bluetooth::Ask::Show { device, code, .. } => (
            format!("Type this on {device}"),
            Some(code.as_str()),
            false,
            None,
        ),
        bluetooth::Ask::Done(_) => return iced::widget::Space::new().into(),
    };
    let head = row![
        container(icon(Icon::Bluetooth, 20.0, s.ink))
            .center(40)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            }),
        label_w(title, text::TITLE, s.ink, theme::SEMIBOLD),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);
    let mut body = column![head].spacing(tok::L);
    if let Some(c) = code {
        // The code as it reads on the device: digits spaced, large.
        body = body.push(
            container(label_w(
                c.chars()
                    .map(|ch| format!("{ch} "))
                    .collect::<String>()
                    .trim_end()
                    .to_string(),
                28.0,
                s.ink,
                theme::SEMIBOLD,
            ))
            .center_x(Length::Fill)
            .padding([tok::S, tok::NONE]),
        );
    }
    if pin {
        body = body.push(
            container(
                text_input("PIN", &st.password)
                    .id(INPUT_ID.clone())
                    .on_input(|p| msg(Message::Password(p)))
                    .on_submit(msg(Message::Submit))
                    .padding([tok::L, tok::L])
                    .size(text::BODY)
                    .font(theme::FONT)
                    .style(crate::ui::input_style(s)),
            )
            .style(move |_| ui::field_style(s)),
        );
    }
    let mut buttons = row![
        iced::widget::Space::new().width(Length::Fill),
        control(label_w(
            if primary.is_some() { "Cancel" } else { "Done" },
            text::SMALL,
            s.ink,
            theme::MEDIUM
        ))
        .pill(s, crate::ui::control::Pill::Plain)
        .on_press(msg(Message::Cancel)),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    if let Some(p) = primary {
        buttons = buttons.push(
            control(label_w(p, text::SMALL, s.on_accent, theme::SEMIBOLD))
                .pill(s, crate::ui::control::Pill::Primary)
                .on_press_maybe((!pin || !st.password.is_empty()).then(|| msg(Message::Submit))),
        );
    }
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}

/// A Wi-Fi network's password: the network, the field, and a quiet line when the saved one no
/// longer works.
fn wifi_view<'a>(st: &'a State, ssid: &'a str, wrong: bool, s: Scheme) -> Element<'a, AppMessage> {
    let head = row![
        container(icon(Icon::Wifi, 20.0, s.ink))
            .center(40)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            }),
        column![
            label_w(ssid, text::TITLE, s.ink, theme::SEMIBOLD),
            label("Wi-Fi password", text::SMALL, s.ink_dim),
        ]
        .spacing(tok::XXS),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);
    let field = container(
        text_input("Password", &st.password)
            .id(INPUT_ID.clone())
            .secure(true)
            .on_input(|p| msg(Message::Password(p)))
            .on_submit(msg(Message::Submit))
            .padding([tok::L, tok::L])
            .size(text::BODY)
            .font(theme::FONT)
            .style(crate::ui::input_style(s)),
    )
    .style(move |_| ui::field_style(s));
    let mut body = column![head, field].spacing(tok::L);
    if wrong {
        body = body.push(label("The saved password did not work", text::SMALL, s.bad));
    }
    let buttons = row![
        iced::widget::Space::new().width(Length::Fill),
        control(label_w("Cancel", text::SMALL, s.ink, theme::MEDIUM))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::Cancel)),
        control(label_w("Join", text::SMALL, s.on_accent, theme::SEMIBOLD))
            .pill(s, crate::ui::control::Pill::Primary)
            .on_press_maybe((st.password.chars().count() >= 8).then(|| msg(Message::Submit))),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}
