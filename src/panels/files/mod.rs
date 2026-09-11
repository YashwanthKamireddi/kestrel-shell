//! The file picker: what an app gets when it asks to open or save a file (through the desktop
//! portal, `services::filechooser`). Modelled on the macOS open panel and GTK 4's chooser:
//! places in a sidebar, the folder as a list with Name / Size / Modified columns (a click on a
//! column sorts by it, again reverses it; remembered per folder, Downloads newest first),
//! search as you type (this folder, then the folders below on the same drive), a typed path
//! goes where it says (Tab completes, Enter opens), and the app's file types at the bottom.
//! Click selects, a second click (or Enter) opens a folder or chooses the file; Esc cancels.
//! When a search, a path or a file type leaves nothing to show, the list says why.

mod folder;
mod update;
mod view;

pub use update::{key, update};
pub use view::view;

use super::cx::Cx;
use crate::app::Message as AppMessage;
use crate::services::filechooser::{self, Answer, Request};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, container, mouse_area, text_input};
use iced::{Alignment, Element, Length, Task};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use folder::*;
use update::*;
use view::kind_icon;

pub const WIDTH: f32 = crate::theme::tok::PANEL_XL;
const ROWS: usize = 12;
const ROW_H: f32 = 40.0;
const SIDEBAR: f32 = 184.0;
const SIZE_W: f32 = 84.0;
const DATE_W: f32 = 132.0;

#[derive(Clone, Debug)]
struct Item {
    path: PathBuf,
    name: String,
    dir: bool,
    size: u64,
    modified: Option<SystemTime>,
    /// Where it is, for results from elsewhere (a search, recent files): "Pictures/Roll".
    rel: Option<String>,
    /// Its type by its name (shared MIME database; read once, no file opened), for the app's
    /// file-type filters.
    mime: Option<String>,
}

/// What the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    /// The current folder.
    #[default]
    Folder,
    /// A typed or pasted path: what is in its folder that starts like it.
    Path,
    /// Search: matches here, then in every folder below.
    Search,
    /// Files opened lately (the desktop's recent-files list).
    Recent,
}

/// What the list is sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum By {
    #[default]
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sort {
    by: By,
    /// Z to A, largest first, newest first.
    desc: bool,
}

impl Sort {
    fn key(self) -> String {
        let by = match self.by {
            By::Name => "name",
            By::Size => "size",
            By::Modified => "modified",
        };
        if self.desc {
            format!("{by}-desc")
        } else {
            by.to_string()
        }
    }

    fn parse(k: &str) -> Option<Self> {
        let (by, desc) = match k.strip_suffix("-desc") {
            Some(b) => (b, true),
            None => (k, false),
        };
        let by = match by {
            "name" => By::Name,
            "size" => By::Size,
            "modified" => By::Modified,
            _ => return None,
        };
        Some(Self { by, desc })
    }
}

/// How `dir` is sorted: as it was last sorted, else Downloads newest first (what was just
/// downloaded is what is usually wanted), everything else by name.
fn sort_for(dir: &Path) -> Sort {
    let key = dir.to_string_lossy().into_owned();
    if let Some(s) = crate::state::get(|s| s.files.sort.get(&key).cloned())
        .as_deref()
        .and_then(Sort::parse)
    {
        return s;
    }
    if dir == home().join("Downloads") {
        Sort {
            by: By::Modified,
            desc: true,
        }
    } else {
        Sort::default()
    }
}

#[derive(Default)]
pub struct State {
    pub req: Option<Request>,
    dir: PathBuf,
    /// The current folder's contents.
    listing: Vec<Item>,
    /// What the list shows (the listing, completions, search results or recent files).
    items: Vec<Item>,
    mode: Mode,
    sort: Sort,
    hidden: bool,
    /// Folders visited before / after (Back, Forward).
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    /// A new folder being named.
    new_folder: Option<String>,
    /// Saving over this existing file was asked for once; the next Save replaces it.
    replace: Option<PathBuf>,
    query: String,
    selected: Option<usize>,
    /// Several chosen (when the app allows more than one): of the list shown, as GTK's and
    /// macOS's pickers keep it (a click picks one, Ctrl adds or takes one away, Shift a run);
    /// a new list (another folder, a search, the filter) starts with none.
    picked: Vec<PathBuf>,
    /// Where a Shift-click's run starts (the last plain or Ctrl click).
    anchor: Option<PathBuf>,
    /// Ctrl and Shift as held now (for a click).
    mods: iced::keyboard::Modifiers,
    top: usize,
    scroll: f32,
    /// Where the list is scrolled, in rows (fractional: it moves by the pixel).
    pos: f32,
    /// Saving: the name typed.
    name: String,
    /// The app's file type filter in use.
    filter: usize,
    /// The last click (row, when), to tell a double click.
    click: Option<(usize, Instant)>,
    /// A search of the folders below is running.
    searching: bool,
    /// For kestrel's own requests (`filechooser::OWN`): what the choice becomes.
    pub then: Option<Box<dyn Fn(PathBuf) -> AppMessage>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// Thumbnails made in the background are ready.
    Pictures,
    Go(PathBuf),
    Up,
    Query(String),
    Name(String),
    Click(usize),
    /// Ctrl or Shift pressed or let go.
    Mods(iced::keyboard::Modifiers),
    Accept,
    Cancel,
    Scroll(f32),
    Filter(usize),
    Back,
    Forward,
    Hidden,
    /// Sort by this column (the one in use: the other way round).
    SortBy(By),
    Recent,
    /// Search results from the folders below: (for which query, paths).
    Found(String, Vec<PathBuf>),
    NewFolder,
    FolderName(String),
    CreateFolder,
}

type Act = super::action::Action<Message>;

pub(super) static SEARCH_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("files-search"));
static NEW_FOLDER_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("files-new-folder"));
pub(super) static NAME_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("files-name"));

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// A new request: start where the app asked, else in the last folder used, else home.
pub fn begin(st: &mut State, req: Request) -> Act {
    // One at a time: an earlier request still open is cancelled.
    if let Some(old) = st.req.take() {
        filechooser::answer(old.id, Answer::Cancel);
    }
    if req.id != filechooser::OWN {
        st.then = None;
    }
    let start = req
        .folder
        .clone()
        .or_else(last_folder)
        .filter(|p| p.is_dir())
        .unwrap_or_else(home);
    st.name = req.name.clone();
    // The app's chosen filter first (its `current_filter`), else its first.
    st.filter = req.current.unwrap_or(0);
    st.picked.clear();
    let save = req.save;
    st.req = Some(req);
    load(st, start);
    Act::Run(Task::batch([
        Task::Iced(iced::widget::focus(if save {
            NAME_ID.clone()
        } else {
            SEARCH_ID.clone()
        })),
        pictures(st),
    ]))
}

/// The picker closed without an answer (Esc, a click outside, another panel): cancelled.
pub fn closed(st: &mut State) {
    if let Some(r) = st.req.take() {
        filechooser::answer(r.id, Answer::Cancel);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn globs_match_like_a_shell() {
        assert!(super::glob("*.png", "Shot.PNG"));
        assert!(super::glob("*.tar.*", "a.tar.gz"));
        assert!(!super::glob("*.png", "a.jpg"));
    }

    #[test]
    fn names_sort_like_people_count() {
        use std::cmp::Ordering::*;
        assert_eq!(super::natural("Shot 2.png", "Shot 10.png"), Less);
        assert_eq!(super::natural("a", "B"), Less);
        assert_eq!(super::natural("x01", "x1"), Equal);
    }

    #[test]
    fn sort_keys_round_trip() {
        for k in [
            "name",
            "name-desc",
            "size-desc",
            "modified-desc",
            "modified",
        ] {
            assert_eq!(
                super::Sort::parse(k).map(super::Sort::key).as_deref(),
                Some(k)
            );
        }
        assert!(super::Sort::parse("kind").is_none());
    }

    #[test]
    fn by_date_mixes_folders_in() {
        use std::time::{Duration, UNIX_EPOCH};
        let it = |name: &str, dir: bool, t: u64| super::Item {
            path: name.into(),
            name: name.into(),
            dir,
            size: 1,
            modified: Some(UNIX_EPOCH + Duration::from_secs(t)),
            rel: None,
            mime: None,
        };
        let mut v = vec![
            it("old dir", true, 1),
            it("new.msix", false, 9),
            it("mid", true, 5),
        ];
        super::sort(
            &mut v,
            super::Sort {
                by: super::By::Modified,
                desc: true,
            },
        );
        assert_eq!(v[0].name, "new.msix");
        super::sort(&mut v, super::Sort::default());
        assert!(v[0].dir && v[1].dir && !v[2].dir);
    }

    /// An upload's filters as Chromium sends them ("Custom files": types and patterns,
    /// chosen first by `current_filter`): every file of those types shows, whatever the
    /// case of its name; others only under "All files".
    #[test]
    fn upload_filters_show_what_the_app_takes() {
        use crate::services::filechooser::{Filter, Request};
        if crate::services::open::type_by_name("a.png").is_none() {
            return; // no shared MIME database here
        }
        let file = |name: &str| super::Item {
            path: name.into(),
            name: name.into(),
            dir: false,
            size: 1,
            modified: None,
            rel: None,
            mime: crate::services::open::type_by_name(name),
        };
        let mut st = super::State {
            req: Some(Request {
                filters: vec![Filter {
                    name: "Custom files".into(),
                    globs: vec!["*.pdf".into()],
                    mimes: vec!["image/*".into(), "application/pdf".into()],
                }],
                current: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        st.filter = 0;
        for shown in [
            "IMG_0001.HEIC",
            "photo.webp",
            "scan.JPG",
            "Report.PDF",
            "a.avif",
        ] {
            assert!(super::folder::passes(&st, &file(shown)), "{shown} hidden");
        }
        for hidden in ["notes.txt", "song.mp3"] {
            assert!(!super::folder::passes(&st, &file(hidden)), "{hidden} shown");
        }
        // "All files": past the app's own.
        st.filter = 1;
        assert!(super::folder::passes(&st, &file("song.mp3")));
    }

    /// Picking several, as GTK and macOS do: a click picks one, Ctrl adds, Shift picks a run,
    /// and another folder (or a search) starts with nothing picked: a file picked in one
    /// folder was sent along with the next folder's.
    #[test]
    fn picks_belong_to_the_list_shown() {
        use crate::services::filechooser::Request;
        use iced::keyboard::Modifiers;
        let dir = std::env::temp_dir().join(format!("kestrel-pick-{}", std::process::id()));
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        for f in ["a.txt", "b.txt", "c.txt", "d.txt"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        std::fs::write(other.join("z.txt"), "").unwrap();
        let mut st = super::State {
            req: Some(Request {
                multiple: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        super::folder::load(&mut st, dir.clone());
        let at = |st: &super::State, n: &str| st.items.iter().position(|i| i.name == n).unwrap();
        let names = |st: &super::State| {
            let mut v: Vec<String> = st
                .picked
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        let click = |st: &mut super::State, n: &str, m: Modifiers| {
            let _ = super::update(st, super::Message::Mods(m));
            let i = at(st, n);
            // Apart in time: not a double click.
            st.click = None;
            let _ = super::update(st, super::Message::Click(i));
        };
        click(&mut st, "a.txt", Modifiers::empty());
        click(&mut st, "b.txt", Modifiers::empty());
        assert_eq!(names(&st), ["b.txt"], "a plain click picks only that file");
        click(&mut st, "d.txt", Modifiers::CTRL);
        assert_eq!(names(&st), ["b.txt", "d.txt"], "Ctrl adds");
        click(&mut st, "d.txt", Modifiers::CTRL);
        assert_eq!(names(&st), ["b.txt"], "Ctrl again takes it away");
        click(&mut st, "a.txt", Modifiers::empty());
        click(&mut st, "c.txt", Modifiers::SHIFT);
        assert_eq!(
            names(&st),
            ["a.txt", "b.txt", "c.txt"],
            "Shift picks the run"
        );
        // Into another folder: nothing carried over.
        super::folder::load(&mut st, other.clone());
        assert!(
            st.picked.is_empty(),
            "another folder starts with nothing picked"
        );
        click(&mut st, "z.txt", Modifiers::empty());
        assert_eq!(names(&st), ["z.txt"]);
        // A search starts a new list too.
        let _ = super::update(&mut st, super::Message::Query("z".into()));
        assert!(st.picked.is_empty(), "a search starts with nothing picked");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
