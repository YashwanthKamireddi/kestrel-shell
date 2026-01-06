//! A tray item's menu, rendered with kestrel's own rows. Sub-menus open as a page with a
//! back row.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{icons, tray};
use crate::theme::{Scheme, size, text};
use crate::ui::{self, Icon, control, divider, icon, label};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Color, Element, Length, Task};

#[derive(Default)]
pub struct State {
    pub item: Option<tray::Item>,
    pub menu: Option<tray::MenuItem>,
    /// Path of sub-menu ids from the root.
    pub path: Vec<i32>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Loaded(Option<tray::MenuItem>),
    Click(i32),
    Enter(i32),
    Back,
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Tray(m))
}

/// Start loading `item`'s menu.
pub fn open(app: &mut App, item: tray::Item) -> Task<AppMessage> {
    let st = &mut app.panels.tray;
    st.path.clear();
    st.menu = None;
    let task = match &item.menu {
        Some(m) => Task::perform(tray::menu(item.bus.clone(), m.clone()), |m| {
            msg(Message::Loaded(m))
        }),
        None => Task::none(),
    };
    st.item = Some(item);
    task
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.tray;
    match m {
        Message::Loaded(m) => st.menu = m,
        Message::Enter(id) => st.path.push(id),
        Message::Back => {
            st.path.pop();
        }
        Message::Click(id) => {
            if let Some(item) = &st.item
                && let Some(menu) = &item.menu
            {
                let t = Task::future(tray::menu_event(item.bus.clone(), menu.clone(), id));
                return Task::batch([t.discard(), Task::done(AppMessage::Close)]);
            }
        }
    }
    Task::none()
}

fn find<'a>(m: &'a tray::MenuItem, id: i32) -> Option<&'a tray::MenuItem> {
    if m.id == id {
        return Some(m);
    }
    m.children.iter().find_map(|c| find(c, id))
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.tray;
    let title = st
        .item
        .as_ref()
        .map(|i| {
            if i.title.is_empty() {
                i.id.clone()
            } else {
                i.title.clone()
            }
        })
        .unwrap_or_default();
    let Some(root) = &st.menu else {
        return container(label(
            if st.item.as_ref().is_some_and(|i| i.menu.is_none()) {
                format!("{title} has no menu")
            } else {
                "Loading…".to_string()
            },
            text::SMALL,
            s.ink_dim,
        ))
        .padding(16)
        .into();
    };
    let current = st
        .path
        .last()
        .and_then(|id| find(root, *id))
        .unwrap_or(root);
    let mut col = column![].spacing(2);
    if !st.path.is_empty() {
        col = col.push(
            control(
                row![
                    icon(Icon::ChevronLeft, size::ICON, s.ink_dim),
                    label(current.label.clone(), text::BODY, s.ink_dim),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([0, 10])
            .height(34)
            .width(Length::Fill)
            .radius(size::ROW_RADIUS)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .no_press_scale()
            .on_press(msg(Message::Back)),
        );
        col = col.push(container(divider(s)).padding([4, 6]));
    }
    let any_marks = current
        .children
        .iter()
        .any(|c| c.toggle.is_some() || c.icon.is_some());
    for c in &current.children {
        if c.separator {
            col = col.push(container(divider(s)).padding([4, 6]));
            continue;
        }
        let fg = if c.enabled { s.ink } else { s.ink_faint };
        let lead: Element<'_, AppMessage> = match (c.toggle, &c.icon) {
            (Some(true), _) => icon(
                if c.radio {
                    Icon::CircleDot
                } else {
                    Icon::Check
                },
                14.0,
                s.accent,
            )
            .into(),
            (Some(false), _) => Space::new().width(14).into(),
            (None, Some(name)) => match icons::lookup(name) {
                Some(p) => ui::image_file(&p, 14.0),
                None => Space::new().width(14).into(),
            },
            (None, None) => Space::new().width(14).into(),
        };
        let has_sub = !c.children.is_empty();
        let mut r = row![].spacing(10).align_y(Alignment::Center);
        if any_marks {
            r = r.push(lead);
        }
        r = r.push(label(ui::ellipsize(&c.label, 40), text::BODY, fg).width(Length::Fill));
        if has_sub {
            r = r.push(icon(Icon::ChevronRight, 14.0, s.ink_dim));
        }
        let on = c.enabled.then(|| {
            if has_sub {
                msg(Message::Enter(c.id))
            } else {
                msg(Message::Click(c.id))
            }
        });
        col = col.push(
            control(r)
                .padding([0, 10])
                .height(34)
                .width(Length::Fill)
                .radius(size::ROW_RADIUS)
                .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
                .no_press_scale()
                .on_press_maybe(on),
        );
    }
    container(col).padding(8).into()
}
