//! The tray panel. Opened from the overflow button it lists every tray item: click to activate,
//! right-click or the chevron for its menu, the pin to keep it in the bar. Opened from a pinned
//! icon it shows that item's menu, rendered with kestrel's own rows; sub-menus open as a page
//! with a back row.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{icons, tray};
use crate::theme::{Scheme, size, text};
use crate::ui::{self, Icon, control, divider, icon, label};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Color, Element, Length, Task};
use std::collections::BTreeSet;

pub struct State {
    /// The item whose menu is shown; None shows the list of all items.
    pub item: Option<tray::Item>,
    pub menu: Option<tray::MenuItem>,
    /// Path of sub-menu ids from the root.
    pub path: Vec<i32>,
    /// The menu was opened from the list, so Back returns to it.
    pub from_list: bool,
    /// Items kept in the bar; the rest live behind the overflow button.
    pub pins: BTreeSet<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            item: None,
            menu: None,
            path: Vec::new(),
            from_list: false,
            pins: load_pins(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Loaded(String, Option<tray::MenuItem>),
    Click(i32),
    Enter(i32),
    Back,
    /// Show this item's menu inside the list.
    Show(tray::Item),
    Activate(tray::Item),
    Pin(String),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Tray(m))
}

/// The name an item is pinned under: its id, which survives restarts (the bus name does not).
pub fn pin_key(item: &tray::Item) -> String {
    if item.id.is_empty() {
        item.title.clone()
    } else {
        item.id.clone()
    }
}

fn pins_path() -> std::path::PathBuf {
    crate::config::state_dir().join("tray-pins")
}

fn load_pins() -> BTreeSet<String> {
    std::fs::read_to_string(pins_path())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn save_pins(pins: &BTreeSet<String>) {
    let path = pins_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let body: String = pins.iter().map(|p| format!("{p}\n")).collect();
    if let Err(e) = std::fs::write(&path, body) {
        log::warn!("tray: saving pins: {e}");
    }
}

fn load(st: &mut State, item: tray::Item) -> Task<AppMessage> {
    st.path.clear();
    st.menu = None;
    let key = item.key.clone();
    let task = match &item.menu {
        Some(m) => Task::perform(tray::menu(item.bus.clone(), m.clone()), move |m| {
            msg(Message::Loaded(key.clone(), m))
        }),
        None => Task::none(),
    };
    st.item = Some(item);
    task
}

/// Open the panel on `item`'s menu, or on the list of all items.
pub fn open(app: &mut App, item: Option<tray::Item>) -> Task<AppMessage> {
    let st = &mut app.panels.tray;
    st.from_list = false;
    match item {
        Some(item) => load(st, item),
        None => {
            st.item = None;
            st.menu = None;
            st.path.clear();
            Task::none()
        }
    }
}

/// Whether the panel currently shows `item` (None: the list).
pub fn showing(app: &App, item: Option<&tray::Item>) -> bool {
    let st = &app.panels.tray;
    match item {
        Some(i) => !st.from_list && st.item.as_ref().is_some_and(|t| t.key == i.key),
        None => st.item.is_none() || st.from_list,
    }
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.tray;
    match m {
        Message::Loaded(key, m) => {
            if st.item.as_ref().is_some_and(|i| i.key == key) {
                st.menu = m;
            }
        }
        Message::Enter(id) => st.path.push(id),
        Message::Back => {
            if st.path.pop().is_none() && st.from_list {
                st.item = None;
                st.menu = None;
                st.from_list = false;
            }
        }
        Message::Show(item) => {
            let t = load(st, item);
            st.from_list = true;
            return t;
        }
        Message::Activate(item) => {
            let t = Task::future(tray::activate(item.bus, item.path, false));
            return Task::batch([t.discard(), Task::done(AppMessage::Close)]);
        }
        Message::Pin(key) => {
            if !st.pins.remove(&key) {
                st.pins.insert(key);
            }
            save_pins(&st.pins);
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

fn find(m: &tray::MenuItem, id: i32) -> Option<&tray::MenuItem> {
    if m.id == id {
        return Some(m);
    }
    m.children.iter().find_map(|c| find(c, id))
}

fn back_row<'a>(s: Scheme, text_: String) -> Element<'a, AppMessage> {
    control(
        row![
            icon(Icon::ChevronLeft, size::ICON, s.ink_dim),
            label(ui::ellipsize(&text_, 28), text::BODY, s.ink_dim),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([0, 10])
    .height(34)
    .width(Length::Fill)
    .radius(size::ROW_RADIUS)
    .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
    .no_press_scale()
    .on_press(msg(Message::Back))
    .into()
}

/// Every tray app as a grid of icons with their names (like Windows 11's hidden icons): click
/// to open the app, right-click for its menu (where it can be kept in the bar). Apps kept in
/// the bar carry a small accent dot.
fn list(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    const CELL: f32 = 70.0;
    const COLS: usize = 4;
    let st = &app.panels.tray;
    let items: Vec<&tray::Item> = app.tray.items.iter().collect();
    if items.is_empty() {
        return Space::new().into();
    }
    let mut grid = column![].spacing(4);
    for chunk in items.chunks(COLS) {
        let mut r = row![].spacing(4);
        for &item in chunk {
            let pinned = st.pins.contains(&pin_key(item));
            let name = if item.title.is_empty() {
                item.id.clone()
            } else {
                item.title.clone()
            };
            let mut glyph = iced::widget::stack![
                container(crate::modules::tray_icon_at(s, item, 22.0)).center(30)
            ];
            if pinned {
                glyph = glyph.push(
                    iced::widget::pin(container(Space::new()).width(6).height(6).style(
                        move |_| container::Style {
                            background: Some(s.accent.into()),
                            border: iced::Border::default().rounded(3),
                            ..container::Style::default()
                        },
                    ))
                    .x(25.0)
                    .y(0.0),
                );
            }
            let primary = if item.item_is_menu {
                msg(Message::Show(item.clone()))
            } else {
                msg(Message::Activate(item.clone()))
            };
            r = r.push(
                control(
                    column![
                        glyph.width(30).height(30),
                        label(
                            ui::fit(&name, text::CAPTION, CELL - 8.0),
                            text::CAPTION,
                            s.ink_dim
                        ),
                    ]
                    .spacing(6)
                    .align_x(Alignment::Center),
                )
                .width(CELL)
                .height(CELL)
                .center_x()
                .radius(16.0)
                .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
                .no_press_scale()
                .on_press(primary)
                .on_right_press(msg(Message::Show(item.clone()))),
            );
        }
        grid = grid.push(r);
    }
    container(grid).padding(10).into()
}

/// "Keep in the bar", for the app whose menu is open.
fn keep_row<'a>(st: &State, s: Scheme) -> Element<'a, AppMessage> {
    let Some(item) = &st.item else {
        return Space::new().into();
    };
    let key = pin_key(item);
    let pinned = st.pins.contains(&key);
    row![
        icon(Icon::Pin, 14.0, if pinned { s.accent } else { s.ink_dim }),
        label("Keep in the bar", text::BODY, s.ink).width(Length::Fill),
        ui::switch(s, pinned, msg(Message::Pin(key))),
    ]
    .spacing(10)
    .padding([6, 10])
    .align_y(Alignment::Center)
    .into()
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.tray;
    if st.item.is_none() {
        return list(app, s);
    }
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
        if st.item.as_ref().is_some_and(|i| i.menu.is_none()) {
            return container(column![keep_row(st, s)]).padding(8).into();
        }
        let note = container(label(
            if st.item.as_ref().is_some_and(|i| i.menu.is_none()) {
                format!("{title} has no menu")
            } else {
                "Loading…".to_string()
            },
            text::SMALL,
            s.ink_dim,
        ))
        .padding(16);
        return if st.from_list {
            container(column![back_row(s, title), note].spacing(2))
                .padding(8)
                .into()
        } else {
            note.into()
        };
    };
    let current = st
        .path
        .last()
        .and_then(|id| find(root, *id))
        .unwrap_or(root);
    let mut col = column![].spacing(2);
    if !st.path.is_empty() || st.from_list {
        let back = if st.path.is_empty() {
            title.clone()
        } else {
            current.label.clone()
        };
        col = col.push(back_row(s, back));
        col = col.push(container(divider(s)).padding([4, 6]));
    }
    // Every app's own menu starts with whether it stays in the bar.
    if st.path.is_empty() {
        col = col.push(keep_row(st, s));
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
                Some(p) => ui::image_file_faded(&p, 14.0, s.opacity),
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
                .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
                .no_press_scale()
                .on_press_maybe(on),
        );
    }
    container(col).padding(8).into()
}
