//! Clipboard history (Super+V): everything kestrel's history keeps (services::clips), pinned entries first, then
//! newest first, until it is deleted by hand ("Clear all" keeps the pinned ones). Type to filter; Up/Down/PageUp/PageDown move; Enter (or a click) puts the entry back
//! on the clipboard and pastes it into the app you were in, like Windows' Win+V; Shift+Enter
//! only copies it. Delete removes the selected entry; "Clear all" asks once before wiping.
//!
//! The list is virtual: only the visible rows are built, so thousands of entries cost nothing.
//! Image entries get a thumbnail, made when first shown and kept in /run.

mod store;
mod update;
mod view;
pub use store::*;
pub use update::*;
pub use view::*;

use super::cx::Cx;
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, container, mouse_area};
use iced::{Alignment, Element, Length, Task};
use std::collections::HashSet;
use std::path::PathBuf;

pub const WIDTH: f32 = crate::theme::tok::PANEL_XL;
/// The list on the left; the selected entry's preview fills the rest.
const LIST_W: f32 = 430.0;
/// Rows shown at once.
const ROWS: usize = 8;
const ROW_H: f32 = 54.0;
/// A row and the gap under it.
const STRIDE: f32 = ROW_H + 2.0;
/// The list's visible height.
const VIEW_H: f32 = ROWS as f32 * STRIDE;
const THUMB_W: f32 = 64.0;
const THUMB_H: f32 = 36.0;

/// One clipboard history entry; images get a small thumbnail so they can be told apart.
#[derive(Clone, Debug)]
pub struct Clip {
    /// The history's id for it, as text (how the panel's caches name it).
    key: String,
    pub entry: crate::services::clips::Entry,
    pub preview: String,
    pub thumb: Option<PathBuf>,
}

impl Clip {
    pub fn id(&self) -> &str {
        &self.key
    }
    pub fn is_image(&self) -> bool {
        self.entry.is_image()
    }
    /// "PNG · 562 × 850 · 248 KiB".
    fn image_info(&self) -> String {
        let kind = self.entry.mime.trim_start_matches("image/").to_uppercase();
        let kib = self.entry.bytes.div_ceil(1024);
        let amount = if kib >= 1024 {
            format!("{:.1} MiB", kib as f64 / 1024.0)
        } else {
            format!("{kib} KiB")
        };
        match self.entry.size {
            Some((w, h)) => format!("{kind} · {w} × {h} · {amount}"),
            None => format!("{kind} · {amount}"),
        }
    }
    /// What a text entry is: a link (and where to), a file path, or text and its length.
    fn text_info(&self) -> String {
        let t = self.preview.trim();
        if let Some(rest) = t
            .strip_prefix("https://")
            .or_else(|| t.strip_prefix("http://"))
            .filter(|_| !t.contains(char::is_whitespace))
        {
            let host = rest.split('/').next().unwrap_or(rest);
            return format!("Link · {}", host.trim_start_matches("www."));
        }
        if (t.starts_with('/') || t.starts_with("~/")) && !t.contains('\n') && t.len() < 300 {
            return "Path".into();
        }
        let n = t.chars().count();
        let lines = t.lines().count();
        if lines > 1 {
            format!("Text · {lines} lines · {n} characters")
        } else {
            format!("Text · {n} characters")
        }
    }
}

#[derive(Default)]
pub struct State {
    /// The scrollbar is being dragged; where the pointer is on it (0 to 1).
    pub dragging: bool,
    bar_at: f32,
    /// While dragging: where the pointer was in the panel and the bar position then, so the
    /// drag follows the pointer anywhere over the panel (not only on the slim bar).
    anchor: Option<(f32, f32)>,
    query: String,
    selected: usize,
    /// First visible row of the filtered list.
    top: usize,
    /// How far the list is scrolled, in pixels (the rows move by the pixel, not by the row):
    /// where it is now (springing towards its target on a wheel step) and where it is going.
    pub glide: crate::motion::Spring,
    clips: Option<Vec<Clip>>,
    /// Thumbnails being made (by entry id).
    pending: HashSet<String>,
    /// "Clear all" was pressed once.
    confirm_clear: bool,
    /// The selected text entry in full (the list has only its start): its id and text.
    preview: Option<(String, String)>,
    /// Full texts already read (by entry id), so moving over the list never starts a reader
    /// process per row more than once.
    texts: std::collections::HashMap<String, String>,
    /// Texts being read now (by entry id), so a row is never read twice at once.
    reading: HashSet<String>,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// The pointer came onto a row: it becomes the selection (one highlight, never two).
    Hover(usize),
    /// The scrollbar: pressed, the pointer moved on it (0 to 1 of its length), released.
    BarPress,
    BarMove(f32),
    BarRelease,
    /// While dragging: the pointer moved (its y on the screen).
    DragMove(f32),
    /// The pointer left the rows.
    LeaveRows,
    Loaded(Vec<Clip>),
    Thumbs(Vec<(String, Option<PathBuf>)>),
    Query(String),
    Move(i32),
    /// Scrolled by pixels (up is positive); `true` for a wheel step, which glides.
    Scroll(f32, bool),
    /// Back on the clipboard and pasted into the app you were in.
    Copy(usize),
    /// Back on the clipboard only.
    CopyOnly(usize),
    Delete(usize),
    Clear,
    /// Keep the entry at the top (or stop keeping it), like Windows' clipboard pins.
    Pin(usize),
    /// A selected picture's big preview is made.
    PictureReady,
    /// A text entry's full text came in (its id, the text).
    Preview(String, String),
    /// Full texts of the rows around the view, read ahead in one go.
    Texts(Vec<(String, String)>),
}

type Act = super::action::Action<Message>;

pub(super) static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("clipboard-input"));

pub fn open(st: &mut State) -> Act {
    st.query.clear();
    st.selected = 0;
    st.dragging = false;
    st.top = 0;
    st.glide.snap(0.0);
    st.confirm_clear = false;
    st.pending.clear();
    Act::Run(Task::batch([
        Task::perform(clip_list(), Message::Loaded),
        Task::Iced(iced::widget::focus(INPUT_ID.clone())),
    ]))
}

/// The pinned entries' ids, in the order they were pinned.
fn pinned() -> Vec<String> {
    crate::services::clips::pinned()
        .iter()
        .map(u64::to_string)
        .collect()
}

/// Indices of the entries matching the filter, pinned ones first. "image" finds pictures.
fn filtered(st: &State) -> Vec<usize> {
    let q = st.query.trim().to_lowercase();
    let Some(clips) = &st.clips else {
        return Vec::new();
    };
    let pins = pinned();
    let mut v: Vec<usize> = clips
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            q.is_empty()
                || if c.is_image() {
                    "image picture screenshot".contains(q.as_str())
                        || c.preview.to_lowercase().contains(&q)
                } else {
                    c.preview.to_lowercase().contains(&q)
                }
        })
        .map(|(i, _)| i)
        .collect();
    // Pinned first, in the order they were pinned; the rest newest first.
    v.sort_by_key(|&i| {
        pins.iter()
            .position(|p| p == clips[i].id())
            .unwrap_or(usize::MAX)
    });
    v
}

/// Keep the selection on screen and its neighbours' thumbnails coming.
fn settle(st: &mut State) -> Task<Message> {
    let n = filtered(st).len();
    st.selected = st.selected.min(n.saturating_sub(1));
    // Scrolled just enough to show the selection, by the pixel.
    let max = max_scroll(n);
    let mut px = st.glide.target;
    let sel_top = st.selected as f32 * STRIDE;
    if sel_top < px {
        px = sel_top;
    } else if sel_top + STRIDE > px + VIEW_H {
        px = sel_top + STRIDE - VIEW_H;
    }
    scroll_to(st, px.clamp(0.0, max), false);
    let thumbs = thumbs_for_view(st);
    Task::batch([thumbs, preview_for(st)])
}

/// The furthest the list scrolls for `n` rows.
fn max_scroll(n: usize) -> f32 {
    (n as f32 * STRIDE - VIEW_H).max(0.0)
}

/// Scrolls to `px`: at once (a touchpad, a drag, a key) or gliding (a wheel step).
fn scroll_to(st: &mut State, px: f32, glide: bool) {
    if glide && !crate::motion::still() {
        st.glide.to(px, crate::motion::Spring::SHOW);
    } else {
        st.glide.snap(px);
    }
    st.top = (px / STRIDE).floor().max(0.0) as usize;
}

/// The preview pane's size.
fn preview_size() -> (f32, f32) {
    (
        WIDTH - LIST_W - 14.0 - 28.0,
        ROWS as f32 * (ROW_H + 2.0) - 56.0,
    )
}

/// The full texts of the text rows in and near view, read ahead off the drawing thread, so
/// moving onto any of them shows its text in the same frame (not a second one after).
fn texts_for_view(st: &mut State) -> Task<Message> {
    let Some(clips) = &st.clips else {
        return Task::none();
    };
    let want: Vec<(String, String)> = filtered(st)
        .into_iter()
        .skip(st.top)
        .take(ROWS * 2)
        .map(|i| &clips[i])
        .filter(|c| !c.is_image() && !st.texts.contains_key(c.id()) && !st.reading.contains(c.id()))
        .map(|c| (c.id().to_string(), c.id().to_string()))
        .collect();
    if want.is_empty() {
        return Task::none();
    }
    st.reading.extend(want.iter().map(|(id, _)| id.clone()));
    Task::perform(
        async move {
            let mut out = Vec::with_capacity(want.len());
            for (id, line) in want {
                out.push((id, clip_text(line).await));
            }
            out
        },
        Message::Texts,
    )
}

/// The selected text entry's full text, when not already in. Text already read is put in at
/// once, in this same update: sent as a message it drew the frame twice for every key press
/// (the new row, then its text).
fn preview_for(st: &mut State) -> Task<Message> {
    let Some(c) = selected_clip(st) else {
        return Task::none();
    };
    // A picture: its big preview made off the drawing thread (decoding it in `view` froze
    // input for a moment on every new picture while moving with the keys).
    if c.is_image() {
        let Some(t) = c.thumb.clone() else {
            return Task::none();
        };
        let (pw, ph) = preview_size();
        if ui::fitted_file_ready(&t, pw - 24.0, ph - 24.0).is_some() {
            return Task::none();
        }
        return Task::perform(
            async move {
                let _ = tokio::task::spawn_blocking(move || {
                    ui::fitted_file(&t, pw - 24.0, ph - 24.0, 8.0)
                })
                .await;
            },
            |()| Message::PictureReady,
        );
    }
    if st.preview.as_ref().is_some_and(|(id, _)| id == c.id()) {
        return Task::none();
    }
    if let Some(t) = st.texts.get(c.id()) {
        st.preview = Some((c.id().to_string(), t.clone()));
        return Task::none();
    }
    let (id, line) = (c.id().to_string(), c.id().to_string());
    Task::perform(clip_text(line), move |t| Message::Preview(id.clone(), t))
}

fn selected_clip(st: &State) -> Option<&Clip> {
    let k = *filtered(st).get(st.selected)?;
    st.clips.as_ref()?.get(k)
}

/// The small row pictures of the rows in and near view, made off the drawing thread.
fn row_pictures(st: &State) -> Task<Message> {
    let Some(clips) = &st.clips else {
        return Task::none();
    };
    let files: Vec<PathBuf> = filtered(st)
        .into_iter()
        .skip(st.top)
        .take(ROWS * 2)
        .filter_map(|i| clips[i].thumb.clone())
        .filter(|t| ui::fitted_file_ready(t, THUMB_W, THUMB_H).is_none())
        .collect();
    if files.is_empty() {
        return Task::none();
    }
    Task::perform(
        async move {
            let _ = tokio::task::spawn_blocking(move || {
                for f in files {
                    let _ = ui::fitted_file(&f, THUMB_W, THUMB_H, 6.0);
                }
            })
            .await;
        },
        |()| Message::PictureReady,
    )
}

fn thumbs_for_view(st: &mut State) -> Task<Message> {
    let rows = Task::batch([row_pictures(st), texts_for_view(st)]);
    let Some(clips) = &st.clips else {
        return rows;
    };
    let want: Vec<String> = filtered(st)
        .into_iter()
        .skip(st.top)
        .take(ROWS * 2)
        .map(|i| &clips[i])
        .filter(|c| c.is_image() && c.thumb.is_none() && !st.pending.contains(c.id()))
        .map(|c| c.id().to_string())
        .collect();
    if want.is_empty() {
        return rows;
    }
    for l in &want {
        st.pending
            .insert(l.split('\t').next().unwrap_or_default().to_string());
    }
    Task::perform(
        async move {
            let mut out = Vec::new();
            for line in want {
                let id = line.split('\t').next().unwrap_or_default().to_string();
                out.push((id, thumbnail(&line).await));
            }
            out
        },
        Message::Thumbs,
    )
    .chain(rows)
}

// --- the history (services::clips) -------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_told_from_text() {
        use crate::services::clips::Entry;
        let c = Clip {
            key: "1".into(),
            entry: Entry {
                id: 1,
                mime: "image/png".into(),
                size: Some((562, 850)),
                bytes: 248 * 1024,
                ..Entry::default()
            },
            preview: String::new(),
            thumb: None,
        };
        assert!(c.is_image());
        assert_eq!(c.image_info(), "PNG · 562 × 850 · 248 KiB");
        let t = Clip {
            key: "2".into(),
            entry: Entry {
                id: 2,
                mime: "text/plain;charset=utf-8".into(),
                ..Entry::default()
            },
            preview: "https://www.github.com/anomalyco/rune".into(),
            thumb: None,
        };
        assert!(!t.is_image());
        assert_eq!(t.text_info(), "Link · github.com");
    }
}
