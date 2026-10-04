//! Launcher: type to find an app. Prefixes switch mode: `=` maths, `>` run a command,
//! `:` clipboard history, `.` emoji, `?` web search, `/` files, `!` running processes.

mod calc;
mod search;
mod update;
mod view;
pub use calc::*;
pub use search::*;
pub use update::*;
pub use view::*;

use super::clipboard::{Clip, clip_copy, clip_list};
use super::cx::Cx;
use crate::services::compositor::Action;
use crate::services::{apps, icons};
use crate::theme::tok;
use crate::theme::{self, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, container};
use iced::{Alignment, Element, Length, Task};
use std::collections::HashMap;
use std::path::PathBuf;

const MAX_RESULTS: usize = 8;

#[derive(Default)]
pub struct State {
    pub query: String,
    pub selected: usize,
    /// Where the selection highlight is drawn (in rows).
    pub glide: crate::motion::Spring,
    /// The arrows have been used since opening: until then (and while nothing is typed) no
    /// row is highlighted.
    touched: bool,
    /// The pointer chose the selection (so leaving the rows hides it again).
    by_pointer: bool,
    apps: Vec<apps::App>,
    /// Launch counts, for ordering.
    counts: HashMap<String, u32>,
    /// Clipboard history, loaded when `:` is typed.
    clips: Option<Vec<Clip>>,
    /// File search results for the query they were found for.
    files: (String, Vec<PathBuf>),
    /// The query's first word is a program on PATH (offered as a command to run).
    exe: bool,
    /// A process asked to end once (the next press on it ends it, as the monitor asks).
    armed: Option<u32>,
}

const EMOJI: &str = include_str!("../../../assets/emoji.txt");
const SEARCH_URL: &str = "https://duckduckgo.com/?q=";

#[derive(Clone, Debug)]
pub enum Message {
    /// The pointer left the rows.
    LeaveRows,
    /// The pointer came onto a row: it becomes the selection.
    Hover(usize),
    Query(String),
    Move(i32),
    Submit,
    Launch(usize),
    Clips(Vec<Clip>),
    Files(String, Vec<PathBuf>),
    /// The installed apps, read in the background.
    Apps(Vec<apps::App>),
}

/// Reads the installed apps off the UI thread (every .desktop file: ~30 ms).
pub fn reload() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(apps::load)
                .await
                .unwrap_or_default()
        },
        Message::Apps,
    )
}

pub enum Entry<'a> {
    App(&'a apps::App),
    Calc(String),
    Run(String),
    /// A clipboard history entry.
    Clip(&'a Clip),
    Emoji(&'a str, &'a str),
    Web(String),
    File(PathBuf),
    Process(u32, String, u64),
    /// A Settings page.
    Setting(crate::panels::settings::Page),
    /// A system switch to flip (Wi-Fi, Do not disturb, night light…).
    Switch(Switch),
}

/// What the launcher asks of the shell (`Action` here is the compositor's).
type Act = super::action::Action<Message>;

pub(super) static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("launcher-input"));

fn load_counts() -> HashMap<String, u32> {
    crate::state::get(|s| {
        s.launcher
            .launches
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    })
}

fn save_counts(c: &HashMap<String, u32>) {
    crate::state::update(|s| {
        s.launcher.launches = c.iter().map(|(k, v)| (k.clone(), *v)).collect()
    });
}

pub fn open(st: &mut State) -> Act {
    st.query.clear();
    st.selected = 0;
    st.touched = false;
    st.glide.snap(0.0);
    // The list it has shows at once; it is read again in the background (an app installed
    // since appears a moment later). Reading it here cost ~30 ms before the first frame.
    st.counts = load_counts();
    st.clips = None;
    st.files = (String::new(), Vec::new());
    Act::Run(Task::batch([
        Task::Iced(iced::widget::focus(INPUT_ID.clone())),
        reload(),
    ]))
}

// --- Web, files, processes -------------------------------------------------------------------

// --- Calculator ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::calc;

    #[test]
    fn maths() {
        assert_eq!(calc("2+2*3").as_deref(), Some("8"));
        assert_eq!(calc("(2+2)*3").as_deref(), Some("12"));
        assert_eq!(calc("2^10").as_deref(), Some("1024"));
        assert_eq!(calc("1/4").as_deref(), Some("0.25"));
        assert_eq!(calc("-3+1").as_deref(), Some("-2"));
        assert_eq!(calc("2+").as_deref(), None);
        assert_eq!(calc("abc"), None);
    }
}

#[cfg(test)]
mod path_tests {
    use super::pasted_path;
    use crate::services::open::percent_decode;

    #[test]
    fn pasted_paths_open() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            pasted_path("/tmp").as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert_eq!(
            pasted_path("'/tmp'").as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert_eq!(
            pasted_path("file:///tmp").as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert_eq!(pasted_path("~").unwrap(), std::path::PathBuf::from(&home));
        assert!(pasted_path("/no/such/thing/here").is_none());
        assert!(pasted_path("firefox").is_none());
        assert_eq!(percent_decode("/a%20b/c%25"), "/a b/c%");
    }
}
