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

/// Up / Down move between the actions, Enter presses (the risky ones still ask a second time,
/// so Enter twice).
pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    let n = Action::ALL.len();
    let st = &mut app.panels.power;
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
            return update(app, Message::Press(Action::ALL[at]));
        }
        _ => {}
    }
    Task::none()
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let armed = app.panels.power.armed;
    let focus = app.panels.power.focus;
    let rows = Action::ALL.map(|a| {
        let is_armed = armed == Some(a);
        let focused = focus.is_some_and(|i| Action::ALL[i] == a);
        let (fg, rest, hover) = if is_armed {
            (s.base, s.bad, s.bad)
        } else if focused {
            (s.ink, s.selected(), s.selected())
        } else {
            (s.ink, Color::TRANSPARENT, s.hover())
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
