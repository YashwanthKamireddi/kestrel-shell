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
mod search_tests {
    use super::*;

    fn app(id: &str, name: &str, generic: &str, keywords: &str, exec: &str) -> apps::App {
        apps::App {
            id: id.into(),
            name: name.into(),
            generic: generic.into(),
            comment: String::new(),
            icon: String::new(),
            exec: exec.into(),
            keywords: keywords.into(),
            terminal: false,
            wm_class: String::new(),
            mime: String::new(),
        }
    }

    fn with_apps() -> State {
        State {
            apps: vec![
                app(
                    "firefox.desktop",
                    "Firefox",
                    "Web Browser",
                    "internet;www",
                    "firefox %u",
                ),
                app(
                    "org.kde.dolphin.desktop",
                    "Dolphin",
                    "File Manager",
                    "files;folders",
                    "dolphin",
                ),
                app(
                    "kitty.desktop",
                    "kitty",
                    "Terminal",
                    "shell;prompt",
                    "/usr/bin/kitty",
                ),
                app(
                    "code.desktop",
                    "Visual Studio Code",
                    "Text Editor",
                    "vscode",
                    "code",
                ),
                app("fire.desktop", "Fire Alarm", "", "", "fire"),
            ],
            ..State::default()
        }
    }

    fn query(st: &mut State, q: &str) {
        st.query = q.into();
    }

    fn names(st: &State) -> Vec<String> {
        entries(st)
            .iter()
            .map(|e| match e {
                Entry::App(a) => format!("app:{}", a.name),
                Entry::Calc(v) => format!("calc:{v}"),
                Entry::Run(c) => format!("run:{c}"),
                Entry::Web(t) => format!("web:{t}"),
                Entry::Clip(c) => format!("clip:{}", c.preview),
                Entry::Emoji(e, _) => format!("emoji:{e}"),
                Entry::File(p) => format!("file:{}", p.display()),
                Entry::Process(_, n, _) => format!("process:{n}"),
                Entry::Setting(p) => format!("setting:{}", p.name()),
                Entry::Switch(w) => format!("switch:{}", w.name()),
            })
            .collect()
    }

    #[test]
    fn nothing_typed_lists_the_most_used_apps_first() {
        let mut st = with_apps();
        st.counts.insert("kitty.desktop".into(), 9);
        st.counts.insert("code.desktop".into(), 3);
        let n = names(&st);
        assert_eq!(n[0], "app:kitty");
        assert_eq!(n[1], "app:Visual Studio Code");
        assert!(n.len() <= MAX_RESULTS);
        assert!(
            !n.iter().any(|e| e.starts_with("web:")),
            "no web search for nothing"
        );
    }

    #[test]
    fn a_name_prefix_ranks_first() {
        let mut st = with_apps();
        query(&mut st, "fire");
        let n = names(&st);
        // Both start with "fire"; the shorter name is closer to what was typed.
        assert_eq!(n[0], "app:Firefox");
        assert_eq!(n[1], "app:Fire Alarm");
    }

    #[test]
    fn launches_lift_an_app_over_a_slightly_better_match() {
        let mut st = with_apps();
        st.counts.insert("fire.desktop".into(), 20);
        query(&mut st, "fire");
        assert_eq!(names(&st)[0], "app:Fire Alarm");
    }

    #[test]
    fn letters_in_order_find_an_app_by_its_name() {
        let mut st = with_apps();
        query(&mut st, "vsc");
        assert_eq!(names(&st)[0], "app:Visual Studio Code");
    }

    #[test]
    fn generic_names_and_keywords_find_apps() {
        let mut st = with_apps();
        query(&mut st, "browser");
        assert_eq!(names(&st)[0], "app:Firefox");
        query(&mut st, "folders");
        assert_eq!(names(&st)[0], "app:Dolphin");
    }

    #[test]
    fn keywords_need_the_text_as_typed() {
        let mut st = with_apps();
        // Letters scattered through "shell;prompt" are no match for a keyword.
        query(&mut st, "spt");
        assert!(!names(&st).contains(&"app:kitty".to_string()));
    }

    #[test]
    fn a_query_that_finds_nothing_still_offers_the_web() {
        let mut st = with_apps();
        query(&mut st, "zzqqxx");
        assert_eq!(names(&st), ["web:zzqqxx"]);
    }

    #[test]
    fn a_program_on_path_is_offered_unless_an_app_runs_it() {
        let mut st = with_apps();
        st.exe = true;
        query(&mut st, "htop -d 5");
        let n = names(&st);
        assert_eq!(n[n.len() - 2], "run:htop -d 5");
        assert_eq!(n[n.len() - 1], "web:htop -d 5");
        // "kitty" is an app whose program is kitty (by its full path): not offered twice.
        query(&mut st, "kitty");
        let n = names(&st);
        assert_eq!(n[0], "app:kitty");
        assert!(!n.iter().any(|e| e.starts_with("run:")), "{n:?}");
    }

    #[test]
    fn results_never_pass_the_limit() {
        let mut st = State {
            apps: (0..30)
                .map(|i| app(&format!("a{i}"), &format!("Tool {i}"), "", "", "x"))
                .collect(),
            exe: true,
            ..State::default()
        };
        query(&mut st, "tool");
        let n = names(&st);
        assert_eq!(n.len(), MAX_RESULTS);
        assert_eq!(n.last().map(String::as_str), Some("web:tool"));
    }

    #[test]
    fn a_bare_sum_is_answered_on_top() {
        let mut st = with_apps();
        query(&mut st, "2+2");
        assert_eq!(names(&st)[0], "calc:4");
        query(&mut st, "(1+2)*3");
        assert_eq!(names(&st)[0], "calc:9");
        // A number that is no sum: no answer row.
        query(&mut st, "2+");
        assert!(!names(&st).iter().any(|e| e.starts_with("calc:")));
    }

    #[test]
    fn prefixes_switch_what_is_searched() {
        let mut st = with_apps();
        query(&mut st, "=2*21");
        assert_eq!(names(&st), ["calc:42"]);
        query(&mut st, "=2*");
        assert!(names(&st).is_empty());
        query(&mut st, "? rust lifetimes ");
        assert_eq!(names(&st), ["web:rust lifetimes"]);
        query(&mut st, "?");
        assert!(names(&st).is_empty());
        query(&mut st, "> make check");
        assert_eq!(names(&st), ["run:make check"]);
        query(&mut st, ">");
        assert!(names(&st).is_empty());
    }

    #[test]
    fn emoji_words_match_without_their_last_letter() {
        let mut st = State::default();
        query(&mut st, ".smile");
        let n = names(&st);
        assert!(!n.is_empty());
        assert!(n.contains(&"emoji:🙂".to_string()), "{n:?}");
        assert!(n.len() <= MAX_RESULTS);
        query(&mut st, ".zzqqxx");
        assert!(names(&st).is_empty());
    }

    #[test]
    fn clipboard_entries_filter_by_their_text() {
        let mut st = State {
            clips: Some(crate::panels::clipboard::sample_clips(30)),
            ..State::default()
        };
        query(&mut st, ":");
        assert_eq!(names(&st).len(), MAX_RESULTS);
        query(&mut st, ": EXAMPLE.COM/page/3");
        let n = names(&st);
        assert!(!n.is_empty());
        assert!(n.iter().all(|e| e.contains("example.com/page/3")), "{n:?}");
        query(&mut st, ":nothing like this");
        assert!(names(&st).is_empty());
    }

    #[test]
    fn file_results_only_for_the_query_they_were_found_for() {
        let mut st = State::default();
        let _ = update(&mut st, Message::Query("/notes".into()));
        let _ = update(
            &mut st,
            Message::Files("/old".into(), vec![PathBuf::from("/x/old.txt")]),
        );
        assert!(
            names(&st).is_empty(),
            "an older search's results are dropped"
        );
        let _ = update(
            &mut st,
            Message::Files("/notes".into(), vec![PathBuf::from("/x/notes.md")]),
        );
        assert_eq!(names(&st), ["file:/x/notes.md"]);
        // Typing on: the results go with the query they were for.
        st.query = "/notesx".into();
        assert!(names(&st).is_empty());
    }

    #[test]
    fn settings_pages_and_switches_are_found_by_everyday_words() {
        let mut st = State::default();
        query(&mut st, "wifi");
        let n = names(&st);
        assert!(n.contains(&"switch:Wi-Fi".to_string()), "{n:?}");
        query(&mut st, "w");
        let n = names(&st);
        assert!(
            !n.iter()
                .any(|e| e.starts_with("setting:") || e.starts_with("switch:")),
            "one letter matches too much: {n:?}"
        );
    }

    #[test]
    fn the_first_arrow_press_shows_where_you_are() {
        let mut st = with_apps();
        let _ = update(&mut st, Message::Move(1));
        assert_eq!(st.selected, 0, "down: the top");
        let mut st = with_apps();
        let _ = update(&mut st, Message::Move(-1));
        assert_eq!(st.selected, entries(&st).len() - 1, "up: the bottom");
        // Then the arrows move, and wrap around.
        let _ = update(&mut st, Message::Move(1));
        assert_eq!(st.selected, 0);
        let _ = update(&mut st, Message::Move(-1));
        assert_eq!(st.selected, entries(&st).len() - 1);
    }

    #[test]
    fn typing_resets_the_selection() {
        let mut st = with_apps();
        st.selected = 3;
        let _ = update(&mut st, Message::Query("fi".into()));
        assert_eq!(st.selected, 0);
        // With text typed, the first press already moves.
        let _ = update(&mut st, Message::Move(1));
        assert_eq!(st.selected, 1);
    }

    #[test]
    fn arrows_on_an_empty_list_do_nothing() {
        let mut st = State {
            query: "?".into(),
            ..State::default()
        };
        let _ = update(&mut st, Message::Move(1));
        assert_eq!(st.selected, 0);
    }

    #[test]
    fn enter_on_a_web_search_opens_it_encoded() {
        let mut st = with_apps();
        let _ = update(&mut st, Message::Query("?c++ & rust".into()));
        match update(&mut st, Message::Submit) {
            Act::Many(v) => match v.as_slice() {
                [Act::Dispatch(Action::Open(url)), Act::Close] => {
                    assert_eq!(url, "https://duckduckgo.com/?q=c%2B%2B+%26+rust");
                }
                _ => panic!("not an open and close"),
            },
            _ => panic!("not an open and close"),
        }
    }

    #[test]
    fn enter_on_an_emoji_types_it() {
        let mut st = State::default();
        let _ = update(&mut st, Message::Query(".slightly smiling".into()));
        match update(&mut st, Message::Submit) {
            Act::Many(v) => assert!(matches!(
                v.as_slice(),
                [Act::Type(e), Act::Close] if e == "🙂"
            )),
            _ => panic!("not typed"),
        }
    }

    #[test]
    fn enter_with_nothing_found_does_nothing() {
        let mut st = State::default();
        let _ = update(&mut st, Message::Query("?".into()));
        assert!(matches!(update(&mut st, Message::Submit), Act::None));
    }

    #[test]
    fn urls_encode_everything_but_the_safe_letters() {
        assert_eq!(encode("a b"), "a+b");
        assert_eq!(encode("Az09-_.~"), "Az09-_.~");
        assert_eq!(encode("é/?"), "%C3%A9%2F%3F");
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
