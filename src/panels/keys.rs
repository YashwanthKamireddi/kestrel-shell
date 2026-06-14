//! Keybind cheatsheet, read live from Hyprland: every bind that has a description, grouped by
//! the text before the first ":" in it.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::hyprland;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{Icon, empty, label, label_w};
use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Element, Length, Task};

pub use crate::services::hyprland::Bind;

#[derive(Default)]
pub struct State {
    groups: Option<Vec<(String, Vec<Bind>)>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Loaded(Vec<(String, Vec<Bind>)>),
}

static LIST_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("keys-list"));

/// The arrows and Page Up / Down scroll the sheet; Home and End go to its ends.
pub fn key(_app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset};
    let by = |y: f32| {
        Task::Iced(iced::widget::scroll_by(
            LIST_ID.clone(),
            AbsoluteOffset { x: 0.0, y },
        ))
    };
    match k {
        Key::Down => by(60.0),
        Key::Up => by(-60.0),
        Key::PageDown | Key::Space => by(320.0),
        Key::PageUp => by(-320.0),
        Key::Home => Task::Iced(iced::widget::snap_to(
            LIST_ID.clone(),
            RelativeOffset { x: 0.0, y: 0.0 },
        )),
        Key::End => Task::Iced(iced::widget::snap_to_end(LIST_ID.clone())),
        _ => Task::none(),
    }
}

pub fn open(_app: &mut App) -> Task<AppMessage> {
    Task::perform(hyprland::binds(), |g| {
        AppMessage::Panel(PanelMessage::Keys(Message::Loaded(g)))
    })
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let Message::Loaded(g) = m;
    app.panels.keys.groups = Some(g);
    Task::none()
}

fn chip<'a>(s: Scheme, k: &str) -> Element<'a, AppMessage> {
    container(label(k.to_string(), text::CAPTION, s.ink))
        .padding([tok::XXS, tok::S])
        .style(move |_| container::Style {
            background: Some(s.raised_hi.into()),
            border: iced::Border::default().rounded(5),
            ..container::Style::default()
        })
        .into()
}

/// The list's most height before it scrolls (a 1080 p screen less the bar and margins).
const MAX_LIST_H: f32 = 900.0;

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let header = label_w("Keyboard shortcuts", text::HEADING, s.ink, theme::SEMIBOLD);
    // Not read yet, or no bindings found: the title and a quiet icon, not 520 px of nothing.
    let Some(groups) = app.panels.keys.groups.as_ref().filter(|g| !g.is_empty()) else {
        return container(column![header, empty(s, Icon::Keyboard)].spacing(tok::L))
            .padding(tok::L)
            .into();
    };
    // Two columns, filled group by group, balancing by row count.
    let mut cols: [Vec<Element<'_, AppMessage>>; 2] = [Vec::new(), Vec::new()];
    let mut heights = [0usize; 2];
    for (name, binds) in groups {
        let c = usize::from(heights[1] < heights[0]);
        heights[c] += binds.len() + 2;
        let mut g =
            column![label_w(name.clone(), text::SMALL, s.accent, theme::MEDIUM)].spacing(tok::S);
        for b in binds {
            let keys = row(b.keys.iter().map(|k| chip(s, k))).spacing(tok::XS);
            g = g.push(
                row![
                    label(crate::ui::ellipsize(&b.what, 44), text::SMALL, s.ink_dim)
                        .width(Length::Fill),
                    keys,
                ]
                .spacing(tok::M)
                .align_y(Alignment::Center),
            );
        }
        cols[c].push(g.into());
    }
    let [a, b] = cols;
    let body = row![
        column(a).spacing(tok::XL).width(Length::Fill),
        column(b).spacing(tok::XL).width(Length::Fill),
    ]
    .spacing(tok::XXXL);
    container(
        column![
            header,
            // As tall as all the shortcuts (a fixed box hid the rest below with nothing to say
            // so); it scrolls only when they don't fit on the screen.
            container(crate::ui::smooth(
                scrollable(body)
                    .id(LIST_ID.clone())
                    .height(Length::Shrink)
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::new().width(0).scroller_width(0),
                    ))
            ))
            .max_height(MAX_LIST_H),
        ]
        .spacing(tok::L),
    )
    .padding(iced::Padding {
        top: tok::L,
        right: tok::XL,
        bottom: tok::L,
        left: tok::XL,
    })
    .into()
}
