//! The password prompt for administrator rights (kestrel's own polkit agent): what is asking,
//! whose password, a password field, Cancel and Authenticate. A wrong password shakes nothing
//! and asks again with one quiet line. Opened by the agent; the keyboard goes straight to it.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::polkit;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{column, container, row, text_input};
use iced::{Alignment, Color, Element, Length, Task};

pub const WIDTH: f32 = 400.0;

#[derive(Default)]
pub struct State {
    pub request: Option<polkit::Request>,
    password: String,
    /// The password was sent; waiting for polkit's answer.
    checking: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Password(String),
    Submit,
    Cancel,
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Auth(m))
}

static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
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
    app.panels.auth.password.clear();
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.auth;
    let Some(r) = &st.request else {
        return iced::widget::Space::new().into();
    };
    let head = row![
        container(icon(Icon::Lock, 20.0, s.ink))
            .center(40)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(0.1 * s.opacity).into()),
                border: iced::Border::default().rounded(20),
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
        .spacing(2),
    ]
    .spacing(12)
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
            .padding([10, 14])
            .size(text::BODY)
            .font(theme::FONT)
            .style(move |_, _| text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                icon: s.ink_dim,
                placeholder: s.ink_faint,
                value: s.ink,
                selection: s.accent_soft,
            }),
    )
    .style(move |_| ui::field_style(s));
    let mut body = column![head, message, field].spacing(14);
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
    .spacing(8)
    .align_y(Alignment::Center);
    container(body.push(buttons))
        .padding(20)
        .width(Length::Fill)
        .into()
}
