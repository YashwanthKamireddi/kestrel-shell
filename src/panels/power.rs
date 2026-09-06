//! Power: lock, suspend, log out, restart, shut down. The last three ask once more.

use super::cx::Cx;
use crate::services::{hyprland, logind};
use crate::theme::text;
use crate::theme::tok;
use crate::ui::{Icon, control, icon, label};
use iced::widget::{Space, column, row};
use iced::{Alignment, Element, Length, Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Lock,
    Suspend,
    LogOut,
    Restart,
    ShutDown,
}

impl Action {
    const ALL: [Self; 5] = [
        Self::Lock,
        Self::Suspend,
        Self::LogOut,
        Self::Restart,
        Self::ShutDown,
    ];
    fn icon(self) -> Icon {
        match self {
            Self::Lock => Icon::Lock,
            Self::Suspend => Icon::Moon,
            Self::LogOut => Icon::LogOut,
            Self::Restart => Icon::RotateCcw,
            Self::ShutDown => Icon::Power,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Lock => "Lock",
            Self::Suspend => "Suspend",
            Self::LogOut => "Log out",
            Self::Restart => "Restart",
            Self::ShutDown => "Shut down",
        }
    }
    /// The armed row asks, in red ("Click again to …" was hint text: the user's rules).
    fn confirm(self) -> Option<&'static str> {
        match self {
            Self::LogOut => Some("Log out?"),
            Self::Restart => Some("Restart?"),
            Self::ShutDown => Some("Shut down?"),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct State {
    armed: Option<Action>,
    /// The row the keyboard is on.
    focus: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Press(Action),
}

type Act = super::action::Action<Message>;

pub fn update(st: &mut State, m: Message) -> Act {
    let Message::Press(a) = m;
    if a.confirm().is_some() && st.armed != Some(a) {
        st.armed = Some(a);
        return Act::None;
    }
    st.armed = None;
    Act::Close.and(Act::Run(run(a).discard()))
}

/// Does a session action (the control centre's lock button asks here too).
pub fn run(a: Action) -> Task<()> {
    match a {
        Action::Lock => Task::future(logind::run(logind::Action::Lock)),
        Action::Suspend => Task::future(logind::run(logind::Action::Suspend)),
        Action::Restart => Task::future(logind::run(logind::Action::Reboot)),
        Action::ShutDown => Task::future(logind::run(logind::Action::PowerOff)),
        Action::LogOut => Task::future(hyprland::dispatch("hl.dsp.exit()".into())),
    }
}

/// Up / Down move between the actions, Enter presses (the risky ones still ask a second time,
/// so Enter twice).
pub fn key(st: &mut State, k: super::Key) -> Act {
    use super::Key;
    let n = Action::ALL.len();
    let at = st.focus.unwrap_or(0);
    match k {
        Key::Down | Key::Tab => st.focus = Some(if st.focus.is_none() { 0 } else { (at + 1) % n }),
        Key::Up | Key::BackTab => {
            st.focus = Some(if st.focus.is_none() {
                n - 1
            } else {
                (at + n - 1) % n
            })
        }
        Key::Enter | Key::Space if st.focus.is_some() => {
            return update(st, Message::Press(Action::ALL[at]));
        }
        _ => {}
    }
    Act::None
}

pub fn view<'a>(st: &State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let armed = st.armed;
    let focus = st.focus;
    let rows = Action::ALL.map(|a| {
        let is_armed = armed == Some(a);
        let focused = focus.is_some_and(|i| Action::ALL[i] == a);
        let fg = if is_armed { s.base } else { s.ink };
        let kind = if is_armed {
            crate::ui::control::Kind::Danger
        } else if focused {
            crate::ui::control::Kind::Chosen
        } else {
            crate::ui::control::Kind::Ghost
        };
        let text_s = if is_armed {
            a.confirm().unwrap_or(a.label())
        } else {
            a.label()
        };
        control(
            row![
                icon(a.icon(), tok::ICON, fg),
                label(text_s, text::BODY, fg),
                Space::new().width(Length::Fill),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::L])
        .height(40)
        .width(Length::Fill)
        .radius(tok::R_ROW)
        .kind(s, kind)
        .on_press(Message::Press(a))
        .into()
    });
    column(rows).spacing(tok::XXS).padding(tok::M).into()
}
