//! Capture: a screenshot or a recording of a region, a window or the whole screen, now or
//! after a few seconds. Opened with `kestrel toggle capture` (and the camera in the control
//! centre). The panel closes first, so it is never in the picture; the tools are the same as
//! the keyboard shortcuts' (hyprshot, wf-recorder), so kestrel's own "Screenshot copied" and
//! recording pill follow as usual.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Color, Element, Length, Task};

pub const WIDTH: f32 = 360.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum What {
    #[default]
    Region,
    Window,
    Screen,
}

#[derive(Default)]
pub struct State {
    pub what: What,
    pub record: bool,
    /// Seconds to wait first.
    pub delay: u32,
}

#[derive(Clone, Debug)]
pub enum Message {
    What(What),
    Record(bool),
    Delay(u32),
    Go,
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Capture(m))
}

/// The shell command for a capture.
fn command(st: &State) -> String {
    let shots = "\"$(xdg-user-dir PICTURES)/Screenshots\"";
    let videos = "\"$(xdg-user-dir VIDEOS)/Recordings\"";
    let monitor = "\"$(hyprctl activeworkspace -j | jq -r '.monitor')\"";
    let file = "Recording_$(date '+%Y-%m-%d_%H.%M.%S').mp4";
    let run = if st.record {
        match st.what {
            What::Region => format!(
                "mkdir -p {videos} && g=\"$(slurp)\" && wf-recorder -g \"$g\" -f {videos}/{file}"
            ),
            // The window under the pointer when it starts (slurp's window picking).
            What::Window => format!(
                "mkdir -p {videos} && g=\"$(hyprctl clients -j | jq -r '.[] | select(.workspace.id != -1) | \"\\(.at[0]),\\(.at[1]) \\(.size[0])x\\(.size[1])\"' | slurp -r)\" && wf-recorder -g \"$g\" -f {videos}/{file}"
            ),
            What::Screen => {
                format!("mkdir -p {videos} && wf-recorder -a -o {monitor} -f {videos}/{file}")
            }
        }
    } else {
        let mode = match st.what {
            What::Region => "region",
            What::Window => "window",
            What::Screen => "output -m active",
        };
        // Saved in Pictures/Screenshots and copied.
        format!("mkdir -p {shots} && hyprshot -m {mode} -o {shots} --silent")
    };
    // A moment for the panel to be gone from the screen, then the delay asked for.
    format!("sleep {}.3 && {run}", st.delay)
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.capture;
    match m {
        Message::What(w) => st.what = w,
        Message::Record(r) => st.record = r,
        Message::Delay(d) => st.delay = d,
        Message::Go => {
            let cmd = command(st);
            return Task::batch([
                Task::done(AppMessage::Close),
                Task::done(AppMessage::Dispatch(
                    crate::services::compositor::Action::Exec(cmd),
                )),
            ]);
        }
    }
    Task::none()
}

/// A big choice: icon over its name, the chosen one filled.
fn choice<'a>(s: Scheme, i: Icon, name: &'a str, on: bool, m: Message) -> Element<'a, AppMessage> {
    control(
        container(
            column![
                icon(i, 22.0, if on { s.on_accent } else { s.ink }),
                label(name, text::SMALL, if on { s.on_accent } else { s.ink }),
            ]
            .spacing(6)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill),
    )
    .padding([12, 0])
    .width(Length::Fill)
    .radius(size::CARD_RADIUS)
    .colors(
        if on {
            s.accent
        } else {
            s.ink.scale_alpha(0.06 * s.opacity)
        },
        if on { s.accent } else { s.hover() },
        s.raised_press,
    )
    .no_press_scale()
    .on_press(msg(m))
    .into()
}

/// A small choice in a row (the kind, the delay).
fn chip<'a>(s: Scheme, name: String, on: bool, m: Message) -> Element<'a, AppMessage> {
    control(
        container(label(
            name,
            text::SMALL,
            if on { s.on_accent } else { s.ink },
        ))
        .center_x(Length::Fill),
    )
    .padding([6, 0])
    .width(Length::Fill)
    .radius(14.0)
    .colors(
        if on { s.accent } else { Color::TRANSPARENT },
        if on { s.accent } else { s.hover() },
        s.raised_press,
    )
    .no_press_scale()
    .on_press(msg(m))
    .into()
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.capture;
    let kind = container(
        row![
            chip(s, "Screenshot".into(), !st.record, Message::Record(false)),
            chip(s, "Record".into(), st.record, Message::Record(true)),
        ]
        .spacing(2),
    )
    .padding(2)
    .style(move |_| container::Style {
        background: Some(s.ink.scale_alpha(0.06 * s.opacity).into()),
        border: iced::Border::default().rounded(16),
        ..container::Style::default()
    });
    let what = row![
        choice(
            s,
            Icon::Crop,
            "Region",
            st.what == What::Region,
            Message::What(What::Region)
        ),
        choice(
            s,
            Icon::AppWindow,
            "Window",
            st.what == What::Window,
            Message::What(What::Window)
        ),
        choice(
            s,
            Icon::Monitor,
            "Screen",
            st.what == What::Screen,
            Message::What(What::Screen)
        ),
    ]
    .spacing(8);
    let mut delays = row![icon(Icon::Timer, 16.0, s.ink_dim), Space::new().width(4)]
        .spacing(2)
        .align_y(Alignment::Center);
    for d in [0u32, 3, 5, 10] {
        delays = delays.push(chip(
            s,
            if d == 0 {
                "Now".into()
            } else {
                format!("{d} s")
            },
            st.delay == d,
            Message::Delay(d),
        ));
    }
    let go = control(
        container(label_w(
            if st.record {
                "Start recording"
            } else {
                "Take screenshot"
            },
            text::BODY,
            s.on_accent,
            theme::SEMIBOLD,
        ))
        .center_x(Length::Fill),
    )
    .pill(s, crate::ui::control::Pill::Primary)
    .width(Length::Fill)
    .on_press(msg(Message::Go));
    container(column![kind, what, delays, go].spacing(14))
        .padding(16)
        .width(Length::Fill)
        .into()
}

/// Enter takes it; the rest is by pointer.
pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    if k == super::Key::Enter {
        return update(app, Message::Go);
    }
    Task::none()
}
