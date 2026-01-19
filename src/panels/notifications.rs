//! Notification centre and toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, empty, icon, label, label_w, switch};
use iced::widget::{Space, column, container, image, row, scrollable};
use iced::{Alignment, Color, Element, Length, Task};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub enum Message {
    Dismiss(u32),
    Action(u32, String),
    ClearAll,
    Dnd,
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
        Message::Dnd => {
            app.dnd = !app.dnd;
            Task::none()
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

fn app_icon<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
    match icons::lookup(&n.app_icon) {
        Some(p) => ui::image_file_faded(&p, 16.0, s.opacity),
        None => icon(Icon::Bell, 14.0, s.ink_dim).into(),
    }
}

fn picture<'a>(n: &Notification) -> Option<Element<'a, AppMessage>> {
    let handle = match n.image.as_ref()? {
        Image::Path(p) if std::path::Path::new(p).is_file() => image::Handle::from_path(p),
        Image::Rgba(p) => crate::ui::pixels(p),
        Image::Path(_) => return None,
    };
    Some(ui::cover(handle, 40.0, 40.0, 8.0))
}

/// One notification. `toast` cards show a close button always; centre cards on hover would
/// need hover state, so they show it always too, dimmed.
pub fn card<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
    let header = row![
        app_icon(s, n),
        label(ui::ellipsize(&n.app, 28), text::CAPTION, s.ink_dim),
        label("·", text::CAPTION, s.ink_faint),
        label(ago(n.time), text::CAPTION, s.ink_faint),
        Space::new().width(Length::Fill),
        control(container(icon(Icon::X, 14.0, s.ink_dim)).center(22))
            .radius(11.0)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .on_press(msg(Message::Dismiss(n.id))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut texts = column![label_w(
        ui::ellipsize(&n.summary, 48),
        text::BODY,
        s.ink,
        theme::MEDIUM
    )]
    .spacing(2)
    .width(Length::Fill);
    if !n.body.is_empty() {
        texts = texts.push(
            iced::widget::text(ui::ellipsize(&n.body, 180))
                .size(text::SMALL)
                .color(s.ink_dim)
                .font(theme::FONT)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        );
    }
    let mut body = row![texts].spacing(12).align_y(Alignment::Start);
    if let Some(pic) = picture(n) {
        body = body.push(pic);
    }

    let mut col = column![header, body].spacing(8);
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
                    .colors(s.raised_hi, s.raised_press, s.raised_press)
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
            s.raised,
            if has_default { s.raised_hi } else { s.raised },
            s.raised,
        )
        .no_press_scale()
        .on_press_maybe(has_default.then(|| msg(Message::Action(n.id, "default".into()))))
        .into()
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let mut trailing = row![
        label("Do not disturb", text::SMALL, s.ink_dim),
        switch(s, app.dnd, msg(Message::Dnd)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !app.notifications.is_empty() {
        trailing = trailing.push(
            control(label("Clear", text::SMALL, s.ink))
                .padding([4, 10])
                .radius(size::CONTROL_RADIUS)
                .colors(s.raised, s.raised_hi, s.raised_press)
                .on_press(msg(Message::ClearAll)),
        );
    }
    let header = row![
        label_w("Notifications", text::HEADING, s.ink, theme::SEMIBOLD).width(Length::Fill),
        trailing,
    ]
    .align_y(Alignment::Center)
    .height(size::CONTROL + 4.0);

    let content: Element<'_, AppMessage> = if app.notifications.is_empty() {
        empty(s, Icon::BellOff, "You're all caught up")
    } else {
        let list = column(app.notifications.iter().rev().map(|n| card(s, n))).spacing(8);
        scrollable(list)
            .height(Length::Shrink)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().width(0).scroller_width(0),
            ))
            .into()
    };
    let max_h = 560.0;
    column![header, container(content).max_height(max_h)]
        .spacing(10)
        .padding(iced::Padding {
            top: 10.0,
            right: 12.0,
            bottom: 12.0,
            left: 16.0,
        })
        .into()
}
