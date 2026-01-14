//! Overview: every workspace and its windows at a glance. Type to filter windows; Enter goes to
//! the first match; click a window to focus it, or a workspace to switch to it.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{apps, hyprland, icons};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row, text_input};
use iced::{Alignment, Color, Element, Length, Task};

const COLUMNS: usize = 4;
pub const WIDTH: f32 = 1000.0;

#[derive(Default)]
pub struct State {
    query: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    Query(String),
    Submit,
}

static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("overview-input"));

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Overview(m))
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    app.panels.overview.query.clear();
    Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

fn matches(w: &hyprland::Window, q: &str) -> bool {
    q.is_empty()
        || w.title.to_lowercase().contains(q)
        || w.class.to_lowercase().contains(q)
        || apps::name_for_class(&w.class).to_lowercase().contains(q)
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Query(q) => app.panels.overview.query = q,
        Message::Submit => {
            let q = app.panels.overview.query.trim().to_lowercase();
            if let Some(w) = app
                .hypr
                .windows
                .iter()
                .filter(|w| matches(w, &q))
                .min_by_key(|w| w.focus)
            {
                let lua = hyprland::focus_window(&w.address);
                return Task::batch([
                    Task::done(AppMessage::Dispatch(lua)),
                    Task::done(AppMessage::Close),
                ]);
            }
        }
    }
    Task::none()
}

fn app_icon<'a>(class: &str, size: f32, s: Scheme) -> Element<'a, AppMessage> {
    match icons::lookup(&apps::icon_for_class(class)) {
        Some(p) => ui::image_file(&p, size),
        None => icon(Icon::AppWindow, size * 0.8, s.ink_dim).into(),
    }
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.overview;
    let q = st.query.trim().to_lowercase();
    let input = text_input("Find a window", &st.query)
        .id(INPUT_ID.clone())
        .on_input(|q| msg(Message::Query(q)))
        .on_submit(msg(Message::Submit))
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

    let count = crate::shell::bar::slot_count(&app.hypr, app.config.workspaces) as i32;
    let active = app.hypr.active_on(&app.hypr.focused_monitor);
    let card_w = (WIDTH - 24.0 - (COLUMNS as f32 - 1.0) * 12.0) / COLUMNS as f32;
    let cards: Vec<Element<'_, AppMessage>> = (1..=count)
        .map(|id| workspace_card(app, s, id, active == Some(id), &q, card_w))
        .collect();
    let mut grid = column![].spacing(12);
    let mut it = cards.into_iter().peekable();
    while it.peek().is_some() {
        let mut r = row![].spacing(12);
        for _ in 0..COLUMNS {
            r = r.push(
                it.next()
                    .unwrap_or_else(|| Space::new().width(card_w).into()),
            );
        }
        grid = grid.push(r);
    }
    container(column![field, grid].spacing(12))
        .padding(12)
        .into()
}

fn workspace_card<'a>(
    app: &'a App,
    s: Scheme,
    id: i32,
    current: bool,
    q: &str,
    width: f32,
) -> Element<'a, AppMessage> {
    let windows: Vec<&hyprland::Window> = app
        .hypr
        .windows
        .iter()
        .filter(|w| w.workspace == id && matches(w, q))
        .collect();
    let header = control(
        row![
            label_w(
                id.to_string(),
                text::BODY,
                if current { s.accent } else { s.ink },
                theme::SEMIBOLD
            ),
            Space::new().width(Length::Fill),
            label(
                match windows.len() {
                    0 => "Empty".to_string(),
                    1 => "1 window".to_string(),
                    n => format!("{n} windows"),
                },
                text::CAPTION,
                s.ink_faint
            ),
        ]
        .align_y(Alignment::Center),
    )
    .padding([0, 10])
    .height(30)
    .width(Length::Fill)
    .radius(size::ROW_RADIUS)
    .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
    .no_press_scale()
    .on_press(AppMessage::Batch(vec![
        AppMessage::Dispatch(hyprland::focus_workspace(id)),
        AppMessage::Close,
    ]));
    let mut col = column![header].spacing(2);
    for w in windows.iter().take(6) {
        let name = apps::name_for_class(&w.class);
        let title = if w.title.is_empty() {
            name.clone()
        } else {
            w.title.clone()
        };
        col = col.push(
            control(
                row![
                    app_icon(&w.class, 20.0, s),
                    column![
                        label(
                            ui::fit(&title, text::SMALL, width - 70.0),
                            text::SMALL,
                            s.ink
                        ),
                        label(
                            ui::fit(&name, text::CAPTION, width - 70.0),
                            text::CAPTION,
                            s.ink_faint
                        ),
                    ]
                    .spacing(1),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([0, 10])
            .height(44)
            .width(Length::Fill)
            .radius(size::ROW_RADIUS)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .no_press_scale()
            .on_press(AppMessage::Batch(vec![
                AppMessage::Dispatch(hyprland::focus_window(&w.address)),
                AppMessage::Close,
            ])),
        );
    }
    if windows.len() > 6 {
        col = col.push(
            container(label(
                format!("+{} more", windows.len() - 6),
                text::CAPTION,
                s.ink_faint,
            ))
            .padding([4, 10]),
        );
    }
    container(col.height(Length::Fixed(30.0 + 2.0 + 6.0 * 46.0)))
        .padding(6)
        .width(width)
        .style(move |_| container::Style {
            background: Some(if current { s.raised_hi } else { s.raised }.into()),
            border: iced::Border::default().rounded(size::CARD_RADIUS),
            ..container::Style::default()
        })
        .into()
}
