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
    let Some(clips) = &st.clips else {
        return Vec::new();
    };
    matching(clips, &st.query, &pinned())
}

/// Indices of the `clips` matching `query`, the `pins` (ids) first in their order.
fn matching(clips: &[Clip], query: &str, pins: &[String]) -> Vec<usize> {
    let q = query.trim().to_lowercase();
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
    if glide && !crate::motion::instant() {
        st.glide.to(px, tok::SPRING_SHOW);
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

    /// Ids far past any real history's, so the history's own pins (read by `filtered`) never
    /// name one of these.
    const BASE: u64 = 1 << 60;

    fn text(n: u64, preview: &str) -> Clip {
        use crate::services::clips::Entry;
        Clip {
            key: (BASE + n).to_string(),
            entry: Entry {
                id: BASE + n,
                mime: "text/plain;charset=utf-8".into(),
                preview: preview.into(),
                ..Entry::default()
            },
            preview: preview.into(),
            thumb: None,
        }
    }

    fn picture(n: u64) -> Clip {
        use crate::services::clips::Entry;
        Clip {
            key: (BASE + n).to_string(),
            entry: Entry {
                id: BASE + n,
                mime: "image/png".into(),
                size: Some((10, 10)),
                bytes: 3 * 1024 * 1024 / 2,
                ..Entry::default()
            },
            preview: String::new(),
            thumb: None,
        }
    }

    fn key(n: u64) -> String {
        (BASE + n).to_string()
    }

    fn loaded(clips: Vec<Clip>) -> State {
        let mut st = State::default();
        let _ = update(&mut st, Message::Loaded(clips));
        st
    }

    fn many(n: u64) -> State {
        loaded((0..n).map(|i| text(i, &format!("entry {i}"))).collect())
    }

    #[test]
    fn a_filter_matches_text_in_any_case() {
        let clips = vec![
            text(0, "Hello World"),
            text(1, "goodbye"),
            text(2, "WORLD news"),
        ];
        assert_eq!(matching(&clips, "", &[]), [0, 1, 2]);
        assert_eq!(matching(&clips, "  world ", &[]), [0, 2]);
        assert_eq!(matching(&clips, "nothing", &[]), Vec::<usize>::new());
    }

    #[test]
    fn pictures_are_found_by_what_they_are() {
        let clips = vec![text(0, "an image of a cat"), picture(1), text(2, "cat")];
        assert_eq!(matching(&clips, "image", &[]), [0, 1]);
        assert_eq!(matching(&clips, "screenshot", &[]), [1]);
        assert_eq!(matching(&clips, "pic", &[]), [1]);
        assert_eq!(matching(&clips, "cat", &[]), [0, 2]);
    }

    #[test]
    fn pinned_entries_come_first_in_the_order_pinned() {
        let clips: Vec<Clip> = (0..5).map(|i| text(i, &format!("e{i}"))).collect();
        let pins = [key(3), key(1)];
        assert_eq!(matching(&clips, "", &pins), [3, 1, 0, 2, 4]);
        // Filtered too: a pin that does not match stays out.
        assert_eq!(matching(&clips, "e1", &pins), [1]);
        // A pin no longer in the history changes nothing.
        assert_eq!(matching(&clips, "", &[key(99)]), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn arrows_stay_inside_the_list() {
        let mut st = many(20);
        let _ = key_press(&mut st, crate::panels::Key::Up);
        assert_eq!(st.selected, 0);
        let _ = key_press(&mut st, crate::panels::Key::Down);
        assert_eq!(st.selected, 1);
        let _ = key_press(&mut st, crate::panels::Key::PageDown);
        assert_eq!(st.selected, 1 + ROWS);
        let _ = key_press(&mut st, crate::panels::Key::End);
        assert_eq!(st.selected, 19);
        let _ = key_press(&mut st, crate::panels::Key::Down);
        assert_eq!(st.selected, 19);
        let _ = key_press(&mut st, crate::panels::Key::Home);
        assert_eq!(st.selected, 0);
    }

    fn key_press(st: &mut State, k: crate::panels::Key) -> Act {
        super::key(st, k)
    }

    #[test]
    fn arrows_on_an_empty_list_do_nothing() {
        let mut st = loaded(Vec::new());
        let _ = update(&mut st, Message::Move(1));
        assert_eq!(st.selected, 0);
        let mut st = State::default();
        let _ = update(&mut st, Message::Move(-3));
        assert_eq!(st.selected, 0);
    }

    #[test]
    fn the_selection_is_always_on_screen() {
        let mut st = many(40);
        for target in [20, 39, 3, 0, 25] {
            let d = target as i32 - st.selected as i32;
            let _ = update(&mut st, Message::Move(d));
            assert_eq!(st.selected, target);
            assert!(
                st.top <= target && target < st.top + ROWS,
                "row {target} with the view from {}",
                st.top
            );
            assert!(st.glide.target >= 0.0 && st.glide.target <= max_scroll(40));
        }
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let mut st = many(30);
        let _ = update(&mut st, Message::Scroll(-1e6, false));
        assert_eq!(st.glide.target, max_scroll(30));
        let _ = update(&mut st, Message::Scroll(1e6, false));
        assert_eq!(st.glide.target, 0.0);
        // A list shorter than the view does not scroll at all.
        let mut st = many(3);
        let _ = update(&mut st, Message::Scroll(-500.0, false));
        assert_eq!(st.glide.target, 0.0);
    }

    #[test]
    fn typing_selects_the_top_match_and_forgets_clear_all() {
        let mut st = many(20);
        let _ = update(&mut st, Message::Move(10));
        let _ = update(&mut st, Message::Clear);
        assert!(st.confirm_clear);
        let _ = update(&mut st, Message::Query("entry 1".into()));
        assert_eq!((st.selected, st.top), (0, 0));
        assert!(
            !st.confirm_clear,
            "typing takes back the half-pressed Clear all"
        );
    }

    #[test]
    fn a_narrower_filter_pulls_the_selection_back_in() {
        let mut st = many(20);
        let _ = update(&mut st, Message::Query("entry".into()));
        let _ = update(&mut st, Message::Move(15));
        assert_eq!(st.selected, 15);
        st.query = "entry 1".into();
        let _ = update(&mut st, Message::Move(0));
        // "entry 1", "entry 10" … "entry 19": eleven rows.
        assert_eq!(st.selected, 10);
    }

    #[test]
    fn enter_pastes_and_shift_enter_only_copies() {
        let mut st = loaded(vec![text(0, "a"), text(1, "b")]);
        let _ = update(&mut st, Message::Move(1));
        match key_press(&mut st, crate::panels::Key::Enter) {
            Act::Many(v) => assert!(matches!(
                v.as_slice(),
                [Act::PasteClip { key: k, paste: true }, Act::Close] if *k == key(1)
            )),
            _ => panic!("not pasted"),
        }
        match key_press(&mut st, crate::panels::Key::ShiftEnter) {
            Act::Many(v) => assert!(matches!(
                v.as_slice(),
                [Act::PasteClip { paste: false, .. }, Act::Close]
            )),
            _ => panic!("not copied"),
        }
    }

    #[test]
    fn the_row_pasted_is_the_one_shown_under_the_filter() {
        let mut st = loaded(vec![
            text(0, "apple"),
            text(1, "banana"),
            text(2, "apricot"),
        ]);
        let _ = update(&mut st, Message::Query("ap".into()));
        match update(&mut st, Message::Copy(1)) {
            Act::Many(v) => assert!(matches!(
                v.as_slice(),
                [Act::PasteClip { key: k, .. }, Act::Close] if *k == key(2)
            )),
            _ => panic!("not pasted"),
        }
        assert!(matches!(update(&mut st, Message::Copy(2)), Act::None));
    }

    #[test]
    fn delete_takes_the_row_shown_and_keeps_a_selection() {
        let mut st = loaded(vec![
            text(0, "apple"),
            text(1, "banana"),
            text(2, "apricot"),
        ]);
        let _ = update(&mut st, Message::Query("ap".into()));
        let _ = update(&mut st, Message::Move(1));
        let _ = key_press(&mut st, crate::panels::Key::Delete);
        let left: Vec<String> = st
            .clips
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.preview.clone())
            .collect();
        assert_eq!(left, ["apple", "banana"]);
        assert_eq!(st.selected, 0, "the selection moves onto what is left");
        // Nothing at that row: nothing happens.
        let _ = update(&mut st, Message::Delete(7));
        assert_eq!(st.clips.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn clear_all_asks_once_first() {
        let mut st = many(5);
        let _ = update(&mut st, Message::Clear);
        assert!(st.confirm_clear);
        assert_eq!(
            st.clips.as_ref().unwrap().len(),
            5,
            "the first press only asks"
        );
        let _ = update(&mut st, Message::Clear);
        assert!(!st.confirm_clear);
        assert!(st.clips.as_ref().unwrap().is_empty());
    }

    #[test]
    fn opening_again_starts_fresh() {
        let mut st = many(30);
        let _ = update(&mut st, Message::Query("entry 2".into()));
        let _ = update(&mut st, Message::Move(5));
        let _ = update(&mut st, Message::Clear);
        let _ = open(&mut st);
        assert!(st.query.is_empty());
        assert_eq!((st.selected, st.top), (0, 0));
        assert!(!st.confirm_clear);
        assert_eq!(st.glide.target, 0.0);
    }

    #[test]
    fn thumbnails_arrive_for_their_own_entries() {
        let mut st = loaded(vec![picture(0), picture(1), text(2, "x")]);
        assert!(st.pending.contains(&key(0)) && st.pending.contains(&key(1)));
        let t = std::path::PathBuf::from("/nonexistent/kestrel-thumb.png");
        let _ = update(
            &mut st,
            Message::Thumbs(vec![(key(1), Some(t.clone())), (key(9), Some(t.clone()))]),
        );
        let clips = st.clips.as_ref().unwrap();
        assert_eq!(clips[1].thumb.as_ref(), Some(&t));
        assert_eq!(clips[0].thumb, None);
        assert!(!st.pending.contains(&key(1)));
        assert!(st.pending.contains(&key(0)));
    }

    #[test]
    fn a_text_read_once_is_shown_without_reading_again() {
        let mut st = loaded(vec![text(0, "short"), text(1, "other")]);
        assert!(
            st.reading.contains(&key(0)),
            "the rows in view are read ahead"
        );
        let _ = update(
            &mut st,
            Message::Texts(vec![(key(0), "short, in full".into())]),
        );
        assert!(!st.reading.contains(&key(0)));
        assert_eq!(
            st.preview,
            Some((key(0), "short, in full".into())),
            "the selected row's text shows at once"
        );
        let _ = update(&mut st, Message::Hover(1));
        let _ = update(&mut st, Message::Preview(key(1), "other, in full".into()));
        assert_eq!(st.preview, Some((key(1), "other, in full".into())));
        // Back on the first: its text is already in.
        let _ = update(&mut st, Message::Hover(0));
        assert_eq!(st.preview, Some((key(0), "short, in full".into())));
    }

    #[test]
    fn what_a_text_entry_is() {
        let info = |s: &str| text(0, s).text_info();
        assert_eq!(info("http://example.org/a b"), "Text · 22 characters");
        assert_eq!(info("http://www.example.org/x"), "Link · example.org");
        assert_eq!(info("~/Pictures/a.png"), "Path");
        assert_eq!(
            info("/etc/hosts\n/etc/passwd"),
            "Text · 2 lines · 22 characters"
        );
        assert_eq!(info("héllo"), "Text · 5 characters");
    }

    #[test]
    fn a_big_picture_is_counted_in_mebibytes() {
        assert_eq!(picture(0).image_info(), "PNG · 10 × 10 · 1.5 MiB");
    }
}
