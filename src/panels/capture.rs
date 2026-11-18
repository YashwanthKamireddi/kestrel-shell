//! Capture: a screenshot or a recording of a region, a window or the whole screen, now or
//! after a few seconds. Opened with `kestrel toggle capture` (and the camera in the control
//! centre). The panel closes first, so it is never in the picture; the tools are the same as
//! the keyboard shortcuts' (hyprshot, wf-recorder), so kestrel's own "Screenshot copied" and
//! recording pill follow as usual. A timer counts down where you can see it (the pop-up
//! under the bar: 3, 2, 1, gone before the capture). Recordings take sound from nothing, the
//! computer (what is playing) or the microphone, the same for a region, a window or the screen.

use super::cx::Cx;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Element, Length, Task};

pub const WIDTH: f32 = crate::theme::tok::PANEL_M;

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
    /// Which countdown is the current one: Go again (from the panel reopened meanwhile)
    /// starts a new one, and the old one's seconds stop counting (two ran side by side,
    /// twice as fast, and both ended in a capture).
    countdown: u32,
}

#[derive(Clone, Debug)]
pub enum Message {
    What(What),
    Record(bool),
    Delay(u32),
    Sound(Sound),
    Go,
    /// A second of countdown `n` has passed.
    Tick(u32),
}

type Act = super::action::Action<Message>;

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

fn run(st: &State) -> Act {
    Act::Dispatch(crate::services::compositor::Action::Exec(command(st)))
}

fn second(n: u32) -> Act {
    Act::Run(Task::perform(
        tokio::time::sleep(std::time::Duration::from_secs(1)),
        move |()| Message::Tick(n),
    ))
}

/// The countdown's number, where the OSD shows.
fn show_left(st: &State) -> Act {
    let what = if st.record { "Recording" } else { "Screenshot" };
    Act::Notice(Icon::Timer, format!("{what} in {}", st.left))
}

pub fn update(st: &mut State, m: Message) -> Act {
    match m {
        Message::What(w) => st.what = w,
        Message::Record(r) => st.record = r,
        Message::Delay(d) => st.delay = d,
        Message::Sound(v) => st.sound = v,
        Message::Go => {
            st.countdown = st.countdown.wrapping_add(1);
            if st.delay == 0 {
                return Act::Close.and(run(st));
            }
            st.left = st.delay;
            // The panel goes first (its closing must not hide the first number).
            return Act::Close.and(Act::Run(Task::done(Message::Tick(st.countdown))));
        }
        Message::Tick(n) if n != st.countdown => {}
        Message::Tick(n) => {
            // After "1" has had its second: the number goes, then the capture.
            if st.left == 0 {
                return Act::HideNotice.and(run(st));
            }
            let shown = show_left(st);
            st.left -= 1;
            return shown.and(second(n));
        }
    }
    Act::None
}

/// A big choice: icon over its name, the chosen one filled.
fn choice<'a>(s: Scheme, i: Icon, name: &'a str, on: bool, m: Message) -> Element<'a, Message> {
    control(
        container(
            column![
                icon(i, tok::ICON_XL, if on { s.on_accent } else { s.ink }),
                label(name, text::SMALL, if on { s.on_accent } else { s.ink }),
            ]
            .spacing(tok::S)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill),
    )
    .padding([tok::L, tok::NONE])
    .width(Length::Fill)
    .radius(tok::R_CARD())
    .kind(
        s,
        if on {
            crate::ui::control::Kind::Primary
        } else {
            crate::ui::control::Kind::Tonal
        },
    )
    .no_press_scale()
    .on_press(m)
    .into()
}

/// A small choice in a row (the kind, the delay).
fn chip<'a>(s: Scheme, name: String, on: bool, m: Message) -> Element<'a, Message> {
    control(
        container(label(
            name,
            text::SMALL,
            if on { s.on_accent } else { s.ink },
        ))
        .center_x(Length::Fill),
    )
    .padding([tok::S, tok::NONE])
    .width(Length::Fill)
    .radius(tok::FULL)
    .kind(
        s,
        if on {
            crate::ui::control::Kind::Primary
        } else {
            crate::ui::control::Kind::Ghost
        },
    )
    .no_press_scale()
    .on_press(m)
    .into()
}

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let kind = container(
        row![
            chip(s, "Screenshot".into(), !st.record, Message::Record(false)),
            chip(s, "Record".into(), st.record, Message::Record(true)),
        ]
        .spacing(tok::XXS),
    )
    .padding(tok::XXS)
    .style(move |_| container::Style {
        background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
        border: iced::Border::default().rounded(tok::FULL),
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
    .spacing(tok::M);
    let mut delays = row![
        icon(Icon::Timer, tok::ICON, s.ink_dim),
        Space::new().width(tok::XS)
    ]
    .spacing(tok::XXS)
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
        let mut r = row![
            icon(Icon::Volume2, tok::ICON, s.ink_dim),
            Space::new().width(tok::XS)
        ]
        .spacing(tok::XXS)
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
    .on_press(Message::Go);
    let mut body = column![kind, what, delays].spacing(tok::L);
    if let Some(r) = sound {
        body = body.push(r);
    }
    container(body.push(go).spacing(tok::L))
        .padding(tok::PANEL_PAD)
        .width(Length::Fill)
        .into()
}

/// Enter takes it; the rest is by pointer.
pub fn key(st: &mut State, k: super::Key) -> Act {
    if k == super::Key::Enter {
        return update(st, Message::Go);
    }
    Act::None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many captures an action starts, and whether it waits for another second.
    fn outcome(a: &Act) -> (usize, bool) {
        match a {
            Act::Dispatch(_) => (1, false),
            Act::Run(_) => (0, true),
            Act::Many(v) => v
                .iter()
                .map(outcome)
                .fold((0, false), |(n, w), (m, x)| (n + m, w || x)),
            _ => (0, false),
        }
    }

    #[test]
    fn go_again_during_a_countdown_takes_one_capture() {
        // A second's wait is a tokio timer, made (not awaited) in `update`.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let _rt = rt.enter();
        let mut st = State {
            delay: 3,
            ..State::default()
        };
        // The seconds waiting to come (each countdown's next one), as the runtime would send.
        let mut due = Vec::new();
        let _ = update(&mut st, Message::Go);
        due.push(st.countdown);
        let mut taken = 0;
        let step = |st: &mut State, due: &mut Vec<u32>| {
            let n = due.remove(0);
            let (shots, again) = outcome(&update(st, Message::Tick(n)));
            if again {
                due.push(n);
            }
            shots
        };
        // A second in, the panel is opened again and Go pressed: a new countdown.
        taken += step(&mut st, &mut due);
        let _ = update(&mut st, Message::Go);
        due.push(st.countdown);
        while !due.is_empty() {
            taken += step(&mut st, &mut due);
        }
        assert_eq!(taken, 1);
    }
}
