//! Panels: what hangs from the bar. Each panel is a view built only from the component kit.

pub mod calendar;
pub mod power;

use crate::app::{App, Message as AppMessage, Output};
use crate::theme::Scheme;
use iced::{Element, Subscription, Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelKind {
    Control,
    Media,
    Calendar,
    Notifications,
    Power,
    Tray,
    Launcher,
}

impl PanelKind {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "control" | "control-centre" | "control-center" => Self::Control,
            "media" => Self::Media,
            "calendar" => Self::Calendar,
            "notifications" => Self::Notifications,
            "power" => Self::Power,
            "launcher" => Self::Launcher,
            _ => return None,
        })
    }

    pub fn width(self) -> f32 {
        match self {
            Self::Control => 360.0,
            Self::Media => 380.0,
            Self::Calendar => 312.0,
            Self::Notifications => 380.0,
            Self::Power => 232.0,
            Self::Tray => 240.0,
            Self::Launcher => 560.0,
        }
    }
}

/// Per-panel UI state that outlives a single open.
#[derive(Default)]
pub struct State {
    pub calendar: calendar::State,
    pub power: power::State,
}

impl State {
    /// A panel was just opened: reset transient state.
    pub fn opened(&mut self, kind: PanelKind) {
        match kind {
            PanelKind::Calendar => self.calendar = calendar::State::default(),
            PanelKind::Power => self.power = power::State::default(),
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Calendar(calendar::Message),
    Power(power::Message),
}

pub fn update(app: &mut App, msg: Message) -> Task<AppMessage> {
    match msg {
        Message::Calendar(m) => {
            app.panels.calendar.update(m);
            Task::none()
        }
        Message::Power(m) => power::update(app, m),
    }
}

pub fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::none()
}

pub fn view(app: &App, kind: PanelKind, s: Scheme) -> Element<'_, AppMessage> {
    match kind {
        PanelKind::Calendar => calendar::view(app, s),
        PanelKind::Power => power::view(app, s),
        _ => iced::widget::Space::new().into(),
    }
}

/// Bar buttons contributed by panels (right side, before the power button).
pub fn bar_items<'a>(
    _app: &'a App,
    _o: &'a Output,
    _s: Scheme,
    _open: Option<PanelKind>,
) -> Vec<Element<'a, AppMessage>> {
    Vec::new()
}
