//! Shown only while something is in use: a screen recording (red dot and its time; a click
//! stops it), the screen shared, or an app recording from a microphone.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::{self, Scheme, text};
use crate::ui::{Icon, icon, label_w};
use iced::widget::{Space, row};
use iced::{Alignment, Element};

pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let _ = (o, open);
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    // Recording (wf-recorder): its own pill, a red dot and the time it has run, like a
    // phone's; a click stops it and the island says where it was saved.
    if app.hypr.screencast && app.hypr.recording.is_some() {
        let secs = app
            .hypr
            .recording_since
            .and_then(|t| t.elapsed().ok())
            .map_or(0, |d| d.as_secs());
        let time = if secs >= 3600 {
            format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
        } else {
            format!("{}:{:02}", secs / 60, secs % 60)
        };
        let red = s.fade(iced::Color::from_rgb8(0xFF, 0x45, 0x3A));
        let dot = iced::widget::container(Space::new())
            .width(8)
            .height(8)
            .style(move |_| iced::widget::container::Style {
                background: Some(red.into()),
                border: iced::Border::default().rounded(4),
                ..iced::widget::container::Style::default()
            });
        // Digits keep their place as they count (a fixed width per length).
        let digits = time.len() as f32 * 7.6;
        items.push(
            super::chip(
                s,
                row![
                    dot,
                    iced::widget::container(label_w(time, text::BODY, s.ink, theme::SEMIBOLD))
                        .width(digits)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                true,
            )
            .on_press(Message::Dispatch(
                crate::services::compositor::Action::Exec("pkill -INT -x wf-recorder".into()),
            ))
            .into(),
        );
    } else if app.hypr.screencast {
        // Shared (a call or a browser showing the screen): the accent mark.
        items.push(indicator(s, Icon::ScreenShare));
    }
    // Privacy: something is recording from a microphone.
    if !app.audio.recording.is_empty() {
        items.push(indicator(s, Icon::Mic));
    }

    items
}

/// A privacy dot: an accent circle with what is in use. No hover text (design call 2026-11-13).
fn indicator<'a>(s: Scheme, i: Icon) -> Element<'a, Message> {
    iced::widget::container(icon(i, 14.0, s.on_accent))
        .center(22)
        .style(move |_| iced::widget::container::Style {
            background: Some(s.accent.into()),
            border: iced::Border::default().rounded(11),
            ..iced::widget::container::Style::default()
        })
        .into()
}
