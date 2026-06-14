//! Shown only while something is in use: a screen recording (red dot and its time; a click
//! stops it), the screen shared, a camera in use, or an app recording from a microphone.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::theme::tok;
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
    let id = o.id;
    let shown = open == Some(PanelKind::Privacy);
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
                border: iced::Border::default().rounded(tok::FULL),
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
                .spacing(tok::M)
                .align_y(Alignment::Center),
                true,
            )
            // Only the recorder shown (pkill -x stopped every wf-recorder on the machine).
            .on_press_maybe(app.hypr.recording_pid.map(|pid| {
                Message::Dispatch(crate::services::compositor::Action::Exec(format!(
                    "kill -INT {pid}"
                )))
            }))
            .into(),
        );
    }
    // What is in use, together in one accent capsule (as iOS and macOS show it); when it all
    // stops, the capsule fades and narrows away (app.privacy_fade) instead of vanishing.
    let k = app.privacy_fade.value.clamp(0.0, 1.0);
    if k > 0.02 && !app.privacy_glyphs.is_empty() {
        items.push(capsule(
            s,
            &app.privacy_glyphs,
            k,
            app.privacy_w.value,
            id,
            shown,
        ));
    }

    items
}

/// A privacy dot: an accent circle with what is in use. No hover text (design call 2026-11-13).
/// What is in use now, as the capsule's icons: the screen shared, the camera, the microphone.
pub fn glyphs(app: &App) -> Vec<Icon> {
    let mut g = Vec::new();
    if app.hypr.screencast && app.hypr.recording.is_none() {
        g.push(Icon::ScreenShare);
    }
    if !app.camera_apps.is_empty() {
        g.push(Icon::Video);
    }
    if !app.audio.recording.is_empty() {
        g.push(Icon::Mic);
    }
    g
}

/// The capsule's full width for `n` icons.
pub fn capsule_width(n: usize) -> f32 {
    let n = n.max(1) as f32;
    n * 14.0 + (n - 1.0) * tok::S + 2.0 * tok::M
}

/// The in-use capsule: one accent pill with an icon for each thing in use, at strength `k`
/// (1 shown; going to 0 it fades and narrows to nothing).
fn capsule<'a>(
    s: Scheme,
    glyphs: &[Icon],
    k: f32,
    width: f32,
    id: iced::OutputId,
    open: bool,
) -> Element<'a, Message> {
    // Springs from nothing as it appears and back to nothing as it leaves.
    let full = width.max(0.0);
    let icons = row(glyphs
        .iter()
        .map(|g| icon(*g, 14.0, s.on_accent.scale_alpha(k)).into()))
    .spacing(tok::S)
    .align_y(Alignment::Center);
    crate::ui::control(
        iced::widget::container(icons)
            .padding([tok::NONE, tok::M])
            .width(full)
            .height(22)
            .center_y(22)
            .clip(true)
            .style(move |_| iced::widget::container::Style {
                background: Some(s.accent.scale_alpha(k).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..iced::widget::container::Style::default()
            }),
    )
    .padding(tok::XXS)
    .radius(tok::FULL)
    .bar(s, open)
    .on_press_at(move |r| Message::Toggle(PanelKind::Privacy, id, r))
    .into()
}
