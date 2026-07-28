//! What is using the camera, microphone and screen: a small menu under the privacy indicator
//! (as GNOME and macOS show it). Each in-use device with the apps using it, by their icon and
//! name; the screen with its recording's stop. Opened by a click on any indicator.

use crate::app::{App, Message as AppMessage};
use crate::theme::{self, Scheme, text, tok};
use crate::ui::{self, Icon, icon, label, label_w};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

pub const WIDTH: f32 = crate::theme::size::PANEL_S;

/// An app's row: its icon (from its desktop entry) and its name.
fn app_row<'a>(s: Scheme, name: &str) -> Element<'a, AppMessage> {
    let glyph = ui::app_icon(s, name, 20.0, Icon::AppWindow);
    row![
        container(glyph).center(24),
        label_w(name.to_string(), text::BODY, s.ink, theme::MEDIUM).width(Length::Fill),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center)
    .height(tok::H_ROW)
    .padding([tok::NONE, tok::M])
    .into()
}

/// One device in use: its name with its icon, then the apps.
fn group<'a>(s: Scheme, i: Icon, what: &'a str, apps: &[String]) -> Element<'a, AppMessage> {
    let mut c = column![
        row![
            container(icon(i, 13.0, s.on_accent))
                .center(20)
                .style(move |_| container::Style {
                    background: Some(s.accent.into()),
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                }),
            label_w(what, text::SMALL, s.ink_dim, theme::SEMIBOLD),
        ]
        .spacing(tok::M)
        .align_y(Alignment::Center)
        .padding([tok::NONE, tok::M]),
    ]
    .spacing(tok::XXS);
    for a in apps {
        c = c.push(app_row(s, a));
    }
    c.into()
}

/// "now", "2 min ago".
pub fn ago(t: std::time::Instant) -> String {
    match t.elapsed().as_secs() / 60 {
        0 => "Just now".into(),
        m => format!("{m} min ago"),
    }
}

/// Camera and microphone use that ended in the last 10 minutes, newest first.
pub fn recent<'a>(s: Scheme, app: &App) -> Option<Element<'a, AppMessage>> {
    if app.privacy_recent.is_empty() {
        return None;
    }
    let mut c =
        column![label_w("Recently", text::SMALL, s.ink_dim, theme::SEMIBOLD),].spacing(tok::XXS);
    for (device, name, at) in &app.privacy_recent {
        let what = if *device == Icon::Video {
            "Camera"
        } else {
            "Microphone"
        };
        let glyph = ui::app_icon(s, name, 20.0, Icon::AppWindow);
        c = c.push(
            row![
                container(glyph).center(24),
                column![
                    label_w(name.clone(), text::BODY, s.ink, theme::MEDIUM),
                    label(format!("{what} · {}", ago(*at)), text::CAPTION, s.ink_dim),
                ]
                .width(Length::Fill),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
        );
    }
    Some(c.into())
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let mut body = column![].spacing(tok::L);
    if !app.camera_apps.is_empty() {
        body = body.push(group(s, Icon::Video, "Camera", &app.camera_apps));
    }
    if !app.audio.recording.is_empty() {
        body = body.push(group(s, Icon::Mic, "Microphone", &app.audio.recording));
    }
    if app.hypr.screencast {
        let what = if app.hypr.recording.is_some() {
            "Screen recording"
        } else {
            "Screen shared"
        };
        body = body.push(group(s, Icon::ScreenShare, what, &[]));
    }
    if app.camera_apps.is_empty() && app.audio.recording.is_empty() && !app.hypr.screencast {
        body = body.push(
            container(label("Nothing is using them now", text::BODY, s.ink_dim))
                .padding([tok::S, tok::M]),
        );
    }
    if let Some(r) = recent(s, app) {
        body = body.push(container(r).padding([tok::NONE, tok::M]));
    }
    container(body).padding(tok::M).width(Length::Fill).into()
}
