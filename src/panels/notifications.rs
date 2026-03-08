//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row, scrollable};
use iced::{Alignment, Color, Element, Length, Task};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub enum Message {
    Dismiss(u32),
    Action(u32, String),
    ClearAll,
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Notifications(m))
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Dismiss(id) => {
            app.notifications.retain(|n| n.id != id);
            app.toasts.retain(|t| t.id != id);
            Task::future(notifications::closed(id, Reason::Dismissed)).discard()
        }
        // Kestrel's own entries: open a file or folder, or save a copied picture.
        Message::Action(id, key) if key.starts_with("kestrel-") => {
            app.toasts.retain(|t| t.id != id);
            if let Some(path) = key.strip_prefix("kestrel-open:") {
                let _ = std::process::Command::new("xdg-open")
                    .arg(path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            } else if let Some(path) = key.strip_prefix("kestrel-save:")
                && let Some(saved) = save_picture(path)
            {
                {
                    // The entry now points at the saved file.
                    if let Some(n) = app.notifications.iter_mut().find(|n| n.id == id) {
                        n.body = format!(
                            "Saved in {}",
                            saved
                                .parent()
                                .map_or(String::new(), |p| p.display().to_string())
                        );
                        n.actions =
                            vec![(format!("kestrel-open:{}", saved.display()), "Open".into())];
                    }
                }
            }
            Task::none()
        }
        Message::Action(id, key) => {
            app.notifications.retain(|n| n.id != id);
            app.toasts.retain(|t| t.id != id);
            Task::future(notifications::invoke(id, key)).discard()
        }
        Message::ClearAll => {
            let ids: Vec<u32> = app.notifications.iter().map(|n| n.id).collect();
            app.notifications.clear();
            app.toasts.clear();
            Task::batch(
                ids.into_iter()
                    .map(|id| Task::future(notifications::closed(id, Reason::Dismissed)).discard()),
            )
        }
    }
}

/// Copies a kept picture into Pictures/Screenshots under a dated name.
fn save_picture(from: &str) -> Option<std::path::PathBuf> {
    let pictures = std::process::Command::new("xdg-user-dir")
        .arg("PICTURES")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|p| !p.is_empty())?;
    let dir = std::path::Path::new(&pictures).join("Screenshots");
    std::fs::create_dir_all(&dir).ok()?;
    let ext = std::path::Path::new(from)
        .extension()
        .map_or("png".into(), |e| e.to_string_lossy().into_owned());
    let to = dir.join(format!(
        "Screenshot_{}.{ext}",
        chrono::Local::now().format("%Y-%m-%d_%H.%M.%S")
    ));
    std::fs::copy(from, &to).ok()?;
    Some(to)
}

fn ago(t: SystemTime) -> String {
    let s = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{} min", s / 60),
        3600..=86_399 => format!("{} h", s / 3600),
        _ => format!("{} d", s / 86_400),
    }
}

/// The app's own icon, large enough to tell at a glance who it is from (its name is tried
/// when it sent no icon: "vesktop", "kitty").
fn app_icon<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
    const D: f32 = 34.0;
    // Kestrel's own entries: its own glyph on a soft round, like its other icons.
    if n.app == "kestrel" {
        let glyph = if n.app_icon == "media-record" {
            Icon::Video
        } else {
            Icon::Camera
        };
        return container(icon(glyph, 17.0, s.ink))
            .center(D)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(0.1 * s.opacity).into()),
                border: iced::Border::default().rounded(D / 2.0),
                ..container::Style::default()
            })
            .into();
    }
    let found = icons::lookup(&n.app_icon)
        .or_else(|| icons::lookup(&n.app.to_lowercase()))
        .or_else(|| icons::lookup(&n.app.to_lowercase().replace(' ', "-")));
    match found {
        Some(p) => container(ui::image_file_faded(&p, D, s.opacity))
            .center(D)
            .into(),
        None => container(icon(Icon::Bell, 18.0, s.ink))
            .center(D)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(0.1 * s.opacity).into()),
                border: iced::Border::default().rounded(D / 2.0),
                ..container::Style::default()
            })
            .into(),
    }
}

fn picture<'a>(n: &Notification) -> Option<Element<'a, AppMessage>> {
    match n.image.as_ref()? {
        // Decoded first: an app's unreadable file must never reach the renderer.
        Image::Path(p) => {
            let h = ui::rounded_file(std::path::Path::new(p), 52.0, 52.0, 10.0)?;
            Some(image(h).width(52.0).height(52.0).into())
        }
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), 52.0, 52.0, 10.0)),
    }
}

/// One notification. In the list it is a card of its own; as a banner (`boxed` false) the
/// banner's box is its card, so there is no box inside a box.
pub fn card<'a>(s: Scheme, n: &Notification, boxed: bool) -> Element<'a, AppMessage> {
    // Laid out like a phone's banner: the app's icon on the left; beside it the app and when on
    // one quiet line, the title, then the message wrapped over up to four lines.
    let top = row![
        label_w(
            ui::ellipsize(&n.app, 28),
            text::CAPTION,
            s.ink_dim,
            theme::MEDIUM
        ),
        Space::new().width(Length::Fill),
        label(ago(n.time), text::CAPTION, s.ink_faint),
        // A real button: a soft round fill, big enough to hit (it was a bare 13 px cross).
        control(container(icon(Icon::X, 12.0, s.ink)).center(24))
            .radius(12.0)
            .colors(
                s.ink.scale_alpha(0.08 * s.opacity),
                s.ink.scale_alpha(0.16 * s.opacity),
                s.ink.scale_alpha(0.22 * s.opacity),
            )
            .on_press(msg(Message::Dismiss(n.id))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut words = column![label_w(
        ui::ellipsize(&n.summary, 60),
        text::BODY,
        s.ink,
        theme::SEMIBOLD
    )]
    .spacing(2)
    .width(Length::Fill);
    if !n.body.is_empty() {
        words = words.push(
            // Up to four lines of it, wrapped: enough to read without opening the app.
            iced::widget::text(ui::ellipsize(
                &n.body.lines().take(4).collect::<Vec<_>>().join("\n"),
                320,
            ))
            .size(text::SMALL)
            .line_height(iced::widget::text::LineHeight::Relative(1.35))
            .color(s.ink_dim)
            .font(theme::FONT)
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        );
    }
    // The picture beside the words, under the close button (not past it).
    let mut main = row![words].spacing(10).align_y(Alignment::Start);
    if let Some(pic) = picture(n) {
        main = main.push(pic);
    }
    let mut texts = column![top, main].spacing(2).width(Length::Fill);
    // Actions: small pills under the words, as wide as their names.
    let actions: Vec<_> = n
        .actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
        .take(3)
        .collect();
    if !actions.is_empty() {
        texts = texts.push(
            container(
                row(actions.into_iter().map(|(k, l)| {
                    control(label_w(
                        ui::ellipsize(l, 24),
                        text::SMALL,
                        s.ink,
                        theme::MEDIUM,
                    ))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(msg(Message::Action(n.id, k.clone())))
                    .into()
                }))
                .spacing(6),
            )
            .padding(iced::Padding {
                top: 8.0,
                ..iced::Padding::ZERO
            }),
        );
    }
    let body = row![app_icon(s, n), texts]
        .spacing(12)
        .align_y(Alignment::Start);
    let col = column![body];
    // What a click on the card does: the app's default action, or for kestrel's own entries
    // (a recording, a picture) opening it.
    let default = n
        .actions
        .iter()
        .find(|(k, _)| k == "default" || k.starts_with("kestrel-open:"))
        .map(|(k, _)| k.clone());
    let has_default = default.is_some();
    let rest = if boxed {
        s.ink.scale_alpha(0.06 * s.opacity)
    } else {
        Color::TRANSPARENT
    };
    control(col)
        .padding(12)
        .width(Length::Fill)
        .radius(size::CARD_RADIUS)
        .colors(
            rest,
            if has_default {
                s.ink.scale_alpha(0.08 * s.opacity)
            } else {
                rest
            },
            rest,
        )
        .no_press_scale()
        .on_press_maybe(default.map(|k| msg(Message::Action(n.id, k))))
        .into()
}

/// Notifications inside the control centre: the newest first, cleared one by one or all at
/// once. Nothing at all when there are none (no "empty" message).
pub fn section(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    if app.notifications.is_empty() {
        return Space::new().into();
    }
    let header = row![
        label_w("Notifications", text::BODY, s.ink, theme::SEMIBOLD).width(Length::Fill),
        control(label("Clear all", text::SMALL, s.ink_dim))
            .pill(s, crate::ui::control::Pill::Quiet)
            .on_press(msg(Message::ClearAll)),
    ]
    .align_y(Alignment::Center)
    .padding([0, 4]);
    let list = column(app.notifications.iter().rev().map(|n| card(s, n, true))).spacing(8);
    let list = scrollable(list)
        .height(Length::Shrink)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ));
    column![header, container(list).max_height(380)]
        .spacing(8)
        .into()
}
