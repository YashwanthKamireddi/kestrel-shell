//! Overview (Super+Tab): where everything is open. The workspaces that have windows (and the
//! one you are on), each a plain card with its windows drawn where they are, as live
//! pictures; the panel is only as wide as its cards (no search, no title line: design call).
//! Nothing is marked until you choose: a thin ring shows the window under the pointer (gone
//! when it leaves) or the one Tab and the arrows reached; Enter or a click goes there. Nothing fades or glides.

use super::Message as PanelMessage;
use super::PanelKind;
use crate::app::{App, Message as AppMessage};
use crate::services::{apps, hyprland};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, label, label_w};
use iced::widget::{Space, column, container, pin, row, stack};
use iced::{Alignment, Element, Task};

/// All workspaces in one compact row.
const COLUMNS: usize = 5;
const GAP: f32 = 14.0;
pub const WIDTH: f32 = 1300.0;
const PAD: f32 = 16.0;
/// The line under each card (number and what is there), a fixed height so the panel's height
/// is known before it is drawn.
const CAPTION: f32 = 20.0;
/// Between a card and its caption.
const CAPTION_GAP: f32 = 8.0;

#[derive(Default)]
pub struct State {
    /// Address of the selected window.
    selected: Option<String>,
    /// Waiting to open (until its fresh pictures are in): its number, screen and place.
    pending: Option<(u64, iced::OutputId, iced::Rectangle)>,
    token: u64,
    /// Live pictures of the windows, by address, ready at their tile's size (kept between
    /// opens so the overview shows them at once; refreshed on each open, off the UI thread).
    previews: std::collections::HashMap<String, Ready>,
}

/// A window's picture, rounded at a tile's size.
struct Ready {
    size: (u32, u32),
    handle: iced::widget::image::Handle,
}

#[derive(Clone, Debug)]
pub enum Message {
    Submit,
    /// The pointer came onto a window: it becomes the selection.
    Hover(String),
    /// The pointer left a window: it is no longer marked.
    Unhover(String),
    /// New pictures of windows that changed, from the live stream while open.
    Live(Vec<crate::services::previews::Frame>),
    /// The pictures taken before opening (`prepare`): shown, then the overview opens.
    Fresh(u64, Vec<crate::services::previews::Frame>),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Overview(m))
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    let st = &mut app.panels.overview;
    // Nothing marked until the pointer or a key chooses (a ring on the current window
    // before you did anything read as a heavy border: design call).
    st.selected = None;
    Task::batch([
        // The pictures were taken just before it opened (`prepare`); the live stream keeps
        // them moving. Positions change without an event (resizing): read them fresh.
        Task::perform(hyprland::snapshot(), |s| match s {
            Some(s) => AppMessage::Hypr(s),
            None => AppMessage::Batch(Vec::new()),
        }),
    ])
}

/// Fresh pictures of every window: captured, scaled and rounded to their tiles off the UI
/// thread. Taken when the overview opens and in the background whenever the set of windows
/// changes (so the first open has pictures, not icons); while it is open, `live_jobs` keeps
/// each picture moving with its window.
/// Opening: every window's picture is taken first (all at once, about a frame), then the
/// overview shows them, current, never an old picture changing in front of you. If the
/// compositor is slow, it opens anyway after `WAIT` with what it has.
pub fn prepare(app: &mut App, output: iced::OutputId, at: iced::Rectangle) -> Task<AppMessage> {
    const WAIT: std::time::Duration = std::time::Duration::from_millis(90);
    // Every window has a picture already (taken when it opened, kept fresh while shown): open at
    // once and refresh them behind it. Waiting for fresh ones every time put ~40 ms between the
    // key and the overview (the user's "delay").
    let complete = app
        .hypr
        .windows
        .iter()
        .filter(|w| w.workspace > 0)
        .all(|w| app.panels.overview.previews.contains_key(&w.address));
    if complete {
        let open = app.open_panel(PanelKind::Overview, output, at);
        let jobs = live_jobs(app);
        let fresh = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    crate::services::previews::snapshot(&jobs, WAIT)
                })
                .await
                .unwrap_or_default()
            },
            |frames| msg(Message::Live(frames)),
        );
        return Task::batch([open, fresh]);
    }
    let st = &mut app.panels.overview;
    st.token += 1;
    let token = st.token;
    st.pending = Some((token, output, at));
    let jobs = live_jobs(app);
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || crate::services::previews::snapshot(&jobs, WAIT))
                .await
                .unwrap_or_default()
        },
        move |frames| msg(Message::Fresh(token, frames)),
    )
}

/// Pictures for windows that have none at their tile's size yet (a window opened or was
/// resized), taken in the background while the overview is closed: the others keep theirs,
/// so opening one window no longer copies every window on the desktop.
pub fn refresh_missing(app: &App) -> Task<AppMessage> {
    let have = &app.panels.overview.previews;
    snapshot(
        live_jobs(app)
            .into_iter()
            .filter(|j| {
                have.get(&j.address)
                    .is_none_or(|r| r.size != (j.width.max(1), j.height.max(1)))
            })
            .collect(),
    )
}

fn snapshot(jobs: Vec<crate::services::previews::Job>) -> Task<AppMessage> {
    if jobs.is_empty() {
        return Task::none();
    }
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                crate::services::previews::snapshot(&jobs, std::time::Duration::from_millis(250))
            })
            .await
            .unwrap_or_default()
        },
        |v| msg(Message::Live(v)),
    )
}

/// The windows to keep live while the overview is open, each at its tile's size.
pub fn live_jobs(app: &App) -> Vec<crate::services::previews::Job> {
    let (cw, ch) = card_size();
    app.hypr
        .windows
        .iter()
        // Only windows the overview draws (special workspaces are never shown).
        .filter(|w| w.workspace > 0)
        .map(|w| {
            let (_, _, bw, bh) = tile_rect(w, cw, ch);
            crate::services::previews::Job {
                address: w.address.clone(),
                width: bw as u32,
                height: bh as u32,
                radius: 7,
            }
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

/// The workspaces shown: those with windows, and the one you are on (empty ones are left
/// out: a row of blank cards was noise).
fn workspace_ids(app: &App) -> Vec<i32> {
    let current = app.hypr.active_on(&app.hypr.focused_monitor);
    let mut ids: Vec<i32> = app
        .hypr
        .windows
        .iter()
        .map(|w| w.workspace)
        .filter(|id| *id > 0)
        .chain(current)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Windows of a workspace as they are seen: tiled top-left first, floating ones last; a
/// fullscreen or maximized window covers the rest, so it is the only one (drawing the ones
/// behind it over it cut it off).
fn windows_on(app: &App, id: i32) -> Vec<&hyprland::Window> {
    let mut v: Vec<&hyprland::Window> = app
        .hypr
        .windows
        .iter()
        .filter(|w| w.workspace == id)
        .collect();
    if let Some(top) = v
        .iter()
        .filter(|w| w.fullscreen > 0)
        .min_by_key(|w| (std::cmp::Reverse(w.fullscreen), w.focus))
    {
        return vec![*top];
    }
    v.sort_by(|a, b| {
        (a.floating, a.y as i32 / 40, a.x as i32).cmp(&(b.floating, b.y as i32 / 40, b.x as i32))
    });
    v
}

/// Every matching window in on-screen order, with its card index.
fn order(app: &App) -> Vec<(usize, &hyprland::Window)> {
    workspace_ids(app)
        .into_iter()
        .enumerate()
        .flat_map(|(card, id)| windows_on(app, id).into_iter().map(move |w| (card, w)))
        .collect()
}

/// The window you were in.
fn focused(app: &App) -> Option<String> {
    app.hypr
        .windows
        .iter()
        .min_by_key(|w| w.focus)
        .map(|w| w.address.clone())
}

fn focus(address: &str) -> Task<AppMessage> {
    Task::batch([
        Task::done(AppMessage::Dispatch(
            crate::services::compositor::Action::FocusWindow(String::from(address)),
        )),
        Task::done(AppMessage::Close),
    ])
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Fresh(token, frames) => {
            let _ = update(app, Message::Live(frames));
            if let Some((t, output, at)) = app.panels.overview.pending
                && t == token
            {
                app.panels.overview.pending = None;
                return app.open_panel(PanelKind::Overview, output, at);
            }
        }
        Message::Live(frames) => {
            let st = &mut app.panels.overview;
            for f in frames {
                st.previews.insert(
                    f.address,
                    Ready {
                        size: (f.width, f.height),
                        handle: iced::widget::image::Handle::from_rgba(f.width, f.height, f.rgba),
                    },
                );
            }
            // Pictures of windows that have closed go.
            let open: std::collections::HashSet<&str> = app
                .hypr
                .windows
                .iter()
                .map(|w| w.address.as_str())
                .collect();
            app.panels
                .overview
                .previews
                .retain(|a, _| open.contains(a.as_str()));
        }
        Message::Hover(a) => app.panels.overview.selected = Some(a),
        Message::Unhover(a) => {
            let st = &mut app.panels.overview;
            if st.selected.as_deref() == Some(a.as_str()) {
                st.selected = None;
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
                .or_else(|| focused(app))
                .or_else(|| list.first().map(|(_, w)| w.address.clone()));
            if let Some(a) = target {
                return focus(&a);
            }
        }
    }
    Task::none()
}

/// The panel's height, worked out from its rows (not measured after drawing): opening after
/// the number of rows changed showed the old height for a frame or two, a tall empty frame
/// snapping shut.
pub fn height(app: &App) -> f32 {
    let (_, ch) = card_size();
    let rows = workspace_ids(app).len().div_ceil(COLUMNS).max(1) as f32;
    2.0 * PAD + rows * (ch + CAPTION_GAP + CAPTION) + (rows - 1.0) * GAP
}

/// The panel's width: its cards side by side (up to a row of five).
pub fn width(app: &App) -> f32 {
    let (cw, _) = card_size();
    let n = workspace_ids(app).len().clamp(1, COLUMNS) as f32;
    2.0 * PAD + n * cw + (n - 1.0) * GAP
}

pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    if k == Key::Enter {
        return update(app, Message::Submit);
    }
    let list = order(app);
    if list.is_empty() {
        return Task::none();
    }
    let cur = app
        .panels
        .overview
        .selected
        .clone()
        .or_else(|| focused(app))
        .and_then(|a| list.iter().position(|(_, w)| w.address == a));
    // Every key moves, starting from the window you are in (the first press used to only mark
    // it: a press that did nothing).
    let Some(cur) = cur else {
        app.panels.overview.selected = list.first().map(|(_, w)| w.address.clone());
        return Task::none();
    };
    let n = list.len();
    let next = match k {
        Key::Right | Key::Tab => (cur + 1) % n,
        Key::Left | Key::BackTab => (cur + n - 1) % n,
        Key::Down | Key::Up => {
            // First a window above or below in the same workspace (the nearest one that way),
            // then the first window of the nearest card in that direction.
            let (card_now, w) = list[cur];
            let down = k == Key::Down;
            let mid = w.x + w.w / 2.0;
            let same = list
                .iter()
                .enumerate()
                .filter(|(_, (c, o))| {
                    *c == card_now
                        && if down {
                            o.y >= w.y + w.h - 1.0
                        } else {
                            o.y + o.h <= w.y + 1.0
                        }
                })
                .min_by(|(_, (_, a)), (_, (_, b))| {
                    let d =
                        |o: &&hyprland::Window| (o.y - w.y).abs() + (o.x + o.w / 2.0 - mid).abs();
                    d(a).total_cmp(&d(b))
                })
                .map(|(i, _)| i);
            if let Some(i) = same {
                app.panels.overview.selected = Some(list[i].1.address.clone());
                return Task::none();
            }
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
    ui::app_icon(s, class, px, Icon::AppWindow)
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let card_w = (WIDTH - 2.0 * PAD - (COLUMNS as f32 - 1.0) * GAP) / COLUMNS as f32;
    let active = app.hypr.active_on(&app.hypr.focused_monitor);
    let cards: Vec<Element<'_, AppMessage>> = workspace_ids(app)
        .into_iter()
        .map(|id| card(app, s, id, active == Some(id), card_w))
        .collect();
    let mut grid = column![].spacing(GAP);
    let mut it = cards.into_iter().peekable();
    while it.peek().is_some() {
        let mut r = row![].spacing(GAP);
        // A short last row stays short (no blank cards pad it out).
        for _ in 0..COLUMNS {
            if let Some(c) = it.next() {
                r = r.push(c);
            }
        }
        grid = grid.push(r);
    }
    container(grid).padding(PAD).into()
}

fn card<'a>(
    app: &'a App,
    s: Scheme,
    id: i32,
    current: bool,
    width: f32,
) -> Element<'a, AppMessage> {
    let windows = windows_on(app, id);
    let selected = app.panels.overview.selected.as_deref();
    let (_, height) = card_size();

    // A plain card (the blurred wallpaper behind each one was the heaviest thing drawn and
    // only added noise); a click on it switches to the workspace.
    let mut layers = stack![
        control(Space::new().width(width).height(height))
            .width(width)
            .height(height)
            .radius(tok::R_CARD)
            .kind(
                s,
                if current {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Tonal
                }
            )
            .no_press_scale()
            .on_press(AppMessage::Batch(vec![
                AppMessage::Dispatch(crate::services::compositor::Action::FocusWorkspace(id)),
                AppMessage::Close,
            ]))
    ]
    .width(width)
    .height(height);
    for w in &windows {
        let (x, y, bw, bh) = tile_rect(w, width, height);
        let sel = selected == Some(w.address.as_str());
        let alpha = 1.0;
        // The window's last picture, even if its tile changed size since (scaled to fit until
        // the fresh one arrives): an icon flashed for a second before.
        let ready = app.panels.overview.previews.get(&w.address);
        let exact = ready.is_some_and(|r| r.size == (bw as u32, bh as u32));
        let body: Element<'_, AppMessage> = match ready {
            Some(r) => iced::widget::image(r.handle.clone())
                .width(bw)
                .height(bh)
                .content_fit(iced::ContentFit::Fill)
                .filter_method(if exact {
                    iced::widget::image::FilterMethod::Nearest
                } else {
                    iced::widget::image::FilterMethod::Linear
                })
                .opacity(alpha * s.opacity)
                .into(),
            // No picture yet: the app's icon on a quiet tile.
            None => container(app_icon(&w.class, (bw.min(bh) * 0.4).clamp(14.0, 28.0), s))
                .center_x(bw)
                .center_y(bh)
                .style(move |_| container::Style {
                    background: Some(s.ink.scale_alpha(0.08 * alpha * s.opacity).into()),
                    border: iced::Border::default().rounded(7),
                    ..container::Style::default()
                })
                .into(),
        };
        let mut tile = stack![body];
        // The selection: a thin quiet ring, at once (no fade).
        if sel {
            tile = tile.push(
                container(Space::new())
                    .width(bw)
                    .height(bh)
                    .style(move |_| container::Style {
                        border: iced::Border {
                            color: s.ink.scale_alpha(0.55 * s.opacity),
                            width: 1.5,
                            radius: 7.0.into(),
                        },
                        ..container::Style::default()
                    }),
            );
        }
        let tile = control(tile)
            .width(bw)
            .height(bh)
            .radius(7.0)
            .no_press_scale()
            .on_enter(msg(Message::Hover(w.address.clone())))
            .on_exit(msg(Message::Unhover(w.address.clone())))
            .on_press(focus_msg(&w.address))
            // Right click: how this app's windows open (its rule in Settings > Windows).
            .on_right_press(AppMessage::RuleFor(w.class.clone()));
        layers = layers.push(pin(tile).x(x).y(y));
    }
    // Under the card: its number (in the bar's white capsule when it is the one you are on)
    // and what is there.
    let summary = match windows.len() {
        0 => String::new(),
        1 => apps::name_for_class(&windows[0].class),
        n => format!("{n} windows"),
    };
    let number: Element<'_, AppMessage> = if current {
        container(label_w(
            id.to_string(),
            text::SMALL,
            s.on_accent,
            theme::SEMIBOLD,
        ))
        .padding([tok::HAIR, tok::M])
        .style(move |_| container::Style {
            background: Some(s.fade(s.accent).into()),
            border: iced::Border::default().rounded(tok::R_CONTROL),
            ..container::Style::default()
        })
        .into()
    } else {
        label_w(id.to_string(), text::SMALL, s.ink_dim, theme::SEMIBOLD).into()
    };
    let caption = row![
        number,
        label(
            ui::fit(&summary, text::CAPTION, width - 30.0),
            text::CAPTION,
            s.ink_dim
        ),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center)
    .padding([tok::NONE, tok::XXS]);
    column![layers, container(caption).height(CAPTION).center_y(CAPTION)]
        .spacing(CAPTION_GAP)
        .width(width)
        .into()
}

fn focus_msg(address: &str) -> AppMessage {
    AppMessage::Batch(vec![
        AppMessage::Dispatch(crate::services::compositor::Action::FocusWindow(
            String::from(address),
        )),
        AppMessage::Close,
    ])
}
