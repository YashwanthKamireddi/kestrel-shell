//! System monitor: each core's load, memory and swap, and the busiest of your processes, each
//! with a way to end it (asked once: "End?"). Live every 1.5 s while open, nothing when closed.
//! Opened from the CPU and memory pill or `kestrel toggle system`.

use super::Message as PanelMessage;
use super::cx::Cx;
use crate::app::Message as AppMessage;
use crate::services::system::Detail;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Element, Length, Task};

pub const WIDTH: f32 = crate::theme::tok::PANEL_M;
/// The process table's columns (header and rows share them, so the names sit over their
/// numbers): CPU, memory, and the end button.
const COL_CPU: f32 = 54.0;
const COL_MEM: f32 = 70.0;
const COL_END: f32 = 46.0;

#[derive(Default)]
pub struct State {
    pub detail: Option<Detail>,
    /// The process whose end button was pressed once.
    armed: Option<u32>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Detail(Detail),
    End(u32),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Monitor(m))
}

pub fn update(st: &mut State, m: Message) -> Task<AppMessage> {
    match m {
        Message::Detail(d) => st.detail = Some(d),
        Message::End(pid) => {
            if st.armed != Some(pid) {
                st.armed = Some(pid);
                return Task::none();
            }
            st.armed = None;
            // Safety: kill(2) with SIGTERM on one of the user's own processes (only those are
            // listed): asks it to close, as a window's close button does.
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            if let Some(d) = &mut st.detail {
                d.top.retain(|p| p.0 != pid);
            }
        }
    }
    Task::none()
}

fn gib(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

/// A thin bar, `v` of 0..1 filled.
fn bar<'a>(s: Scheme, v: f32, h: f32) -> Element<'a, AppMessage> {
    let v = v.clamp(0.0, 1.0);
    let hot = if v > 0.85 { s.bad } else { s.accent };
    row![
        container(Space::new())
            .width(Length::FillPortion(((v * 1000.0) as u16).max(1)))
            .height(h)
            .style(move |_| container::Style {
                background: Some(hot.into()),
                border: iced::Border::default().rounded(h / 2.0),
                ..container::Style::default()
            }),
        container(Space::new())
            .width(Length::FillPortion((((1.0 - v) * 1000.0) as u16).max(1)))
            .height(h)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
                border: iced::Border::default().rounded(h / 2.0),
                ..container::Style::default()
            }),
    ]
    .into()
}

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, AppMessage> {
    let s = cx.scheme;
    let title = label_w("System", text::HEADING, s.ink, theme::SEMIBOLD);
    let Some(d) = &st.detail else {
        // The first reading is 1.5 s away: the cores from the dashboard's own numbers meanwhile.
        return container(
            column![title, crate::ui::waiting(s, Icon::Activity, "Reading…")].spacing(tok::L),
        )
        .padding(crate::theme::tok::PANEL_PAD)
        .into();
    };
    // Cores: one bar each, in two columns.
    let mut cores = column![].spacing(tok::S);
    for pair in d.cores.chunks(2).enumerate() {
        let (k, c) = pair;
        let mut r = row![].spacing(tok::L);
        for (j, v) in c.iter().enumerate() {
            r = r.push(
                row![
                    container(label(
                        format!("{}", k * 2 + j + 1),
                        text::CAPTION,
                        s.ink_faint
                    ))
                    .width(16),
                    container(bar(s, f32::from(*v) / 100.0, 6.0)).width(Length::Fill),
                    container(label(format!("{v}%"), text::CAPTION, s.ink_dim))
                        .width(34)
                        .align_right(34),
                ]
                .spacing(tok::M)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            );
        }
        cores = cores.push(r);
    }
    let memory = |name: &'static str, (used, total): (u64, u64)| {
        column![
            row![
                label(name, text::SMALL, s.ink).width(Length::Fill),
                label(
                    format!("{} of {}", gib(used), gib(total)),
                    text::SMALL,
                    s.ink_dim
                ),
            ],
            bar(s, used as f32 / total.max(1) as f32, 6.0),
        ]
        .spacing(tok::S)
    };
    let mut mem = column![memory("Memory", d.mem)].spacing(tok::L);
    if d.swap.1 > 0 {
        mem = mem.push(memory("Swap", d.swap));
    }
    // The busiest processes.
    let mut procs = column![
        row![
            label("Process", text::CAPTION, s.ink_faint).width(Length::Fill),
            container(label("CPU", text::CAPTION, s.ink_faint))
                .width(COL_CPU)
                .align_right(COL_CPU),
            container(label("Memory", text::CAPTION, s.ink_faint))
                .width(COL_MEM)
                .align_right(COL_MEM),
            Space::new().width(COL_END),
        ]
        .padding([tok::NONE, tok::L])
    ]
    .spacing(tok::XXS);
    for (pid, name, cpu, rss) in &d.top {
        let armed = st.armed == Some(*pid);
        // The session itself (the compositor, this shell, sound, the session bus) has no end
        // button: ending one would end the desktop.
        let vital = matches!(
            name.as_str(),
            "Hyprland"
                | "kestrel"
                | "systemd"
                | "pipewire"
                | "pipewire-pulse"
                | "wireplumber"
                | "dbus-broker"
                | "dbus-daemon"
                | "Xwayland"
                | "xdg-desktop-portal"
                | "xdg-desktop-portal-hyprland"
        );
        let end: Element<'_, AppMessage> = if vital {
            // Not ended from here (it would end the desktop): a lock says why the ✕ is missing.
            container(icon(Icon::Lock, tok::ICON_XS, s.ink_faint))
                .center_x(26)
                .into()
        } else if armed {
            control(label_w("End?", text::CAPTION, s.base, theme::SEMIBOLD))
                .padding([tok::XS, tok::M])
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Danger)
                .on_press(msg(Message::End(*pid)))
                .into()
        } else {
            control(icon(Icon::X, tok::ICON_S, s.ink_dim))
                .padding(tok::XS)
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Ghost)
                .on_press(msg(Message::End(*pid)))
                .into()
        };
        procs = procs.push(
            container(
                row![
                    label(crate::ui::ellipsize(name, 28), text::SMALL, s.ink).width(Length::Fill),
                    container(label(format!("{cpu:.0}%"), text::SMALL, s.ink_dim))
                        .width(54)
                        .align_right(54),
                    container(label(
                        format!("{:.0} MB", *rss as f64 / 1e6),
                        text::SMALL,
                        s.ink_dim
                    ))
                    .width(70)
                    .align_right(70),
                    container(end).width(COL_END).align_right(COL_END),
                ]
                .spacing(tok::NONE)
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::L])
            .height(tok::H_ROW)
            .align_y(Alignment::Center),
        );
    }
    container(
        column![
            title,
            crate::ui::card(s, container(cores).padding([tok::XS, tok::XXS])),
            crate::ui::card(s, container(mem).padding([tok::XS, tok::XXS])),
            crate::ui::card(s, container(procs).padding([tok::XS, tok::XXS])),
        ]
        .spacing(tok::L),
    )
    .padding(crate::theme::tok::PANEL_PAD)
    .width(Length::Fill)
    .into()
}
