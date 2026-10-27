//! The system's questions, one panel: the password prompt for administrator rights (kestrel's
//! own polkit agent: what is asking, whose password, a password field, Cancel and
//! Authenticate; a wrong password asks again with one quiet line), and Bluetooth pairing
//! (kestrel's own BlueZ agent: confirm the code, type a PIN, or type a code on a keyboard),
//! and the keyring's (kestrel is GNOME Keyring's system prompter: unlock a keyring, choose a
//! new one's password, or confirm).
//! Opened by the agents; the keyboard goes straight to it.
//!
//! A self-contained component (its own state, messages and view, nothing of the shell's):
//! the prompt process (prompt.rs) shows it in a window of its own, so a shell that crashes or
//! freezes never takes a password, Wi-Fi or pairing question down with it.

use crate::services::{bluetooth, keyring, network, polkit};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{column, container, row, text_input};
use iced::{Alignment, Element, Length};

pub const WIDTH: f32 = crate::theme::tok::PANEL_M;

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
    /// A keyring's question (when nothing else is asked).
    pub keyring: Option<keyring::Prompt>,
    /// A new password typed again, to be sure of it.
    again: String,
    /// The keyring question's check box ("Automatically unlock…").
    choice: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Password(String),
    Submit,
    Cancel,
    /// A new password typed again.
    Again(String),
    /// The keyring question's check box.
    Choice(bool),
}

/// What an update asks of the window holding the prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Keep showing.
    Stay,
    /// The question is over: close the window.
    Close,
}

pub(crate) static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("auth-password"));

/// Focuses the field (after a question arrives).
pub fn focus<M: Send + 'static>() -> iced::Task<M> {
    iced::Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

impl State {
    /// Nothing is being asked.
    pub fn is_empty(&self) -> bool {
        self.request.is_none()
            && self.bt.is_none()
            && wifi_id(self).is_none()
            && self.keyring.is_none()
    }

    /// A keyring's question arrived (or asks again, with why). False when something else is
    /// being asked: it is declined, and the app asks again when it needs the keyring.
    pub fn ask_keyring(&mut self, p: keyring::Prompt) -> bool {
        let same = self.keyring.as_ref().is_some_and(|k| k.id == p.id);
        if !same && (self.request.is_some() || self.bt.is_some() || wifi_id(self).is_some()) {
            keyring::answer(p.id, None);
            return false;
        }
        self.password.clear();
        self.again.clear();
        self.checking = false;
        self.choice = p.choice.as_ref().is_some_and(|c| c.1);
        self.keyring = Some(p);
        true
    }

    pub fn ended_keyring(&mut self, id: u64) -> Outcome {
        if self.keyring.as_ref().is_some_and(|k| k.id == id) {
            self.keyring = None;
            self.password.clear();
            self.again.clear();
            self.checking = false;
        }
        if self.is_empty() {
            Outcome::Close
        } else {
            Outcome::Stay
        }
    }

    /// A password request (or a retry) arrived: shown with an empty field.
    pub fn ask(&mut self, r: polkit::Request) {
        self.password.clear();
        self.checking = false;
        self.request = Some(r);
    }

    /// A pairing question arrived. False when an administrator password is being asked: the
    /// pairing is declined (it can be tried again).
    pub fn ask_bt(&mut self, a: bluetooth::Ask) -> bool {
        if self.request.is_some() {
            bluetooth::answer(a.id(), false);
            return false;
        }
        self.password.clear();
        self.bt = Some(a);
        true
    }

    /// A Wi-Fi password question arrived. False when something else is being asked: the
    /// network then fails with "Password needed" and can be joined again.
    pub fn ask_wifi(&mut self, a: network::Ask) -> bool {
        if self.request.is_some() || self.bt.is_some() {
            network::answer(a.id(), None);
            return false;
        }
        self.password.clear();
        self.wifi = Some(a);
        true
    }

    /// A question ended elsewhere (answered by the service, or it gave up): Close when nothing
    /// is left to ask.
    pub fn ended_wifi(&mut self, id: u64) -> Outcome {
        if wifi_id(self) == Some(id) {
            self.wifi = None;
        }
        if self.is_empty() {
            Outcome::Close
        } else {
            Outcome::Stay
        }
    }
    pub fn ended_bt(&mut self, id: u64) -> Outcome {
        if bt_id(self) == Some(id) {
            self.bt = None;
        }
        if self.is_empty() {
            Outcome::Close
        } else {
            Outcome::Stay
        }
    }
    pub fn ended_polkit(&mut self, cookie: &str) -> Outcome {
        if self.request.as_ref().is_some_and(|r| r.cookie == cookie) {
            self.request = None;
        }
        if self.is_empty() {
            Outcome::Close
        } else {
            Outcome::Stay
        }
    }

    pub fn update(&mut self, m: Message) -> Outcome {
        let st = self;
        if st.request.is_none()
            && st.bt.is_none()
            && wifi_id(st).is_none()
            && let Some(k) = st.keyring.clone()
        {
            match m {
                Message::Password(p) => st.password = p,
                Message::Again(p) => st.again = p,
                Message::Choice(on) => st.choice = on,
                Message::Submit if keyring_ready(st, &k) && !st.checking => {
                    keyring::answer(
                        k.id,
                        Some(keyring::Reply {
                            password: std::mem::take(&mut st.password),
                            choice: st.choice,
                        }),
                    );
                    st.again.clear();
                    // The keyring says when it is over (it may ask again: a wrong password).
                    st.checking = true;
                    return Outcome::Stay;
                }
                Message::Submit => {}
                Message::Cancel => {
                    keyring::answer(k.id, None);
                    st.keyring = None;
                    st.password.clear();
                    st.again.clear();
                    return if st.is_empty() {
                        Outcome::Close
                    } else {
                        Outcome::Stay
                    };
                }
            }
            return Outcome::Stay;
        }
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
                    return Outcome::Close;
                }
                Message::Submit | Message::Again(_) | Message::Choice(_) => {}
                Message::Cancel => {
                    network::answer(id, None);
                    st.wifi = None;
                    return Outcome::Close;
                }
            }
            return Outcome::Stay;
        }
        if st.request.is_none()
            && let Some(a) = st.bt.clone()
        {
            match (m, a) {
                (Message::Password(p), _) => st.password = p,
                (Message::Submit, bluetooth::Ask::Confirm { id, .. }) => {
                    bluetooth::answer(id, true);
                    st.bt = None;
                    return Outcome::Close;
                }
                (Message::Submit, bluetooth::Ask::Pin { id, .. }) if !st.password.is_empty() => {
                    bluetooth::answer_pin(id, std::mem::take(&mut st.password));
                    st.bt = None;
                    return Outcome::Close;
                }
                (Message::Cancel, a) => {
                    bluetooth::answer(a.id(), false);
                    st.bt = None;
                    return Outcome::Close;
                }
                _ => {}
            }
            return Outcome::Stay;
        }
        match m {
            Message::Password(p) => st.password = p,
            Message::Again(_) | Message::Choice(_) => {}
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
                return Outcome::Close;
            }
        }
        Outcome::Stay
    }

    /// The window closed without an answer: every question asked is declined.
    pub fn closed(&mut self) {
        if let Some(r) = self.request.take() {
            polkit::cancel(&r.cookie);
        }
        if let Some(id) = bt_id(self) {
            bluetooth::answer(id, false);
        }
        self.bt = None;
        if let Some(id) = wifi_id(self) {
            network::answer(id, None);
        }
        self.wifi = None;
        if let Some(k) = self.keyring.take() {
            keyring::answer(k.id, None);
        }
        self.password.clear();
        self.again.clear();
    }
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

/// Whether the keyring's question can be answered: a password typed (a new one, twice the
/// same), or a yes/no.
fn keyring_ready(st: &State, k: &keyring::Prompt) -> bool {
    match k.kind {
        Some(keyring::Kind::Password) if k.new_password => {
            !st.password.is_empty() && st.password == st.again
        }
        Some(keyring::Kind::Password) => !st.password.is_empty(),
        _ => true,
    }
}

/// The prompt, its messages wrapped by `msg` for the host.
pub fn view<M: Clone + 'static>(st: &State, s: Scheme, msg: fn(Message) -> M) -> Element<'_, M> {
    let Some(r) = &st.request else {
        return match (&st.bt, &st.wifi, &st.keyring) {
            (Some(a), _, _) => bt_view(st, a, s, msg),
            (None, Some(network::Ask::Password { ssid, wrong, .. }), _) => {
                wifi_view(st, ssid, *wrong, s, msg)
            }
            (None, _, Some(k)) => keyring_view(st, k, s, msg),
            _ => iced::widget::Space::new().into(),
        };
    };
    let head = row![
        container(icon(Icon::Lock, tok::ICON_L, s.ink))
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
            .on_input(move |p| msg(Message::Password(p)))
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
        ui::button(
            s,
            if st.checking {
                "Checking…"
            } else {
                "Authenticate"
            },
            crate::ui::control::Pill::Primary,
            (!st.password.is_empty() && !st.checking).then(|| msg(Message::Submit)),
        ),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}

/// A pairing question: the device, the code (large, to compare or type), and the answer.
fn bt_view<'a, M: Clone + 'static>(
    st: &'a State,
    a: &'a bluetooth::Ask,
    s: Scheme,
    msg: fn(Message) -> M,
) -> Element<'a, M> {
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
        container(icon(Icon::Bluetooth, tok::ICON_L, s.ink))
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
                    .on_input(move |p| msg(Message::Password(p)))
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
        buttons = buttons.push(ui::button(
            s,
            p,
            crate::ui::control::Pill::Primary,
            (!pin || !st.password.is_empty()).then(|| msg(Message::Submit)),
        ));
    }
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}

/// A Wi-Fi network's password: the network, the field, and a quiet line when the saved one no
/// longer works.
fn wifi_view<'a, M: Clone + 'static>(
    st: &'a State,
    ssid: &'a str,
    wrong: bool,
    s: Scheme,
    msg: fn(Message) -> M,
) -> Element<'a, M> {
    let head = row![
        container(icon(Icon::Wifi, tok::ICON_L, s.ink))
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
            .on_input(move |p| msg(Message::Password(p)))
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
        ui::button(
            s,
            "Join",
            crate::ui::control::Pill::Primary,
            (st.password.chars().count() >= 8).then(|| msg(Message::Submit)),
        ),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}

/// A keyring's question, in its words: what it is (its message, e.g. "Authentication
/// required"), why (its description), the password (twice for a new one), why it asks again,
/// its check box, and its buttons' own labels.
fn keyring_view<'a, M: Clone + 'static>(
    st: &'a State,
    k: &'a keyring::Prompt,
    s: Scheme,
    msg: fn(Message) -> M,
) -> Element<'a, M> {
    let heading = [&k.message, &k.title]
        .into_iter()
        .find(|t| !t.is_empty())
        .map_or("Keyring", |t| t.as_str());
    let mut titles =
        column![label_w(heading, text::TITLE, s.ink, theme::SEMIBOLD)].spacing(tok::XXS);
    if !k.title.is_empty() && k.title != heading {
        titles = titles.push(label(k.title.as_str(), text::SMALL, s.ink_dim));
    }
    let head = row![
        container(icon(Icon::Lock, tok::ICON_L, s.ink))
            .center(40)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            }),
        titles,
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);
    let mut body = column![head].spacing(tok::L);
    if !k.description.is_empty() {
        body = body.push(
            iced::widget::text(k.description.as_str())
                .size(text::BODY)
                .color(s.ink)
                .font(theme::FONT)
                .line_height(iced::widget::text::LineHeight::Relative(1.35))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        );
    }
    let ready = keyring_ready(st, k) && !st.checking;
    if k.kind == Some(keyring::Kind::Password) {
        let field = |id: Option<iced::widget::Id>,
                     hint: &'static str,
                     v: &'a str,
                     on: fn(String) -> Message| {
            let mut t = text_input(hint, v)
                .secure(true)
                .on_input(move |p| msg(on(p)))
                .on_submit(msg(Message::Submit))
                .padding([tok::L, tok::L])
                .size(text::BODY)
                .font(theme::FONT)
                .style(crate::ui::input_style(s));
            if let Some(id) = id {
                t = t.id(id);
            }
            container(t).style(move |_| ui::field_style(s))
        };
        body = body.push(field(
            Some(INPUT_ID.clone()),
            "Password",
            &st.password,
            Message::Password,
        ));
        if k.new_password {
            body = body.push(field(None, "Password again", &st.again, Message::Again));
            if !st.again.is_empty() && st.again != st.password {
                body = body.push(label("The passwords are not the same", text::SMALL, s.bad));
            }
        }
    }
    if !k.warning.is_empty() {
        body = body.push(label(k.warning.as_str(), text::SMALL, s.bad));
    }
    if let Some((what, _)) = &k.choice {
        body = body.push(
            row![
                iced::widget::text(what.as_str())
                    .size(text::SMALL)
                    .color(s.ink_dim)
                    .font(theme::FONT)
                    .width(Length::Fill),
                ui::switch(s, st.choice, msg(Message::Choice(!st.choice))),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
        );
    }
    let or = |t: &'a str, d: &'static str| if t.is_empty() { d } else { t };
    let buttons = row![
        iced::widget::Space::new().width(Length::Fill),
        control(label_w(
            or(&k.cancel_label, "Cancel"),
            text::SMALL,
            s.ink,
            theme::MEDIUM
        ))
        .pill(s, crate::ui::control::Pill::Plain)
        .on_press(msg(Message::Cancel)),
        ui::button(
            s,
            if st.checking {
                "Checking…"
            } else {
                or(&k.continue_label, "Continue")
            },
            crate::ui::control::Pill::Primary,
            ready.then(|| msg(Message::Submit)),
        ),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    container(body.push(buttons))
        .padding(tok::XXL)
        .width(Length::Fill)
        .into()
}

/// A made-up question for screen tests (devtools builds only): the shell's `preview` command
/// and the prompt process's `KESTREL_PREVIEW`. polkit only asks the live session, and the user's own
/// session is let through without a password, so the real ones cannot be called up at will.
#[cfg(feature = "devtools")]
pub enum Preview {
    Polkit(polkit::Request),
    Wifi(network::Ask),
    Bt(bluetooth::Ask),
    Keyring(keyring::Prompt),
}

#[cfg(feature = "devtools")]
pub fn preview(which: &str) -> Option<Preview> {
    use bluetooth::Ask as Bt;
    use network::Ask as Net;
    let req = |retry| polkit::Request {
        cookie: "kestrel-preview".into(),
        message: "Authentication is required to install packages".into(),
        action: "org.archlinux.pkexec.pacman".into(),
        user: "user".into(),
        retry,
    };
    let id = 0xfeed;
    Some(match which {
        "password" => Preview::Polkit(req(false)),
        "password-wrong" => Preview::Polkit(req(true)),
        "wifi" | "wifi-wrong" => Preview::Wifi(Net::Password {
            id,
            ssid: "Home Wi-Fi".into(),
            wrong: which == "wifi-wrong",
        }),
        "pair-confirm" => Preview::Bt(Bt::Confirm {
            id,
            device: "AirPods".into(),
            code: Some("482913".into()),
        }),
        "pair-pin" => Preview::Bt(Bt::Pin {
            id,
            device: "Keychron K2".into(),
        }),
        "pair-show" => Preview::Bt(Bt::Show {
            id,
            device: "Keychron K2".into(),
            code: "771204".into(),
        }),
        // GNOME Keyring's own words for the login keyring (and a new keyring's, with its
        // check box).
        "keyring" | "keyring-wrong" => Preview::Keyring(keyring::Prompt {
            id,
            kind: Some(keyring::Kind::Password),
            title: "Unlock Login Keyring".into(),
            message: "Authentication required".into(),
            description: "The login keyring did not get unlocked when you logged into your \
                          computer."
                .into(),
            warning: if which == "keyring-wrong" {
                "The unlock password was incorrect".into()
            } else {
                String::new()
            },
            continue_label: "Unlock".into(),
            ..Default::default()
        }),
        "keyring-new" => Preview::Keyring(keyring::Prompt {
            id,
            kind: Some(keyring::Kind::Password),
            title: "New Keyring Password".into(),
            message: "Choose password for new keyring".into(),
            description: "An application wants to create a new keyring called “Default \
                          keyring”. Choose the password you want to use for it."
                .into(),
            new_password: true,
            choice: Some((
                "Automatically unlock this keyring whenever I’m logged in".into(),
                true,
            )),
            continue_label: "Continue".into(),
            ..Default::default()
        }),
        _ => return None,
    })
}
