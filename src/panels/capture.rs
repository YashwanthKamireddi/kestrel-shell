//! Capture: a screenshot or a recording of a region, a window or the whole screen, now or
//! after a few seconds. Opened with `kestrel toggle capture` (and the camera in the control
//! centre). The panel closes first, so it is never in the picture; the tools are the same as
//! the keyboard shortcuts' (hyprshot, wf-recorder), so kestrel's own "Screenshot copied" and
//! recording pill follow as usual. A timer counts down where you can see it (the pop-up
//! under the bar: 3, 2, 1, gone before the capture). Recordings take sound from nothing, the
//! computer (what is playing) or the microphone, the same for a region, a window or the screen.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Element, Length, Task};

pub const WIDTH: f32 = 360.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum What {
    #[default]
    Region,
    Window,
    Screen,
}

/// Where a recording's sound comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sound {
    #[default]
    None,
    /// What the computer plays (the default output's monitor).
    Computer,
    Microphone,
}

#[derive(Default)]
pub struct State {
    pub what: What,
    pub record: bool,
    /// Seconds to wait first.
    pub delay: u32,
    pub sound: Sound,
    /// Seconds left of a countdown under way.
    left: u32,
}

#[derive(Clone, Debug)]
pub enum Message {
    What(What),
    Record(bool),
    Delay(u32),
    Sound(Sound),
    Go,
    /// A second of the countdown has passed.
    Tick,
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
    // The same sound for every kind (a whole-screen recording used to take the microphone and
    // the others none).
    let audio = match st.sound {
        Sound::None => "",
        Sound::Computer => " --audio=\"$(pactl get-default-sink).monitor\"",
        Sound::Microphone => " --audio=\"$(pactl get-default-source)\"",
    };
    let run = if st.record {
        match st.what {
            What::Region => format!(
                "mkdir -p {videos} && g=\"$(slurp)\" && wf-recorder{audio} -g \"$g\" -f {videos}/{file}"
            ),
            // The window under the pointer when it starts (slurp's window picking).
            What::Window => format!(
                "mkdir -p {videos} && g=\"$(hyprctl clients -j | jq -r '.[] | select(.workspace.id != -1) | \"\\(.at[0]),\\(.at[1]) \\(.size[0])x\\(.size[1])\"' | slurp -r)\" && wf-recorder{audio} -g \"$g\" -f {videos}/{file}"
            ),
            What::Screen => {
                format!("mkdir -p {videos} && wf-recorder{audio} -o {monitor} -f {videos}/{file}")
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
    // A moment for the panel (or the countdown) to be gone from the screen.
    format!("sleep 0.3 && {run}")
}

fn run(st: &State) -> Task<AppMessage> {
    Task::done(AppMessage::Dispatch(
        crate::services::compositor::Action::Exec(command(st)),
    ))
}

fn second() -> Task<AppMessage> {
    Task::perform(
        tokio::time::sleep(std::time::Duration::from_secs(1)),
        |()| msg(Message::Tick),
    )
}

/// The countdown's number, where the OSD shows.
fn show_left(app: &mut App) {
    let st = &app.panels.capture;
    let what = if st.record { "Recording" } else { "Screenshot" };
    app.show_event(Icon::Timer, format!("{what} in {}", st.left), String::new());
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.capture;
    match m {
        Message::What(w) => st.what = w,
        Message::Record(r) => st.record = r,
        Message::Delay(d) => st.delay = d,
        Message::Sound(v) => st.sound = v,
        Message::Go => {
            if st.delay == 0 {
                return Task::batch([Task::done(AppMessage::Close), run(st)]);
            }
            st.left = st.delay;
            // The panel goes first (its closing must not hide the first number).
            return Task::done(AppMessage::Close).chain(Task::done(msg(Message::Tick)));
        }
        Message::Tick => {
            // After "1" has had its second: the number goes, then the capture.
            if st.left == 0 {
                app.hide_osd();
                return run(&app.panels.capture);
            }
            show_left(app);
            app.panels.capture.left -= 1;
            return second();
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
    .kind(
        s,
        if on {
            crate::ui::control::Kind::Primary
        } else {
            crate::ui::control::Kind::Tonal
        },
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
    .kind(
        s,
        if on {
            crate::ui::control::Kind::Primary
        } else {
            crate::ui::control::Kind::Ghost
        },
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
    // Recording: where its sound comes from.
    let sound = st.record.then(|| {
        let mut r = row![icon(Icon::Volume2, 16.0, s.ink_dim), Space::new().width(4)]
            .spacing(2)
            .align_y(Alignment::Center);
        for (v, name) in [
            (Sound::None, "No sound"),
            (Sound::Computer, "Computer"),
            (Sound::Microphone, "Microphone"),
        ] {
            r = r.push(chip(s, name.into(), st.sound == v, Message::Sound(v)));
        }
        r
    });
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
    let mut body = column![kind, what, delays].spacing(14);
    if let Some(r) = sound {
        body = body.push(r);
    }
    container(body.push(go).spacing(14))
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
