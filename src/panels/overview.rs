//! Overview (Super+Tab): every workspace as a small picture of its screen, each window drawn
//! where it really is with its app's icon. Type to find a window (the rest dim); Tab or the
//! arrows move between windows; Enter goes to the selected one; a click focuses a window, or
//! a workspace when it lands on the background.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{apps, hyprland, icons};
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, pin, row, stack, text_input};
use iced::{Alignment, Color, Element, Task};

/// All workspaces in one compact row.
const COLUMNS: usize = 5;
const GAP: f32 = 14.0;
pub const WIDTH: f32 = 1300.0;
const PAD: f32 = 16.0;

#[derive(Default)]
pub struct State {
    query: String,
    /// Address of the selected window.
    selected: Option<String>,
    /// Live pictures of the windows, by address, ready at their tile's size (kept between
    /// opens so the overview shows them at once; refreshed on each open, off the UI thread).
    previews: std::collections::HashMap<String, Ready>,
}

/// A window's picture, rounded at a tile's size.
struct Ready {
    size: (u32, u32),
    handle: iced::widget::image::Handle,
}

/// (address, tile size, picture width, height, RGBA): made off the UI thread.
type Made = (String, (u32, u32), u32, u32, Vec<u8>);

#[derive(Clone, Debug)]
pub enum Message {
    Previews(Vec<Made>),
    Query(String),
    Submit,
}

static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("overview-input"));

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Overview(m))
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    let st = &mut app.panels.overview;
    st.query.clear();
    // Start on the window you were in.
    st.selected = app
        .hypr
        .windows
        .iter()
        .min_by_key(|w| w.focus)
        .map(|w| w.address.clone());
    Task::batch([
        Task::Iced(iced::widget::focus(INPUT_ID.clone())),
        // Fresh pictures of every window: captured, scaled and rounded to their tiles off the
        // UI thread, so opening never stutters.
        Task::perform(
            {
                let (cw, ch) = card_size();
                let jobs: Vec<(String, (f32, f32))> = app
                    .hypr
                    .windows
                    .iter()
                    .map(|w| {
                        let (_, _, bw, bh) = tile_rect(w, cw, ch);
                        (w.address.clone(), (bw, bh))
                    })
                    .collect();
                async move {
                    tokio::task::spawn_blocking(move || make_previews(&jobs))
                        .await
                        .unwrap_or_default()
                }
            },
            |v| msg(Message::Previews(v)),
        ),
        // Positions change without an event (resizing); read them fresh.
        Task::perform(hyprland::snapshot(), |s| match s {
            Some(s) => AppMessage::Hypr(s),
            None => AppMessage::Batch(Vec::new()),
        }),
    ])
}

fn matches(w: &hyprland::Window, q: &str) -> bool {
    q.is_empty()
        || w.title.to_lowercase().contains(q)
        || w.class.to_lowercase().contains(q)
        || apps::name_for_class(&w.class).to_lowercase().contains(q)
}

fn make_previews(jobs: &[(String, (f32, f32))]) -> Vec<Made> {
    let addrs: Vec<String> = jobs.iter().map(|(a, _)| a.clone()).collect();
    let widest = jobs.iter().map(|(_, (w, _))| *w).fold(0.0, f32::max);
    crate::services::previews::capture(&addrs, (widest * 2.0).max(64.0) as u32)
        .into_iter()
        .filter_map(|(a, p)| {
            let p = p?;
            let (bw, bh) = jobs.iter().find(|(j, _)| *j == a)?.1;
            let img = image::RgbaImage::from_raw(p.width, p.height, p.rgba)?;
            let r = ui::rounded_cover(&img, bw, bh, 7.0);
            Some((
                a,
                (bw as u32, bh as u32),
                r.width(),
                r.height(),
                r.into_raw(),
            ))
        })
        .collect()
}

/// Where a window sits on its card (inset, so a selected window's ring never meets the
/// card's rounded edge).
fn tile_rect(w: &hyprland::Window, cw: f32, ch: f32) -> (f32, f32, f32, f32) {
    const INSET: f32 = 5.0;
    let (sw, _) = w.screen;
    let k = (cw - 2.0 * INSET) / sw.max(1.0);
    let (x, y) = (INSET + (w.x * k).max(0.0), INSET + (w.y * k).max(0.0));
    (
        x,
        y,
        (w.w * k).clamp(16.0, cw - INSET - x),
        (w.h * k).clamp(12.0, ch - INSET - y),
    )
}

/// A workspace card's picture size (for a 16:9 screen).
pub fn card_size() -> (f32, f32) {
    let w = (WIDTH - 2.0 * PAD - (COLUMNS as f32 - 1.0) * GAP) / COLUMNS as f32;
    (w, w * 9.0 / 16.0)
}

fn workspace_ids(app: &App) -> Vec<i32> {
    let count = crate::shell::bar::slot_count(&app.hypr, app.config.workspaces) as i32;
    (1..=count).collect()
}

/// Windows of a workspace in reading order: tiled top-left first, floating ones last.
fn windows_on(app: &App, id: i32) -> Vec<&hyprland::Window> {
    let mut v: Vec<&hyprland::Window> = app
        .hypr
        .windows
        .iter()
        .filter(|w| w.workspace == id)
        .collect();
    v.sort_by(|a, b| {
        (a.floating, a.y as i32 / 40, a.x as i32).cmp(&(b.floating, b.y as i32 / 40, b.x as i32))
    });
    v
}

/// Every matching window in on-screen order, with its card index.
fn order(app: &App) -> Vec<(usize, &hyprland::Window)> {
    let q = app.panels.overview.query.trim().to_lowercase();
    workspace_ids(app)
        .into_iter()
        .enumerate()
        .flat_map(|(card, id)| windows_on(app, id).into_iter().map(move |w| (card, w)))
        .filter(|(_, w)| matches(w, &q))
        .collect()
}

fn focus(address: &str) -> Task<AppMessage> {
    Task::batch([
        Task::done(AppMessage::Dispatch(hyprland::focus_window(address))),
        Task::done(AppMessage::Close),
    ])
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Previews(v) => {
            let st = &mut app.panels.overview;
            for (a, size, w, h, rgba) in v {
                st.previews.insert(
                    a,
                    Ready {
                        size,
                        handle: iced::widget::image::Handle::from_rgba(w, h, rgba),
                    },
                );
            }
            let live: std::collections::HashSet<&str> = app
                .hypr
                .windows
                .iter()
                .map(|w| w.address.as_str())
                .collect();
            app.panels
                .overview
                .previews
                .retain(|a, _| live.contains(a.as_str()));
        }
        Message::Query(q) => {
            app.panels.overview.query = q;
            // Keep the selection on a match.
            let list = order(app);
            let st = &app.panels.overview;
            let still = st
                .selected
                .as_ref()
                .is_some_and(|a| list.iter().any(|(_, w)| &w.address == a));
            if !still {
                let first = list.first().map(|(_, w)| w.address.clone());
                app.panels.overview.selected = first;
            }
        }
        Message::Submit => {
            let list = order(app);
            let st = &app.panels.overview;
            let target = st
                .selected
                .as_ref()
                .filter(|a| list.iter().any(|(_, w)| &w.address == *a))
                .cloned()
                .or_else(|| list.first().map(|(_, w)| w.address.clone()));
            if let Some(a) = target {
                return focus(&a);
            }
        }
    }
    Task::none()
}

pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    let list = order(app);
    if list.is_empty() {
        return Task::none();
    }
    let cur = app
        .panels
        .overview
        .selected
        .as_ref()
        .and_then(|a| list.iter().position(|(_, w)| &w.address == a))
        .unwrap_or(0);
    let n = list.len();
    let next = match k {
        Key::Right | Key::Tab => (cur + 1) % n,
        Key::Left | Key::BackTab => (cur + n - 1) % n,
        Key::Down | Key::Up => {
            // The first window of the nearest card in that direction.
            let card = list[cur].0 as i64;
            let step = if k == Key::Down {
                COLUMNS as i64
            } else {
                -(COLUMNS as i64)
            };
            let mut target = card + step;
            let last = list.last().map_or(0, |(c, _)| *c) as i64;
            let mut found = None;
            while (0..=last).contains(&target) && found.is_none() {
                found = list.iter().position(|(c, _)| *c as i64 == target);
                target += step.signum();
            }
            found.unwrap_or(cur)
        }
        Key::Home => 0,
        Key::End => n - 1,
        _ => return Task::none(),
    };
    app.panels.overview.selected = Some(list[next].1.address.clone());
    Task::none()
}

fn app_icon<'a>(class: &str, px: f32, s: Scheme) -> Element<'a, AppMessage> {
    match icons::lookup(&apps::icon_for_class(class)) {
        Some(p) => ui::image_file_faded(&p, px, s.opacity),
        None => icon(Icon::AppWindow, px * 0.8, s.ink_dim).into(),
    }
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.overview;
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
        row![
            icon(Icon::Search, 18.0, s.ink_dim),
            input,
            label("Tab  ↑↓←→  Enter", text::CAPTION, s.ink_faint),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .height(44)
    .center_y(44)
    .style(move |_| container::Style {
        background: Some(s.ink.scale_alpha(0.06 * s.opacity).into()),
        border: iced::Border::default().rounded(22),
        ..container::Style::default()
    });

    let card_w = (WIDTH - 2.0 * PAD - (COLUMNS as f32 - 1.0) * GAP) / COLUMNS as f32;
    let active = app.hypr.active_on(&app.hypr.focused_monitor);
    let q = st.query.trim().to_lowercase();
    let cards: Vec<Element<'_, AppMessage>> = workspace_ids(app)
        .into_iter()
        .map(|id| card(app, s, id, active == Some(id), &q, card_w))
        .collect();
    let mut grid = column![].spacing(GAP);
    let mut it = cards.into_iter().peekable();
    while it.peek().is_some() {
        let mut r = row![].spacing(GAP);
        for _ in 0..COLUMNS {
            r = r.push(
                it.next()
                    .unwrap_or_else(|| Space::new().width(card_w).into()),
            );
        }
        grid = grid.push(r);
    }
    container(column![field, grid].spacing(PAD))
        .padding(PAD)
        .into()
}

fn card<'a>(
    app: &'a App,
    s: Scheme,
    id: i32,
    current: bool,
    q: &str,
    width: f32,
) -> Element<'a, AppMessage> {
    let windows = windows_on(app, id);
    let selected = app.panels.overview.selected.as_deref();
    let (_, height) = card_size();

    let backdrop: Element<'_, AppMessage> = match &app.wallpaper {
        Some(w) => iced::widget::image(w.thumb_soft.clone())
            .width(width)
            .height(height)
            .content_fit(iced::ContentFit::Fill)
            .opacity(s.opacity)
            .into(),
        None => container(Space::new())
            .width(width)
            .height(height)
            .style(move |_| container::Style {
                background: Some(s.raised.into()),
                border: iced::Border::default().rounded(10),
                ..container::Style::default()
            })
            .into(),
    };
    // Clicking the background switches to the workspace.
    let mut layers = stack![
        control(backdrop)
            .width(width)
            .height(height)
            .radius(10.0)
            .no_press_scale()
            .on_press(AppMessage::Batch(vec![
                AppMessage::Dispatch(hyprland::focus_workspace(id)),
                AppMessage::Close,
            ]))
    ]
    .width(width)
    .height(height);
    for w in &windows {
        let (x, y, bw, bh) = tile_rect(w, width, height);
        let hit = matches(w, q);
        let sel = selected == Some(w.address.as_str());
        // No borders or rings (design call): the selected window at full strength, the others
        // back, the ones a search does not match far back.
        let alpha = if !hit {
            0.25
        } else if selected.is_some() && !sel {
            0.5
        } else {
            1.0
        };
        let ready = app
            .panels
            .overview
            .previews
            .get(&w.address)
            .filter(|r| r.size == (bw as u32, bh as u32));
        let body: Element<'_, AppMessage> = match ready {
            Some(r) => iced::widget::image(r.handle.clone())
                .width(bw)
                .height(bh)
                .opacity(alpha * s.opacity)
                .into(),
            // No picture yet: the app's icon on a quiet card.
            None => container(app_icon(&w.class, (bw.min(bh) * 0.4).clamp(14.0, 28.0), s))
                .center_x(bw)
                .center_y(bh)
                .style(move |_| container::Style {
                    background: Some(s.raised.scale_alpha(0.9 * alpha).into()),
                    border: iced::Border::default().rounded(7),
                    ..container::Style::default()
                })
                .into(),
        };
        let tile = stack![body];
        let tile = control(tile)
            .width(bw)
            .height(bh)
            .radius(7.0)
            .colors(Color::TRANSPARENT, s.ink.scale_alpha(0.08), s.raised_press)
            .no_press_scale()
            .on_press(focus_msg(&w.address));
        layers = layers.push(pin(tile).x(x).y(y));
    }
    // Under the card: its number (bright when it is the one you are on) and what is there.
    let summary = match windows.len() {
        0 => String::new(),
        1 => apps::name_for_class(&windows[0].class),
        n => format!("{n} windows"),
    };
    let caption = row![
        label_w(
            id.to_string(),
            text::SMALL,
            if current { s.ink } else { s.ink_dim },
            theme::SEMIBOLD
        ),
        label(
            ui::fit(&summary, text::CAPTION, width - 30.0),
            text::CAPTION,
            s.ink_dim
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([0, 2]);
    column![layers, caption].spacing(8).width(width).into()
}

fn focus_msg(address: &str) -> AppMessage {
    AppMessage::Batch(vec![
        AppMessage::Dispatch(hyprland::focus_window(address)),
        AppMessage::Close,
    ])
}
