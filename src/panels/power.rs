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

pub fn view<'a>(st: &State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let armed = st.armed;
    let rows = Action::ALL.map(|a| {
        let is_armed = armed == Some(a);
        let fg = if is_armed { s.base } else { s.ink };
        let kind = if is_armed {
            crate::ui::control::Kind::Danger
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
        .radius(tok::R_ROW())
        .kind(s, kind)
        .on_press(Message::Press(a))
        .into()
    });
    column(rows).spacing(tok::XXS).padding(tok::M).into()
}
