//! Power: lock, suspend, log out, restart, shut down. The last three ask once more.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{hyprland, logind};
use crate::theme::{Scheme, size, text};
use crate::ui::{Icon, control, icon, label};
use iced::widget::{Space, column, row};
use iced::{Alignment, Color, Element, Length, Task};

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
    fn confirm(self) -> Option<&'static str> {
        match self {
            Self::LogOut => Some("Click again to log out"),
            Self::Restart => Some("Click again to restart"),
            Self::ShutDown => Some("Click again to shut down"),
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

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let Message::Press(a) = m;
    if a.confirm().is_some() && app.panels.power.armed != Some(a) {
        app.panels.power.armed = Some(a);
        return Task::none();
    }
    app.panels.power.armed = None;
    let run = match a {
        Action::Lock => Task::future(logind::run(logind::Action::Lock)),
        Action::Suspend => Task::future(logind::run(logind::Action::Suspend)),
        Action::Restart => Task::future(logind::run(logind::Action::Reboot)),
        Action::ShutDown => Task::future(logind::run(logind::Action::PowerOff)),
        Action::LogOut => Task::future(hyprland::dispatch("hl.dsp.exit()".into())),
    };
    Task::batch([Task::done(AppMessage::Close), run.discard()])
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let armed = app.panels.power.armed;
    let rows = Action::ALL.map(|a| {
        let is_armed = armed == Some(a);
        let (fg, rest, hover) = if is_armed {
            (s.base, s.bad, s.bad)
        } else {
            (s.ink, Color::TRANSPARENT, s.raised_hi)
        };
        let text_s = if is_armed {
            a.confirm().unwrap_or(a.label())
        } else {
            a.label()
        };
        control(
            row![
                icon(a.icon(), size::ICON, fg),
                label(text_s, text::BODY, fg),
                Space::new().width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([0, 12])
        .height(40)
        .width(Length::Fill)
        .radius(size::ROW_RADIUS)
        .colors(rest, hover, if is_armed { s.bad } else { s.raised_press })
        .on_press(AppMessage::Panel(PanelMessage::Power(Message::Press(a))))
        .into()
    });
    column(rows).spacing(2).padding(8).into()
}
