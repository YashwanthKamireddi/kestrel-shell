//! Overview (Super+Tab): every workspace as a small picture of its screen, each window drawn
//! where it really is with its app's icon. Type to find a window (the rest dim); Tab or the
//! arrows move between windows; Enter goes to the selected one; a click focuses a window, or
//! a workspace when it lands on the background.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{apps, hyprland, icons};
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, pin, row, stack, text_input};
use iced::{Alignment, Color, Element, Length, Task};

const COLUMNS: usize = 3;
const GAP: f32 = 16.0;
pub const WIDTH: f32 = 1040.0;
const PAD: f32 = 16.0;

#[derive(Default)]
pub struct State {
    query: String,
    /// Address of the selected window.
    selected: Option<String>,
    /// Live pictures of the windows, by address (kept between opens, refreshed on each).
    previews: std::collections::HashMap<String, crate::services::previews::Preview>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Previews(Vec<(String, Option<crate::services::previews::Preview>)>),
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
        // Fresh pictures of every window, made off the UI thread.
        Task::perform(
            {
                let addrs: Vec<String> =
                    app.hypr.windows.iter().map(|w| w.address.clone()).collect();
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::services::previews::capture(&addrs, 640)
                    })
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
            let live: std::collections::HashSet<&str> = app
                .hypr
                .windows
                .iter()
                .map(|w| w.address.as_str())
                .collect();
            let st = &mut app.panels.overview;
            st.previews.retain(|a, _| live.contains(a.as_str()));
            for (a, p) in v {
                if let Some(p) = p {
                    st.previews.insert(a, p);
                }
            }
            preview_cache_clear();
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
        background: Some(s.raised.into()),
        border: iced::Border::default().rounded(size::CARD_RADIUS),
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
    // The picture has the screen's shape.
    let (sw, sh) = windows.first().map_or((1920.0, 1080.0), |w| w.screen);
    let height = width * sh / sw.max(1.0);
    // Windows are drawn inside an inset, so a selected window's outline never meets the
    // card's rounded edge (where it would be cut).
    const INSET: f32 = 6.0;
    let k = (width - 2.0 * INSET) / sw.max(1.0);

    let summary = match windows.len() {
        0 => "Empty".to_string(),
        1 => apps::name_for_class(&windows[0].class),
        n => format!("{n} windows"),
    };
    let header = row![
        label_w(
            id.to_string(),
            text::BODY,
            if current { s.accent } else { s.ink },
            theme::SEMIBOLD
        ),
        Space::new().width(Length::Fill),
        label(summary, text::CAPTION, s.ink_faint),
    ]
    .align_y(Alignment::Center)
    .padding([0, 4]);

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
        let (x, y) = (INSET + (w.x * k).max(0.0), INSET + (w.y * k).max(0.0));
        let (bw, bh) = (
            (w.w * k).clamp(16.0, width - INSET - x),
            (w.h * k).clamp(12.0, height - INSET - y),
        );
        let hit = matches(w, q);
        let sel = selected == Some(w.address.as_str());
        let alpha = if hit { 1.0 } else { 0.3 };
        let body: Element<'_, AppMessage> = match preview(app, &w.address, bw, bh) {
            // The window itself, with its app's icon as a small badge.
            Some(h) => stack![
                iced::widget::image(h)
                    .width(bw)
                    .height(bh)
                    .opacity(alpha * s.opacity),
                container(app_icon(&w.class, 16.0, s))
                    .padding(6)
                    .width(bw)
                    .height(bh)
                    .align_x(Alignment::Start)
                    .align_y(Alignment::End),
            ]
            .into(),
            // No picture yet: the icon (and title when there is room) on a card.
            None => {
                let px = (bw.min(bh) * 0.42).clamp(14.0, 36.0);
                let mut inner = column![app_icon(&w.class, px, s)]
                    .spacing(4)
                    .align_x(Alignment::Center);
                if bh >= 64.0 && bw >= 96.0 {
                    let title = if w.title.is_empty() {
                        apps::name_for_class(&w.class)
                    } else {
                        w.title.clone()
                    };
                    inner = inner.push(label(
                        ui::fit(&title, text::CAPTION, bw - 16.0),
                        text::CAPTION,
                        s.ink_dim,
                    ));
                }
                container(inner)
                    .center_x(bw)
                    .center_y(bh)
                    .style(move |_| container::Style {
                        background: Some(s.raised.scale_alpha(0.94 * alpha).into()),
                        border: iced::Border::default().rounded(8),
                        ..container::Style::default()
                    })
                    .into()
            }
        };
        // Selection: an accent ring drawn over the picture.
        let ring = container(Space::new())
            .width(bw)
            .height(bh)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: if sel {
                        s.accent
                    } else {
                        s.ink.scale_alpha(0.10)
                    },
                    width: if sel { 2.5 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            });
        let tile = control(stack![body, ring])
            .width(bw)
            .height(bh)
            .radius(8.0)
            .colors(Color::TRANSPARENT, s.ink.scale_alpha(0.06), s.raised_press)
            .no_press_scale()
            .on_press(focus_msg(&w.address));
        layers = layers.push(pin(tile).x(x).y(y));
    }
    // An empty workspace: its number, large, on the calm backdrop.
    if windows.is_empty() {
        layers = layers.push(
            container(
                column![
                    label_w(
                        id.to_string(),
                        theme::text::DISPLAY,
                        if current { s.accent } else { s.ink },
                        theme::SEMIBOLD
                    ),
                    label("Empty", text::CAPTION, s.ink_dim),
                ]
                .spacing(2)
                .align_x(Alignment::Center),
            )
            .center_x(width)
            .center_y(height),
        );
    }
    // The current workspace's outline sits on top, where the picture can't cover it.
    if current {
        layers = layers.push(container(Space::new()).width(width).height(height).style(
            move |_| container::Style {
                border: iced::Border {
                    color: s.accent,
                    width: 2.0,
                    radius: 10.0.into(),
                },
                ..container::Style::default()
            },
        ));
    }
    let frame = layers;
    column![header, frame].spacing(8).width(width).into()
}

/// A window's picture at a tile's size, corners rounded; made once per size.
fn preview(app: &App, address: &str, w: f32, h: f32) -> Option<iced::widget::image::Handle> {
    let key = (address.to_string(), w as u32, h as u32);
    if let Some(hit) = PREVIEWS.lock().ok()?.get(&key) {
        return Some(hit.clone());
    }
    let p = app.panels.overview.previews.get(address)?;
    let img = image::RgbaImage::from_raw(p.width, p.height, p.rgba.clone())?;
    let r = ui::rounded_cover(&img, w, h, 8.0);
    let handle = iced::widget::image::Handle::from_rgba(r.width(), r.height(), r.into_raw());
    PREVIEWS.lock().ok()?.insert(key, handle.clone());
    Some(handle)
}

type PreviewKey = (String, u32, u32);
static PREVIEWS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<PreviewKey, iced::widget::image::Handle>>,
> = std::sync::LazyLock::new(Default::default);

fn preview_cache_clear() {
    if let Ok(mut c) = PREVIEWS.lock() {
        c.clear();
    }
}

fn focus_msg(address: &str) -> AppMessage {
    AppMessage::Batch(vec![
        AppMessage::Dispatch(hyprland::focus_window(address)),
        AppMessage::Close,
    ])
}
