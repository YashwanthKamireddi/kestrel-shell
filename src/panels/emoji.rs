//! Emoji picker (Super+.): a grid, recently used first. Type to filter by name; arrows move in
//! the grid; Enter (or a click) copies the emoji and types it into the window you came from.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::apps;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, mouse_area, row, text_input};
use iced::{Alignment, Color, Element, Length, Task};

const COLUMNS: usize = 10;
const ROWS: usize = 6;
const CELL: f32 = 50.0;
pub const WIDTH: f32 = COLUMNS as f32 * (CELL + 4.0) + 24.0;
const RECENT_MAX: usize = 30;

const EMOJI: &str = include_str!("../../assets/emoji.txt");

#[derive(Default)]
pub struct State {
    query: String,
    selected: usize,
    /// First visible grid row.
    top: usize,
    recent: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Query(String),
    Pick(usize),
    Scroll(f32),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Emoji(m))
}

static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("emoji-input"));

fn recent_path() -> std::path::PathBuf {
    crate::config::state_dir().join("emoji-recent")
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    let st = &mut app.panels.emoji;
    st.query.clear();
    st.selected = 0;
    st.top = 0;
    st.recent = std::fs::read_to_string(recent_path())
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

/// (emoji, name) pairs to show: recent ones first when nothing is typed.
fn shown(st: &State) -> Vec<(&str, &str)> {
    let all = EMOJI.lines().filter_map(|l| l.split_once(' '));
    let q = st.query.trim().to_lowercase();
    if q.is_empty() {
        let name_of = |e: &str| {
            EMOJI
                .lines()
                .filter_map(|l| l.split_once(' '))
                .find(|(x, _)| *x == e)
                .map_or("", |(_, n)| n)
        };
        let recent: Vec<(&str, &str)> =
            st.recent.iter().map(|e| (e.as_str(), name_of(e))).collect();
        recent
            .iter()
            .copied()
            .chain(all.filter(|(e, _)| !st.recent.iter().any(|r| r == e)))
            .collect()
    } else {
        all.filter(|(_, name)| q.split_whitespace().all(|w| name.contains(w)))
            .collect()
    }
}

fn settle(st: &mut State, n: usize) {
    st.selected = st.selected.min(n.saturating_sub(1));
    let row = st.selected / COLUMNS;
    if row < st.top {
        st.top = row;
    } else if row >= st.top + ROWS {
        st.top = row + 1 - ROWS;
    }
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.emoji;
    match m {
        Message::Query(q) => {
            st.query = q;
            st.selected = 0;
            st.top = 0;
        }
        Message::Scroll(lines) => {
            let rows = shown(st).len().div_ceil(COLUMNS);
            let top = (st.top as f32 - lines).max(0.0) as usize;
            st.top = top.min(rows.saturating_sub(ROWS));
            let first = st.top * COLUMNS;
            st.selected = st.selected.clamp(first, first + ROWS * COLUMNS - 1);
        }
        Message::Pick(i) => {
            let Some((e, _)) = shown(st).get(i).copied() else {
                return Task::none();
            };
            let e = e.to_string();
            st.recent.retain(|r| *r != e);
            st.recent.insert(0, e.clone());
            st.recent.truncate(RECENT_MAX);
            let _ = std::fs::create_dir_all(crate::config::state_dir());
            let _ = std::fs::write(recent_path(), st.recent.join("\n"));
            // Copy it, and type it into the window that gets the keyboard back.
            let typed = format!("sleep 0.2; wtype {e}");
            return Task::batch([
                Task::Iced(iced::clipboard::write(e)),
                Task::done(AppMessage::Close),
                Task::done(AppMessage::Dispatch(apps::launch_lua(&typed))),
            ]);
        }
    }
    Task::none()
}

pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    let st = &mut app.panels.emoji;
    let n = shown(st).len();
    if n == 0 {
        return Task::none();
    }
    let i = st.selected as i64;
    let c = COLUMNS as i64;
    let to = match k {
        Key::Left => i - 1,
        Key::Right | Key::Tab => i + 1,
        Key::Up => i - c,
        Key::Down => i + c,
        Key::PageUp => i - c * ROWS as i64,
        Key::PageDown => i + c * ROWS as i64,
        Key::Home => 0,
        Key::End => n as i64 - 1,
        _ => return Task::none(),
    };
    st.selected = to.clamp(0, n as i64 - 1) as usize;
    settle(st, n);
    Task::none()
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.emoji;
    let list = shown(st);
    let input = text_input("Search emoji", &st.query)
        .id(INPUT_ID.clone())
        .on_input(|q| msg(Message::Query(q)))
        .on_submit(msg(Message::Pick(st.selected)))
        .padding([10, 12])
        .size(text::TITLE)
        .font(theme::FONT)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: iced::Border::default(),
            icon: s.ink_dim,
            placeholder: s.ink_faint,
            value: s.ink,
            selection: s.accent_soft,
        });
    let field = container(
        row![icon(Icon::Search, 18.0, s.ink_dim), input]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .height(44)
    .center_y(44)
    .style(move |_| container::Style {
        background: Some(s.raised.into()),
        border: iced::Border::default().rounded(size::CARD_RADIUS),
        ..container::Style::default()
    });

    let grid_h = ROWS as f32 * (CELL + 4.0);
    let mut grid = column![].spacing(4);
    for r in st.top..st.top + ROWS {
        let mut line = row![].spacing(4);
        for c in 0..COLUMNS {
            let i = r * COLUMNS + c;
            let Some((e, _)) = list.get(i) else {
                break;
            };
            let selected = i == st.selected;
            line = line.push(
                control(
                    container(
                        label(e.to_string(), 26.0, s.ink).font(iced::Font::with_name("Twemoji")),
                    )
                    .center(CELL),
                )
                .radius(10.0)
                .colors(
                    if selected {
                        s.accent_soft
                    } else {
                        Color::TRANSPARENT
                    },
                    if selected { s.accent_soft } else { s.raised_hi },
                    s.raised_press,
                )
                .on_press(msg(Message::Pick(i))),
            );
        }
        grid = grid.push(line);
    }
    let body: Element<'_, AppMessage> = if list.is_empty() {
        container(label("No matches", text::SMALL, s.ink_dim))
            .center_x(Length::Fill)
            .center_y(grid_h)
            .into()
    } else {
        mouse_area(container(grid).height(grid_h))
            .on_scroll(|d| {
                msg(Message::Scroll(match d {
                    iced::mouse::ScrollDelta::Lines { y, .. } => y,
                    iced::mouse::ScrollDelta::Pixels { y, .. } => y / CELL,
                }))
            })
            .into()
    };
    let name = list
        .get(st.selected)
        .map(|(_, n)| n.to_string())
        .unwrap_or_default();
    let section = if st.query.trim().is_empty() && !st.recent.is_empty() {
        if st.selected < st.recent.len() {
            "Recent"
        } else {
            "All"
        }
    } else {
        ""
    };
    let footer = row![
        label_w(name, text::SMALL, s.ink, theme::MEDIUM),
        Space::new().width(Length::Fill),
        label(section, text::CAPTION, s.ink_faint),
    ]
    .align_y(Alignment::Center);
    container(column![field, body, container(footer).padding([0, 6])].spacing(10))
        .padding(12)
        .into()
}
