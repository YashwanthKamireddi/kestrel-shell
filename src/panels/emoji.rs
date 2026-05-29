//! Emoji picker (Super+.): a grid, recently used first. Type to filter by name; arrows move in
//! the grid; Enter (or a click) copies the emoji and types it into the window you came from.
//! Along the bottom, the categories (as on macOS and Windows): a click jumps to one, and the
//! one in view is marked as you scroll.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{column, container, mouse_area, row, text_input};
use iced::{Alignment, Color, Element, Length, Task};

const COLUMNS: usize = 10;
const ROWS: usize = 6;
const CELL: f32 = 50.0;
pub const WIDTH: f32 = COLUMNS as f32 * (CELL + 4.0) + 24.0;
const RECENT_MAX: usize = 30;

const EMOJI: &str = include_str!("../../assets/emoji.txt");

/// The Unicode groups, in the list's order: (the group's first emoji, its tab, its name).
const GROUPS: [(&str, &str, &str); 9] = [
    ("😀", "😀", "Smileys"),
    ("👋", "👋", "People"),
    ("🐵", "🐶", "Animals and nature"),
    ("🍇", "🍔", "Food and drink"),
    ("🌍", "✈️", "Travel and places"),
    ("🎃", "⚽", "Activities"),
    ("👓", "💡", "Objects"),
    ("🏧", "🔣", "Symbols"),
    ("🏁", "🏁", "Flags"),
];

/// Which group `e` is in (by its place in the list).
fn group_of(e: &str) -> usize {
    static AT: std::sync::LazyLock<(std::collections::HashMap<&'static str, usize>, [usize; 9])> =
        std::sync::LazyLock::new(|| {
            let line: std::collections::HashMap<&'static str, usize> = EMOJI
                .lines()
                .enumerate()
                .filter_map(|(i, l)| l.split_once(' ').map(|(e, _)| (e, i)))
                .collect();
            let starts = GROUPS.map(|(first, _, _)| line.get(first).copied().unwrap_or(0));
            (line, starts)
        });
    let (line, starts) = &*AT;
    let at = line.get(e).copied().unwrap_or(0);
    starts.iter().rposition(|&s| s <= at).unwrap_or(0)
}

#[derive(Default)]
pub struct State {
    /// Scrolling not yet worth a whole row.
    scroll: f32,
    query: String,
    selected: usize,
    /// First visible grid row.
    top: usize,
    recent: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// The pointer came onto an emoji: it becomes the selection.
    Hover(usize),
    Query(String),
    Pick(usize),
    Scroll(f32),
    /// Go to a category (None: recently used).
    Jump(Option<usize>),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Emoji(m))
}

static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("emoji-input"));

pub fn open(app: &mut App) -> Task<AppMessage> {
    let st = &mut app.panels.emoji;
    st.query.clear();
    st.selected = 0;
    st.top = 0;
    st.recent = crate::state::get(|s| s.emoji.recent.clone());
    Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

/// The emoji (from the list) the installed emoji font can draw. Newer ones than the font
/// (Twemoji lags Unicode) drew as empty cells; they are left out. Read once.
fn drawable() -> &'static [(&'static str, &'static str)] {
    static LIST: std::sync::LazyLock<Vec<(&'static str, &'static str)>> =
        std::sync::LazyLock::new(|| {
            let all = EMOJI.lines().filter_map(|l| l.split_once(' '));
            let file = std::process::Command::new("fc-match")
                .args(["-f", "%{file}", "Twemoji"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .filter(|f| f.to_lowercase().contains("twemoji"));
            let Some(data) = file.and_then(|f| std::fs::read(f).ok()) else {
                return all.collect();
            };
            let Ok(face) = ttf_parser::Face::parse(&data, 0) else {
                return all.collect();
            };
            // Every codepoint but joiners, variation selectors and tags must have a glyph.
            all.filter(|(e, _)| {
                e.chars()
                    .filter(|c| !matches!(*c as u32, 0x200D | 0xFE0E | 0xFE0F | 0xE0020..=0xE007F))
                    .all(|c| face.glyph_index(c).is_some())
            })
            .collect()
        });
    &LIST
}

/// (emoji, name) pairs to show: recent ones first when nothing is typed.
fn shown(st: &State) -> Vec<(&str, &str)> {
    let all = drawable().iter().copied();
    let q = st.query.trim().to_lowercase();
    if q.is_empty() {
        let name_of = |e: &str| {
            drawable()
                .iter()
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

/// How many of the shown emoji are the recently used ones at the top.
fn recent_count(st: &State) -> usize {
    if st.query.trim().is_empty() {
        st.recent.len()
    } else {
        0
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
            // Small touchpad steps add up to whole rows.
            st.scroll += lines;
            let whole = st.scroll.trunc();
            st.scroll -= whole;
            let top = (st.top as f32 - whole).max(0.0) as usize;
            st.top = top.min(rows.saturating_sub(ROWS));
            let first = st.top * COLUMNS;
            st.selected = st.selected.clamp(first, first + ROWS * COLUMNS - 1);
        }
        Message::Hover(i) => st.selected = i,
        Message::Jump(g) => {
            let list = shown(st);
            let recents = recent_count(st);
            let at = match g {
                None => Some(0),
                Some(g) => list
                    .iter()
                    .enumerate()
                    .skip(recents)
                    .find(|(_, (e, _))| group_of(e) == g)
                    .map(|(i, _)| i),
            };
            if let Some(i) = at {
                let rows = list.len().div_ceil(COLUMNS);
                st.top = (i / COLUMNS).min(rows.saturating_sub(ROWS));
                st.selected = i;
                st.scroll = 0.0;
            }
            // The keyboard back on the search (a click on a tab left it on nothing).
            return Task::Iced(iced::widget::focus(INPUT_ID.clone()));
        }
        Message::Pick(i) => {
            let Some((e, _)) = shown(st).get(i).copied() else {
                return Task::none();
            };
            let e = e.to_string();
            st.recent.retain(|r| *r != e);
            st.recent.insert(0, e.clone());
            st.recent.truncate(RECENT_MAX);
            let recent = st.recent.clone();
            crate::state::update(|s| s.emoji.recent = recent);
            // Copy it, and type it into the window that gets the keyboard back.
            let typed = format!("sleep 0.2; wtype {e}");
            return Task::batch([
                Task::Iced(iced::clipboard::write(e)),
                Task::done(AppMessage::Close),
                Task::done(AppMessage::Dispatch(
                    crate::services::compositor::Action::Exec(String::from(&typed)),
                )),
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
        .padding([tok::L, tok::L])
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
            .spacing(tok::XS)
            .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::L])
    .height(44)
    .center_y(44)
    .style(move |_| ui::field_style(s));

    let grid_h = ROWS as f32 * (CELL + 4.0);
    let mut grid = column![].spacing(tok::XS);
    for r in st.top..st.top + ROWS {
        let mut line = row![].spacing(tok::XS);
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
                .kind(
                    s,
                    if selected {
                        crate::ui::control::Kind::Chosen
                    } else {
                        crate::ui::control::Kind::Ghost
                    },
                )
                .on_enter(msg(Message::Hover(i)))
                .on_press(msg(Message::Pick(i))),
            );
        }
        grid = grid.push(line);
    }
    let body: Element<'_, AppMessage> = if list.is_empty() {
        // Nothing found: said where the grid would be.
        container(label(
            format!("No emoji for “{}”", st.query.trim()),
            text::BODY,
            s.ink_dim,
        ))
        .center_x(Length::Fill)
        .padding(iced::Padding {
            top: 48.0,
            ..iced::Padding::ZERO
        })
        .height(grid_h)
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
    let mut footer = row![
        label_w(
            ui::fit(&name, text::SMALL, 190.0),
            text::SMALL,
            s.ink,
            theme::MEDIUM
        )
        .width(Length::Fill)
    ]
    .spacing(tok::XXS)
    .align_y(Alignment::Center);
    // The categories, while browsing (a search shows only matches). The one in view is marked.
    if st.query.trim().is_empty() && !list.is_empty() {
        let recents = recent_count(st);
        // The selection's (categories do not start on a new row, so the first emoji in view
        // can still be the one before's).
        let here = if st.selected < recents {
            None
        } else {
            list.get(st.selected).map(|(e, _)| group_of(e))
        };
        let tab = |glyph: &'static str, on: bool, m: Option<usize>| {
            control(
                container(label(glyph, 16.0, s.ink).font(iced::Font::with_name("Twemoji")))
                    .center(30),
            )
            .radius(9.0)
            .kind(
                s,
                if on {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .on_press(msg(Message::Jump(m)))
        };
        if recents > 0 {
            footer = footer.push(tab("🕘", here.is_none(), None));
        }
        for (g, (_, glyph, _)) in GROUPS.iter().enumerate() {
            footer = footer.push(tab(glyph, here == Some(g), Some(g)));
        }
    }
    container(column![field, body, container(footer).padding([tok::NONE, tok::S])].spacing(tok::L))
        .padding(tok::L)
        .into()
}
