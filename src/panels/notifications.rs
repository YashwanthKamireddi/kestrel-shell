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
            let h = ui::rounded_file(std::path::Path::new(p), 40.0, 40.0, 8.0)?;
            Some(image(h).width(40.0).height(40.0).into())
        }
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), 40.0, 40.0, 8.0)),
    }
}

/// One notification. `toast` cards show a close button always; centre cards on hover would
/// need hover state, so they show it always too, dimmed.
pub fn card<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
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
        control(container(icon(Icon::X, 13.0, s.ink_dim)).center(20))
            .radius(10.0)
            .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
            .on_press(msg(Message::Dismiss(n.id))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut texts = column![
        top,
        label_w(
            ui::ellipsize(&n.summary, 60),
            text::BODY,
            s.ink,
            theme::SEMIBOLD
        )
    ]
    .spacing(2)
    .width(Length::Fill);
    if !n.body.is_empty() {
        texts = texts.push(
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
    let mut body = row![app_icon(s, n), texts]
        .spacing(12)
        .align_y(Alignment::Start);
    if let Some(pic) = picture(n) {
        body = body.push(pic);
    }

    let mut col = column![body].spacing(10);
    let actions: Vec<_> = n
        .actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
        .take(3)
        .collect();
    if !actions.is_empty() {
        col = col.push(
            row(actions.into_iter().map(|(k, l)| {
                control(label(ui::ellipsize(l, 20), text::SMALL, s.ink))
                    .padding([6, 12])
                    .radius(size::CONTROL_RADIUS)
                    .colors(
                        s.ink.scale_alpha(0.08 * s.opacity),
                        s.selected(),
                        s.raised_press,
                    )
                    .center_x()
                    .width(Length::Fill)
                    .on_press(msg(Message::Action(n.id, k.clone())))
                    .into()
            }))
            .spacing(6),
        );
    }
    let has_default = n.actions.iter().any(|(k, _)| k == "default");
    control(col)
        .padding(12)
        .width(Length::Fill)
        .radius(size::CARD_RADIUS)
        .colors(
            s.ink.scale_alpha(0.06 * s.opacity),
            if has_default {
                s.ink.scale_alpha(0.1 * s.opacity)
            } else {
                s.ink.scale_alpha(0.06 * s.opacity)
            },
            s.ink.scale_alpha(0.06 * s.opacity),
        )
        .no_press_scale()
        .on_press_maybe(has_default.then(|| msg(Message::Action(n.id, "default".into()))))
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
            .padding([4, 10])
            .radius(12.0)
            .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
            .on_press(msg(Message::ClearAll)),
    ]
    .align_y(Alignment::Center)
    .padding([0, 4]);
    let list = column(app.notifications.iter().rev().map(|n| card(s, n))).spacing(8);
    let list = scrollable(list)
        .height(Length::Shrink)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ));
    column![header, container(list).max_height(380)]
        .spacing(8)
        .into()
}
