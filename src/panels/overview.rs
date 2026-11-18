//! Overview (Super+Tab): where everything is open. The workspaces that have windows (and the
//! one you are on), each a plain card with its windows drawn where they are, as live
//! pictures; the panel is only as wide as its cards (no search, no title line: design call).
//! Nothing is marked until you choose: a thin ring shows the window under the pointer (gone
//! when it leaves) or the one Tab and the arrows reached; Enter or a click goes there. Nothing fades or glides.

use super::PanelKind;
use super::cx::Cx;
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
    /// Go to a workspace (its card's number), closing the overview.
    Workspace(i32),
    /// Go to a window, closing the overview.
    Focus(String),
    /// Its app's window rule in Settings (right click).
    Rule(String),
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

type Act = super::action::Action<Message>;

pub fn open(st: &mut State) -> Act {
    // Nothing marked until the pointer or a key chooses (a ring on the current window
    // before you did anything read as a heavy border: design call).
    st.selected = None;
    // The pictures were taken just before it opened (`prepare`); the live stream keeps them
    // moving. Positions change without an event (resizing): read them fresh.
    Act::ReadHyprland
}

/// Fresh pictures of every window: captured, scaled and rounded to their tiles off the UI
/// thread. Taken when the overview opens and in the background whenever the set of windows
/// changes (so the first open has pictures, not icons); while it is open, `live_jobs` keeps
/// each picture moving with its window.
/// Opening: every window's picture is taken first (all at once, about a frame), then the
/// overview shows them, current, never an old picture changing in front of you. If the
/// compositor is slow, it opens anyway after `WAIT` with what it has.
pub fn prepare(
    st: &mut State,
    hypr: &hyprland::State,
    output: iced::OutputId,
    at: iced::Rectangle,
) -> Act {
    const WAIT: std::time::Duration = std::time::Duration::from_millis(90);
    // Every window has a picture already (taken when it opened, kept fresh while shown): open at
    // once and refresh them behind it. Waiting for fresh ones every time put ~40 ms between the
    // key and the overview (it read as a delay).
    let complete = hypr
        .windows
        .iter()
        .filter(|w| w.workspace > 0)
        .all(|w| st.previews.contains_key(&w.address));
    if complete {
        let open = Act::Open {
            kind: PanelKind::Overview,
            output,
            at,
        };
        let jobs = live_jobs(hypr);
        let fresh = Task::perform(
            crate::services::previews::snapshot(jobs, WAIT),
            Message::Live,
        );
        return open.and(Act::Run(fresh));
    }
    st.token += 1;
    let token = st.token;
    st.pending = Some((token, output, at));
    let jobs = live_jobs(hypr);
    Act::Run(Task::perform(
        crate::services::previews::snapshot(jobs, WAIT),
        move |frames| Message::Fresh(token, frames),
    ))
}

/// Pictures for windows that have none at their tile's size yet (a window opened or was
/// resized), taken in the background while the overview is closed: the others keep theirs,
/// so opening one window no longer copies every window on the desktop.
pub fn refresh_missing(st: &State, hypr: &hyprland::State) -> Act {
    let have = &st.previews;
    snapshot(
        live_jobs(hypr)
            .into_iter()
            .filter(|j| {
                have.get(&j.address)
                    .is_none_or(|r| r.size != (j.width.max(1), j.height.max(1)))
            })
            .collect(),
    )
}

fn snapshot(jobs: Vec<crate::services::previews::Job>) -> Act {
    if jobs.is_empty() {
        return Act::None;
    }
    Act::Run(Task::perform(
        crate::services::previews::snapshot(jobs, std::time::Duration::from_millis(250)),
        Message::Live,
    ))
}

/// The windows to keep live while the overview is open, each at its tile's size.
pub fn live_jobs(hypr: &hyprland::State) -> Vec<crate::services::previews::Job> {
    let (cw, ch) = card_size();
    hypr.windows
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
    // At least 16 × 12, and always on the card: a window at a screen's edge (or past it) used
    // to put its corner so far in that less than the minimum was left, and `clamp(min, max)`
    // with max < min panicked: kestrel crashed on every start while that window stayed there
    // (76 times on 2026-11-15).
    let (min_w, min_h) = (16.0, 12.0);
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let x = INSET + finite(w.x * k).clamp(0.0, (cw - 2.0 * INSET - min_w).max(0.0));
    let y = INSET + finite(w.y * k).clamp(0.0, (ch - 2.0 * INSET - min_h).max(0.0));
    (
        x,
        y,
        (w.w * k).min(cw - INSET - x).max(min_w),
        (w.h * k).min(ch - INSET - y).max(min_h),
    )
}

/// A workspace card's picture size (for a 16:9 screen).
pub fn card_size() -> (f32, f32) {
    let w = (WIDTH - 2.0 * PAD - (COLUMNS as f32 - 1.0) * GAP) / COLUMNS as f32;
    (w, w * 9.0 / 16.0)
}

/// The workspaces shown: those with windows, and the one you are on (empty ones are left
/// out: a row of blank cards was noise).
fn workspace_ids(hypr: &hyprland::State) -> Vec<i32> {
    let current = hypr.active_on(&hypr.focused_monitor);
    let mut ids: Vec<i32> = hypr
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
fn windows_on(hypr: &hyprland::State, id: i32) -> Vec<&hyprland::Window> {
    let mut v: Vec<&hyprland::Window> = hypr.windows.iter().filter(|w| w.workspace == id).collect();
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
fn order(hypr: &hyprland::State) -> Vec<(usize, &hyprland::Window)> {
    workspace_ids(hypr)
        .into_iter()
        .enumerate()
        .flat_map(|(card, id)| windows_on(hypr, id).into_iter().map(move |w| (card, w)))
        .collect()
}

/// The window you were in.
fn focused(hypr: &hyprland::State) -> Option<String> {
    hypr.windows
        .iter()
        .min_by_key(|w| w.focus)
        .map(|w| w.address.clone())
}

fn focus(address: &str) -> Act {
    Act::Dispatch(crate::services::compositor::Action::FocusWindow(
        String::from(address),
    ))
    .and(Act::Close)
}

pub fn update(st: &mut State, m: Message, cx: &Cx) -> Act {
    let hypr = cx.hypr;
    match m {
        Message::Workspace(id) => {
            return Act::Dispatch(crate::services::compositor::Action::FocusWorkspace(id))
                .and(Act::Close);
        }
        Message::Focus(a) => return focus(&a),
        Message::Rule(class) => return Act::WindowRule(class),
        Message::Fresh(token, frames) => {
            let _ = update(st, Message::Live(frames), cx);
            if let Some((t, output, at)) = st.pending
                && t == token
            {
                st.pending = None;
                return Act::Open {
                    kind: PanelKind::Overview,
                    output,
                    at,
                };
            }
        }
        Message::Live(frames) => {
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
            let open: std::collections::HashSet<&str> =
                hypr.windows.iter().map(|w| w.address.as_str()).collect();
            st.previews.retain(|a, _| open.contains(a.as_str()));
        }
        Message::Hover(a) => st.selected = Some(a),
        Message::Unhover(a) => {
            if st.selected.as_deref() == Some(a.as_str()) {
                st.selected = None;
            }
        }
        Message::Submit => {
            let list = order(hypr);
            let target = st
                .selected
                .as_ref()
                .filter(|a| list.iter().any(|(_, w)| &w.address == *a))
                .cloned()
                .or_else(|| focused(hypr))
                .or_else(|| list.first().map(|(_, w)| w.address.clone()));
            if let Some(a) = target {
                return focus(&a);
            }
        }
    }
    Act::None
}

/// The panel's height, worked out from its rows (not measured after drawing): opening after
/// the number of rows changed showed the old height for a frame or two, a tall empty frame
/// snapping shut.
pub fn height(hypr: &hyprland::State) -> f32 {
    let (_, ch) = card_size();
    let rows = workspace_ids(hypr).len().div_ceil(COLUMNS).max(1) as f32;
    2.0 * PAD + rows * (ch + CAPTION_GAP + CAPTION) + (rows - 1.0) * GAP
}

/// The panel's width: its cards side by side (up to a row of five).
pub fn width(hypr: &hyprland::State) -> f32 {
    let (cw, _) = card_size();
    let n = workspace_ids(hypr).len().clamp(1, COLUMNS) as f32;
    2.0 * PAD + n * cw + (n - 1.0) * GAP
}

pub fn key(st: &mut State, k: super::Key, cx: &Cx) -> Act {
    use super::Key;
    let hypr = cx.hypr;
    if k == Key::Enter {
        return update(st, Message::Submit, cx);
    }
    let list = order(hypr);
    if list.is_empty() {
        return Act::None;
    }
    let cur = st
        .selected
        .clone()
        .or_else(|| focused(hypr))
        .and_then(|a| list.iter().position(|(_, w)| w.address == a));
    // Every key moves, starting from the window you are in (the first press used to only mark
    // it: a press that did nothing).
    let Some(cur) = cur else {
        st.selected = list.first().map(|(_, w)| w.address.clone());
        return Act::None;
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
                st.selected = Some(list[i].1.address.clone());
                return Act::None;
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
        _ => return Act::None,
    };
    st.selected = Some(list[next].1.address.clone());
    Act::None
}

fn app_icon<'a>(class: &str, px: f32, s: Scheme) -> Element<'a, Message> {
    ui::app_icon(s, class, px, Icon::AppWindow)
}

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let (s, hypr) = (cx.scheme, cx.hypr);
    let card_w = (WIDTH - 2.0 * PAD - (COLUMNS as f32 - 1.0) * GAP) / COLUMNS as f32;
    let active = hypr.active_on(&hypr.focused_monitor);
    let cards: Vec<Element<'a, Message>> = workspace_ids(hypr)
        .into_iter()
        .map(|id| card(st, hypr, s, id, active == Some(id), card_w))
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
    st: &'a State,
    hypr: &'a hyprland::State,
    s: Scheme,
    id: i32,
    current: bool,
    width: f32,
) -> Element<'a, Message> {
    let windows = windows_on(hypr, id);
    let selected = st.selected.as_deref();
    let (_, height) = card_size();

    // A plain card (the blurred wallpaper behind each one was the heaviest thing drawn and
    // only added noise); a click on it switches to the workspace.
    let mut layers = stack![
        control(Space::new().width(width).height(height))
            .width(width)
            .height(height)
            .radius(tok::R_CARD())
            .kind(
                s,
                if current {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Tonal
                }
            )
            .no_press_scale()
            .on_press(Message::Workspace(id))
    ]
    .width(width)
    .height(height);
    for w in &windows {
        let (x, y, bw, bh) = tile_rect(w, width, height);
        let sel = selected == Some(w.address.as_str());
        let alpha = 1.0;
        // The window's last picture, even if its tile changed size since (scaled to fit until
        // the fresh one arrives): an icon flashed for a second before.
        let ready = st.previews.get(&w.address);
        let exact = ready.is_some_and(|r| r.size == (bw as u32, bh as u32));
        let body: Element<'_, Message> = match ready {
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
                    background: Some(s.ink.scale_alpha(tok::FILL * alpha * s.opacity).into()),
                    border: iced::Border::default().rounded(tok::R_CONTROL()),
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
                            color: s.ink.scale_alpha(tok::RING * s.opacity),
                            width: 1.5,
                            radius: tok::R_CONTROL().into(),
                        },
                        ..container::Style::default()
                    }),
            );
        }
        let tile = control(tile)
            .width(bw)
            .height(bh)
            .radius(tok::R_CONTROL())
            .no_press_scale()
            .on_enter(Message::Hover(w.address.clone()))
            .on_exit(Message::Unhover(w.address.clone()))
            .on_press(focus_msg(&w.address))
            // Right click: how this app's windows open (its rule in Settings > Windows).
            .on_right_press(Message::Rule(w.class.clone()));
        layers = layers.push(pin(tile).x(x).y(y));
    }
    // Under the card: its number (in the bar's white capsule when it is the one you are on)
    // and what is there.
    let summary = match windows.len() {
        0 => String::new(),
        1 => apps::name_for_class(&windows[0].class),
        n => format!("{n} windows"),
    };
    let number: Element<'_, Message> = if current {
        container(label_w(
            id.to_string(),
            text::SMALL,
            s.on_accent,
            theme::SEMIBOLD,
        ))
        .padding([tok::HAIR, tok::M])
        .style(move |_| container::Style {
            background: Some(s.fade(s.accent).into()),
            border: iced::Border::default().rounded(tok::R_CONTROL()),
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

fn focus_msg(address: &str) -> Message {
    Message::Focus(String::from(address))
}

#[cfg(test)]
mod tests {
    fn window(x: f32, y: f32, w: f32, h: f32) -> super::hyprland::Window {
        super::hyprland::Window {
            x,
            y,
            w,
            h,
            screen: (1920.0, 1080.0),
            ..Default::default()
        }
    }

    #[test]
    fn a_window_at_or_past_the_edge_stays_on_its_card() {
        let (cw, ch) = super::card_size();
        for w in [
            window(0.0, 1075.0, 800.0, 600.0),
            window(1915.0, 0.0, 800.0, 600.0),
            window(5000.0, 5000.0, 10.0, 10.0),
            window(-50.0, -50.0, 4000.0, 4000.0),
            window(f32::NAN, 0.0, 100.0, 100.0),
        ] {
            let (x, y, bw, bh) = super::tile_rect(&w, cw, ch);
            assert!(
                x + bw <= cw + 0.01 && y + bh <= ch + 0.01,
                "{x} {y} {bw} {bh}"
            );
            assert!(bw >= 16.0 && bh >= 12.0);
        }
    }
}
