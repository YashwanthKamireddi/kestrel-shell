//! System monitor, with DMS's process manager in kestrel's look (Modules/ProcessList,
//! ProcessesView.qml): every process, searchable, filtered (All / Mine / System), sorted by
//! name, CPU or memory, each app's processes under it (a terminal holds its shell and what runs
//! there); right-click or Enter for what can be done to one (quit, force quit, end with its
//! children, pause, priority, copy). Keyboard: type to search, ↑↓ move, Enter its menu, Delete
//! quits (asked once). Live every 1.5 s while open, nothing when closed. Opened from the
//! dashboard's gauges or `kestrel toggle system`.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::about::About;
use crate::services::disks::{Drive, Volume};
use crate::services::system::{Detail, Proc};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, row, text_input};
use iced::{Alignment, Element, Length, Task};
use std::collections::{BTreeSet, HashMap};

pub const WIDTH: f32 = 560.0;
/// A process row's height.
const ROW: f32 = tok::H_CONTROL;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Mine,
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    Name,
    #[default]
    Cpu,
    Memory,
}

#[derive(Default)]
pub struct State {
    pub detail: Option<Detail>,
    query: String,
    filter: Filter,
    sort: Sort,
    /// Smallest first (names A–Z; numbers are biggest first by default).
    ascending: bool,
    /// Processes shown with their children.
    open: BTreeSet<u32>,
    /// The highlighted row (keyboard and pointer).
    selected: Option<u32>,
    /// The process whose menu is open.
    menu: Option<u32>,
    /// The process whose Quit was pressed once.
    armed: Option<u32>,
    tab: Tab,
    /// The last minute of readings, oldest first (Performance's graphs).
    history: std::collections::VecDeque<Sample>,
    /// Disks: every drive with its health, and the filesystems on them.
    drives: Option<(Vec<Drive>, Vec<Volume>)>,
    /// System: what this computer is (read when the page is shown).
    about: Option<About>,
}

/// The monitor's pages (DMS's process manager views).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Processes,
    Performance,
    Disks,
    System,
}

/// One reading for the graphs.
#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    cpu: f32,
    mem: f32,
    gpu: f32,
    disk: (u64, u64),
    net: (u64, u64),
}

/// Samples the graphs hold: a minute at one per 1.5 s.
const SPAN: usize = 40;

#[derive(Clone, Copy, Debug)]
pub enum Act {
    /// SIGTERM: asks it to close.
    Quit,
    /// SIGKILL: ends it at once.
    Force,
    /// SIGTERM to it and everything it started.
    Tree,
    /// SIGSTOP / SIGCONT.
    Pause(bool),
    /// A nice value: -10 high, 0 normal, 10 low (raising priority needs rights the user lacks:
    /// those fail and say so).
    Priority(i32),
    CopyName,
    CopyPid,
    CopyCommand,
}

#[derive(Clone, Debug)]
pub enum Message {
    Detail(Detail),
    Query(String),
    Filter(Filter),
    Sort(Sort),
    Open(u32, bool),
    Select(u32),
    Menu(Option<u32>),
    Act(u32, Act),
    Tab(Tab),
    Drives(Vec<Drive>, Vec<Volume>),
    About(About),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Monitor(m))
}

static INPUT_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("monitor-input"));

/// Opened: a fresh search, the field ready for typing.
pub fn open(app: &mut App) -> Task<AppMessage> {
    let st = &mut app.panels.monitor;
    st.query.clear();
    st.menu = None;
    st.armed = None;
    st.selected = None;
    Task::Iced(iced::widget::focus(INPUT_ID.clone()))
}

/// The session itself (the compositor, this shell, sound, the session bus): never ended from
/// here, ending one would end the desktop (DMS warns the same).
fn vital(name: &str) -> bool {
    matches!(
        name,
        "Hyprland"
            | "kestrel"
            | "systemd"
            | "pipewire"
            | "pipewire-pulse"
            | "wireplumber"
            | "dbus-broker"
            | "dbus-daemon"
            | "Xwayland"
            | "xdg-desktop-portal"
            | "xdg-desktop-portal-hyprland"
    )
}

fn signal(pid: u32, sig: i32) -> bool {
    // Safety: kill(2) on a process id; the kernel checks the user may signal it.
    unsafe { libc::kill(pid as i32, sig) == 0 }
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.monitor;
    match m {
        Message::Detail(d) => {
            let cpu = if d.cores.is_empty() {
                0.0
            } else {
                d.cores.iter().map(|c| f32::from(*c)).sum::<f32>() / d.cores.len() as f32 / 100.0
            };
            st.history.push_back(Sample {
                cpu,
                mem: d.mem.0 as f32 / d.mem.1.max(1) as f32,
                gpu: d
                    .gpu_mhz
                    .map_or(0.0, |(now, max)| now as f32 / max.max(1) as f32),
                disk: d.disk_io,
                net: d.net_io,
            });
            while st.history.len() > SPAN {
                st.history.pop_front();
            }
            st.detail = Some(d);
        }
        Message::Tab(t) => {
            st.tab = t;
            st.menu = None;
            match t {
                Tab::Disks => {
                    return Task::perform(
                        async {
                            let (d, v) = futures::join!(
                                crate::services::disks::drives(),
                                crate::services::disks::list()
                            );
                            (d, v)
                        },
                        |(d, v)| msg(Message::Drives(d, v)),
                    );
                }
                Tab::System if st.about.is_none() => {
                    return Task::perform(
                        async {
                            tokio::task::spawn_blocking(crate::services::about::read_all)
                                .await
                                .unwrap_or_default()
                        },
                        |a| msg(Message::About(a)),
                    );
                }
                _ => {}
            }
        }
        Message::Drives(d, v) => st.drives = Some((d, v)),
        Message::About(a) => st.about = Some(a),
        Message::Query(q) => {
            st.query = q;
            st.menu = None;
        }
        Message::Filter(f) => st.filter = f,
        Message::Sort(k) => {
            if st.sort == k {
                st.ascending = !st.ascending;
            } else {
                st.sort = k;
                st.ascending = k == Sort::Name;
            }
        }
        Message::Open(pid, on) => {
            if on {
                st.open.insert(pid);
            } else {
                st.open.remove(&pid);
            }
        }
        Message::Select(pid) => st.selected = Some(pid),
        Message::Menu(m) => {
            st.menu = m;
            st.armed = None;
            if let Some(pid) = m {
                st.selected = Some(pid);
            }
        }
        Message::Act(pid, act) => return act_on(app, pid, act),
    }
    Task::none()
}

fn act_on(app: &mut App, pid: u32, act: Act) -> Task<AppMessage> {
    let st = &mut app.panels.monitor;
    let Some(p) = st
        .detail
        .as_ref()
        .and_then(|d| d.procs.iter().find(|p| p.pid == pid))
        .cloned()
    else {
        return Task::none();
    };
    let copy = |t: String| Task::Iced(iced::clipboard::write(t));
    match act {
        Act::Quit if st.armed != Some(pid) => {
            st.armed = Some(pid);
            return Task::none();
        }
        Act::Quit => {
            signal(pid, libc::SIGTERM);
        }
        Act::Force => {
            signal(pid, libc::SIGKILL);
        }
        Act::Tree => {
            if let Some(d) = &st.detail {
                for child in descendants(&d.procs, pid) {
                    signal(child, libc::SIGTERM);
                }
            }
            signal(pid, libc::SIGTERM);
        }
        Act::Pause(on) => {
            signal(pid, if on { libc::SIGSTOP } else { libc::SIGCONT });
        }
        Act::Priority(nice) => {
            // Safety: setpriority(2) on one process; refused (EACCES) without the right.
            let ok = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid, nice) } == 0;
            if !ok {
                log::info!("monitor: priority {nice} for {pid} refused");
            }
        }
        Act::CopyName => return copy(p.name),
        Act::CopyPid => return copy(pid.to_string()),
        Act::CopyCommand => return copy(p.command),
    }
    st.armed = None;
    st.menu = None;
    Task::none()
}

/// Every process `pid` started, and theirs.
fn descendants(all: &[Proc], pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut todo = vec![pid];
    while let Some(p) = todo.pop() {
        for c in all.iter().filter(|c| c.ppid == p) {
            out.push(c.pid);
            todo.push(c.pid);
        }
    }
    out
}

/// A row: a process, and its children folded under it (their CPU and memory added in).
struct Node<'a> {
    p: &'a Proc,
    kids: Vec<Node<'a>>,
    cpu: f32,
    rss: u64,
}

/// What starts other programs without being their app: init, the session and display
/// managers, the compositor and this shell (they launch your apps). Their children are rows of
/// their own; everything else holds what it started (a terminal its shell, a browser its tabs).
fn starter(name: &str) -> bool {
    matches!(
        name,
        "systemd"
            | "init"
            | "ly-dm"
            | "ly"
            | "login"
            | "greetd"
            | "sddm"
            | "gdm"
            | "uwsm"
            | "start-hyprland"
            | "Hyprland"
            | "kestrel"
    )
}

/// The shown processes as a tree, one row per app (see `starter`).
fn tree<'a>(shown: &[&'a Proc]) -> Vec<Node<'a>> {
    let by_pid: HashMap<u32, &Proc> = shown.iter().map(|p| (p.pid, *p)).collect();
    let root = |p: &Proc| {
        starter(&p.name)
            || p.ppid <= 1
            || by_pid
                .get(&p.ppid)
                .is_none_or(|parent| starter(&parent.name))
    };
    fn build<'a>(p: &'a Proc, shown: &[&'a Proc]) -> Node<'a> {
        // A starter's children are rows of their own, not under it.
        let kids: Vec<Node<'a>> = if starter(&p.name) {
            Vec::new()
        } else {
            shown
                .iter()
                .filter(|c| c.ppid == p.pid && c.pid != p.pid && !starter(&c.name))
                .map(|c| build(c, shown))
                .collect()
        };
        let cpu = p.cpu + kids.iter().map(|k| k.cpu).sum::<f32>();
        let rss = p.rss + kids.iter().map(|k| k.rss).sum::<u64>();
        Node { p, kids, cpu, rss }
    }
    shown
        .iter()
        .filter(|p| root(p))
        .map(|p| build(p, shown))
        .collect()
}

/// The rows before sorting: the tree, or while searching every match on its own.
fn nodes_of<'a>(st: &State, list: &[&'a Proc]) -> Vec<Node<'a>> {
    if st.query.trim().is_empty() {
        tree(list)
    } else {
        list.iter()
            .map(|p| Node {
                p,
                kids: Vec::new(),
                cpu: p.cpu,
                rss: p.rss,
            })
            .collect()
    }
}

fn sort_nodes(nodes: &mut [Node<'_>], sort: Sort, ascending: bool) {
    nodes.sort_by(|a, b| {
        let o = match sort {
            Sort::Name => a.p.name.to_lowercase().cmp(&b.p.name.to_lowercase()),
            Sort::Cpu => a.cpu.total_cmp(&b.cpu),
            Sort::Memory => a.rss.cmp(&b.rss),
        };
        if ascending { o } else { o.reverse() }
    });
    for n in nodes {
        sort_nodes(&mut n.kids, sort, ascending);
    }
}

/// The rows as listed now, in order (folded ones without their children): the keyboard moves
/// through these.
fn flatten<'a>(
    nodes: &'a [Node<'a>],
    open: &BTreeSet<u32>,
    depth: usize,
    out: &mut Vec<(&'a Node<'a>, usize)>,
) {
    for n in nodes {
        out.push((n, depth));
        if open.contains(&n.p.pid) {
            flatten(&n.kids, open, depth + 1, out);
        }
    }
}

/// What the list shows: the filter, then the search (matching ones listed flat, no tree).
fn shown<'a>(st: &State, d: &'a Detail) -> Vec<&'a Proc> {
    let q = st.query.trim().to_lowercase();
    d.procs
        .iter()
        // Kernel threads (no command line, children of kthreadd) are not programs.
        .filter(|p| !p.command.is_empty() || p.ppid > 2)
        .filter(|p| match st.filter {
            Filter::All => true,
            Filter::Mine => p.mine,
            Filter::System => !p.mine,
        })
        .filter(|p| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.command.to_lowercase().contains(&q)
                || p.pid.to_string() == q
        })
        .collect()
}

pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    let st = &app.panels.monitor;
    let Some(d) = &st.detail else {
        return Task::none();
    };
    let list = shown(st, d);
    let mut nodes = nodes_of(st, &list);
    sort_nodes(&mut nodes, st.sort, st.ascending);
    let mut flat = Vec::new();
    flatten(&nodes, &st.open, 0, &mut flat);
    if flat.is_empty() {
        return Task::none();
    }
    let at = st
        .selected
        .and_then(|s| flat.iter().position(|(n, _)| n.p.pid == s));
    let pick = |i: usize| flat[i.min(flat.len() - 1)].0.p.pid;
    let m = match k {
        Key::Down => Message::Select(pick(at.map_or(0, |i| i + 1))),
        Key::Up => Message::Select(pick(at.map_or(0, |i| i.saturating_sub(1)))),
        Key::Home => Message::Select(pick(0)),
        Key::End => Message::Select(pick(flat.len() - 1)),
        Key::Right => match at {
            Some(i) => Message::Open(flat[i].0.p.pid, true),
            None => return Task::none(),
        },
        Key::Left => match at {
            Some(i) => Message::Open(flat[i].0.p.pid, false),
            None => return Task::none(),
        },
        Key::Enter => match at {
            Some(i) => {
                let pid = flat[i].0.p.pid;
                Message::Menu(if st.menu == Some(pid) {
                    None
                } else {
                    Some(pid)
                })
            }
            None => return Task::none(),
        },
        Key::Delete => match at {
            Some(i) if flat[i].0.p.mine && !vital(&flat[i].0.p.name) => {
                Message::Act(flat[i].0.p.pid, Act::Quit)
            }
            _ => return Task::none(),
        },
        _ => return Task::none(),
    };
    update(app, m)
}

fn gib(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

fn mib(b: u64) -> String {
    if b >= 1_000_000_000 {
        format!("{:.1} GB", b as f64 / 1e9)
    } else {
        format!("{:.0} MB", b as f64 / 1e6)
    }
}

/// A thin bar, `v` of 0..1 filled.
fn bar<'a>(s: Scheme, v: f32, h: f32) -> Element<'a, AppMessage> {
    let v = v.clamp(0.0, 1.0);
    let fill = container(Space::new())
        .width(Length::FillPortion((v * 1000.0) as u16))
        .height(h)
        .style(move |_| container::Style {
            background: Some(s.accent.into()),
            border: iced::Border::default().rounded(h / 2.0),
            ..container::Style::default()
        });
    let rest = Space::new().width(Length::FillPortion(((1.0 - v) * 1000.0) as u16));
    container(row![fill, rest])
        .width(Length::Fill)
        .height(h)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
            border: iced::Border::default().rounded(h / 2.0),
            ..container::Style::default()
        })
        .into()
}

/// A column title that sorts by it (again: the other way).
fn sorter<'a>(
    s: Scheme,
    st: &State,
    k: Sort,
    name: &'static str,
    w: Length,
) -> Element<'a, AppMessage> {
    let on = st.sort == k;
    let mut r = row![label_w(
        name,
        text::CAPTION,
        if on { s.ink } else { s.ink_faint },
        theme::MEDIUM
    )]
    .spacing(tok::XXS)
    .align_y(Alignment::Center);
    if on {
        r = r.push(icon(
            if st.ascending {
                Icon::ChevronUp
            } else {
                Icon::ChevronDown
            },
            12.0,
            s.ink,
        ));
    }
    control(r)
        .padding([tok::XXS, tok::S])
        .width(w)
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(msg(Message::Sort(k)))
        .into()
}

/// What can be done to `p` (DMS ProcessActions): under its row.
fn menu<'a>(s: Scheme, st: &State, p: &Proc) -> Element<'a, AppMessage> {
    let pid = p.pid;
    let item = |t: String, m: Option<Act>, danger: bool| -> Element<'a, AppMessage> {
        let color = match (m.is_some(), danger) {
            (false, _) => s.ink_faint,
            (true, true) => s.bad,
            (true, false) => s.ink,
        };
        let mut c = control(label(t, text::SMALL, color))
            .padding([tok::S, tok::L])
            .width(Length::Fill)
            .radius(tok::R_ROW)
            .kind(s, crate::ui::control::Kind::Ghost);
        if let Some(a) = m {
            c = c.on_press(msg(Message::Act(pid, a)));
        }
        c.into()
    };
    let can = p.mine && !vital(&p.name);
    let mut c = column![].spacing(tok::XXS);
    if vital(&p.name) {
        c = c.push(
            container(label(
                format!(
                    "{} keeps your desktop running: ending it would end your session.",
                    p.name
                ),
                text::SMALL,
                s.ink_dim,
            ))
            .padding([tok::S, tok::L]),
        );
    } else if !p.mine {
        c = c.push(
            container(label(
                "A system process: only the system can end it.",
                text::SMALL,
                s.ink_dim,
            ))
            .padding([tok::S, tok::L]),
        );
    }
    if can {
        let armed = st.armed == Some(pid);
        c = c
            .push(item(
                if armed {
                    "Quit? Press again".into()
                } else {
                    "Quit".into()
                },
                Some(Act::Quit),
                armed,
            ))
            .push(item("Force quit".into(), Some(Act::Force), true))
            .push(item(
                "End with everything it started".into(),
                Some(Act::Tree),
                true,
            ))
            .push(item(
                if p.paused {
                    "Resume".into()
                } else {
                    "Pause".into()
                },
                Some(Act::Pause(!p.paused)),
                false,
            ));
        // Priority: three steps, the current one marked.
        let step = |name: &'static str, nice: i32| -> Element<'a, AppMessage> {
            let on = (p.nice < -3 && nice < 0)
                || (p.nice > 3 && nice > 0)
                || ((-3..=3).contains(&p.nice) && nice == 0);
            control(label_w(
                name,
                text::SMALL,
                if on { s.on_accent } else { s.ink },
                theme::MEDIUM,
            ))
            .padding([tok::XS, tok::M])
            .radius(tok::FULL)
            .kind(
                s,
                if on {
                    crate::ui::control::Kind::Primary
                } else {
                    crate::ui::control::Kind::Tonal
                },
            )
            .on_press(msg(Message::Act(pid, Act::Priority(nice))))
            .into()
        };
        c = c.push(
            row![
                label("Priority", text::SMALL, s.ink).width(Length::Fill),
                step("High", -10),
                step("Normal", 0),
                step("Low", 10),
            ]
            .spacing(tok::XS)
            .align_y(Alignment::Center)
            .padding([tok::XS, tok::L]),
        );
    }
    c = c
        .push(item("Copy name".into(), Some(Act::CopyName), false))
        .push(item(format!("Copy PID ({pid})"), Some(Act::CopyPid), false))
        .push(item("Copy command".into(), Some(Act::CopyCommand), false));
    container(c)
        .padding(tok::XS)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            border: iced::Border::default().rounded(tok::R_CARD),
            ..container::Style::default()
        })
        .into()
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let st = &app.panels.monitor;
    let title = label_w("System", text::HEADING, s.ink, theme::SEMIBOLD);
    let Some(d) = &st.detail else {
        // The first reading is 1.5 s away.
        return container(
            column![title, crate::ui::waiting(s, Icon::Activity, "Reading…")].spacing(tok::L),
        )
        .padding(tok::XL)
        .into();
    };
    let tabs = ui::segments(
        s,
        &[
            (
                "Processes",
                st.tab == Tab::Processes,
                msg(Message::Tab(Tab::Processes)),
            ),
            (
                "Performance",
                st.tab == Tab::Performance,
                msg(Message::Tab(Tab::Performance)),
            ),
            ("Disks", st.tab == Tab::Disks, msg(Message::Tab(Tab::Disks))),
            (
                "System",
                st.tab == Tab::System,
                msg(Message::Tab(Tab::System)),
            ),
        ],
    );
    let page: Option<Element<'_, AppMessage>> = match st.tab {
        Tab::Processes => None,
        Tab::Performance => Some(performance(s, st, d)),
        Tab::Disks => Some(disks(s, st)),
        Tab::System => Some(system(s, st)),
    };
    if let Some(page) = page {
        return container(
            column![
                row![title.width(Length::Fill), tabs].align_y(Alignment::Center),
                page
            ]
            .spacing(tok::L),
        )
        .padding(tok::XL)
        .width(Length::Fill)
        .into();
    }
    // At a glance: the CPU (all cores) and memory.
    let cpu = if d.cores.is_empty() {
        0.0
    } else {
        d.cores.iter().map(|c| f32::from(*c)).sum::<f32>() / d.cores.len() as f32
    };
    let glance = |name: &'static str, value: String, v: f32| {
        column![
            row![
                label(name, text::SMALL, s.ink).width(Length::Fill),
                label(value, text::SMALL, s.ink_dim)
            ],
            bar(s, v, 6.0),
        ]
        .spacing(tok::S)
        .width(Length::Fill)
    };
    let summary = row![
        glance("CPU", format!("{cpu:.0}%"), cpu / 100.0),
        glance(
            "Memory",
            format!("{} of {}", gib(d.mem.0), gib(d.mem.1)),
            d.mem.0 as f32 / d.mem.1.max(1) as f32
        ),
    ]
    .spacing(tok::XL);

    // Search and the filter.
    let input = text_input("Search processes", &st.query)
        .id(INPUT_ID.clone())
        .on_input(|q| msg(Message::Query(q)))
        .padding([tok::M, tok::S])
        .size(text::BODY)
        .font(theme::FONT)
        .style(crate::ui::input_style(s));
    let field = container(
        row![icon(Icon::Search, 16.0, s.ink_dim), input]
            .spacing(tok::XS)
            .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::L])
    .height(tok::H_ROW)
    .center_y(tok::H_ROW)
    .width(Length::Fill)
    .style(move |_| ui::field_style(s));
    let chips = ui::segments(
        s,
        &[
            (
                "All",
                st.filter == Filter::All,
                msg(Message::Filter(Filter::All)),
            ),
            (
                "Mine",
                st.filter == Filter::Mine,
                msg(Message::Filter(Filter::Mine)),
            ),
            (
                "System",
                st.filter == Filter::System,
                msg(Message::Filter(Filter::System)),
            ),
        ],
    );
    let tools = row![field, chips]
        .spacing(tok::M)
        .align_y(Alignment::Center);

    // The list.
    let list = shown(st, d);
    let mut nodes = nodes_of(st, &list);
    sort_nodes(&mut nodes, st.sort, st.ascending);
    let mut flat = Vec::new();
    flatten(&nodes, &st.open, 0, &mut flat);
    let count = label(
        format!("{} processes", flat.len()),
        text::CAPTION,
        s.ink_faint,
    );
    let head = row![
        sorter(s, st, Sort::Name, "Name", Length::Fill),
        count,
        sorter(s, st, Sort::Cpu, "CPU", Length::Fixed(64.0)),
        sorter(s, st, Sort::Memory, "Memory", Length::Fixed(84.0)),
    ]
    .align_y(Alignment::Center);
    let mut rows = column![].spacing(tok::XXS);
    for (n, depth) in &flat {
        let p = n.p;
        let open = st.open.contains(&p.pid);
        let fold: Element<'_, AppMessage> = if n.kids.is_empty() {
            Space::new().width(20).into()
        } else {
            control(icon(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                12.0,
                s.ink_dim,
            ))
            .width(20)
            .height(20)
            .center_x()
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(msg(Message::Open(p.pid, !open)))
            .into()
        };
        let mut name = row![
            Space::new().width(*depth as f32 * tok::L),
            fold,
            ui::app_icon(s, &p.name, 16.0, Icon::Cpu),
            label(
                ui::ellipsize(&p.name, 32),
                text::SMALL,
                if p.paused { s.ink_faint } else { s.ink }
            ),
        ]
        .spacing(tok::S)
        .align_y(Alignment::Center);
        if !n.kids.is_empty() && !open {
            name = name.push(label(
                format!("{}", n.kids.len() + 1),
                text::CAPTION,
                s.ink_faint,
            ));
        }
        if p.paused {
            name = name.push(label("paused", text::CAPTION, s.ink_faint));
        }
        let line = row![
            name.width(Length::Fill),
            container(label(format!("{:.1}%", n.cpu), text::SMALL, s.ink_dim))
                .width(64)
                .align_right(64),
            container(label(mib(n.rss), text::SMALL, s.ink_dim))
                .width(84)
                .align_right(84),
        ]
        .align_y(Alignment::Center)
        .padding([tok::NONE, tok::S]);
        let menu_open = st.menu == Some(p.pid);
        rows = rows.push(
            control(line)
                .identity(u64::from(p.pid))
                .height(ROW)
                .width(Length::Fill)
                .radius(tok::R_ROW)
                .kind(
                    s,
                    if st.selected == Some(p.pid) {
                        crate::ui::control::Kind::Chosen
                    } else {
                        crate::ui::control::Kind::Ghost
                    },
                )
                .on_press(msg(Message::Select(p.pid)))
                .on_right_press(msg(Message::Menu(if menu_open {
                    None
                } else {
                    Some(p.pid)
                }))),
        );
        if menu_open {
            rows = rows.push(menu(s, st, p));
        }
    }
    let body: Element<'_, AppMessage> = if flat.is_empty() {
        ui::empty_reason(s, "No process matches.".into(), 120.0)
    } else {
        let room = app
            .panel
            .as_ref()
            .and_then(|p| app.output(p.output))
            .map_or(480.0, |o| o.size.height - size::BAR - 360.0);
        container(crate::ui::smooth(
            iced::widget::scrollable(rows)
                .height(Length::Shrink)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new()
                        .width(0)
                        .scroller_width(0),
                )),
        ))
        .max_height(room.max(240.0))
        .into()
    };
    container(
        column![
            row![title.width(Length::Fill), tabs].align_y(Alignment::Center),
            crate::ui::card(s, container(summary).padding([tok::XS, tok::XXS])),
            tools,
            head,
            body,
        ]
        .spacing(tok::L),
    )
    .padding(tok::XL)
    .width(Length::Fill)
    .into()
}

/// Bytes per second as people read them.
fn rate(b: u64) -> String {
    let b = b as f64;
    if b >= 1e6 {
        format!("{:.1} MB/s", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB/s", b / 1e3)
    } else {
        format!("{b:.0} B/s")
    }
}

/// A Performance card: its name, what it reads now, the last minute as a graph.
fn chart<'a>(
    s: Scheme,
    name: &'static str,
    now: String,
    lines: Vec<(Vec<f32>, iced::Color)>,
) -> Element<'a, AppMessage> {
    crate::ui::card(
        s,
        column![
            row![
                label_w(name, text::SMALL, s.ink, theme::MEDIUM).width(Length::Fill),
                label(now, text::SMALL, s.ink_dim),
            ],
            crate::ui::graph::graph(lines, SPAN, 56.0),
        ]
        .spacing(tok::S)
        .padding([tok::XS, tok::XXS])
        .width(Length::Fill),
    )
}

/// Performance (DMS PerformanceView): CPU, memory, graphics, disk and network over the last
/// minute; each core's load under them.
fn performance<'a>(s: Scheme, st: &'a State, d: &'a Detail) -> Element<'a, AppMessage> {
    let h = &st.history;
    let series = |f: &dyn Fn(&Sample) -> f32| h.iter().map(f).collect::<Vec<f32>>();
    // Rates are drawn against the largest of the minute.
    let scaled = |f: &dyn Fn(&Sample) -> u64| {
        let top = h.iter().map(f).max().unwrap_or(0).max(1) as f32;
        h.iter().map(|x| f(x) as f32 / top).collect::<Vec<f32>>()
    };
    let last = h.back().copied().unwrap_or_default();
    let second = s.ink_dim;
    let cpu_now = match d.cpu_temp {
        Some(t) => format!("{:.0}% · {t:.0} °C", last.cpu * 100.0),
        None => format!("{:.0}%", last.cpu * 100.0),
    };
    let mut mem_now = format!("{} of {}", gib(d.mem.0), gib(d.mem.1));
    if d.swap.1 > 0 {
        mem_now.push_str(&format!(" · swap {}", gib(d.swap.0)));
    }
    let gpu: Element<'a, AppMessage> = match d.gpu_mhz {
        Some((now, max)) => chart(
            s,
            "Graphics clock",
            format!("{now} of {max} MHz"),
            vec![(series(&|x| x.gpu), s.accent)],
        ),
        None => crate::ui::card(
            s,
            container(label(
                "This graphics card does not report its load.",
                text::SMALL,
                s.ink_dim,
            ))
            .padding(tok::XS),
        ),
    };
    let grid = column![
        row![
            chart(s, "CPU", cpu_now, vec![(series(&|x| x.cpu), s.accent)]),
            chart(s, "Memory", mem_now, vec![(series(&|x| x.mem), s.accent)]),
        ]
        .spacing(tok::M),
        row![
            gpu,
            chart(
                s,
                "Disk",
                format!("↓ {}  ↑ {}", rate(last.disk.0), rate(last.disk.1)),
                vec![
                    (scaled(&|x| x.disk.0), s.accent),
                    (scaled(&|x| x.disk.1), second)
                ],
            ),
        ]
        .spacing(tok::M),
        chart(
            s,
            "Network",
            format!("↓ {}  ↑ {}", rate(last.net.0), rate(last.net.1)),
            vec![
                (scaled(&|x| x.net.0), s.accent),
                (scaled(&|x| x.net.1), second)
            ],
        ),
    ]
    .spacing(tok::M);
    // Each core: a bar, in two columns.
    let mut cores = column![].spacing(tok::S);
    for (k, c) in d.cores.chunks(2).enumerate() {
        let mut r = row![].spacing(tok::L);
        for (j, v) in c.iter().enumerate() {
            r = r.push(
                row![
                    container(label(
                        format!("{}", k * 2 + j + 1),
                        text::CAPTION,
                        s.ink_faint
                    ))
                    .width(16),
                    container(bar(s, f32::from(*v) / 100.0, 6.0)).width(Length::Fill),
                    container(label(format!("{v}%"), text::CAPTION, s.ink_dim))
                        .width(34)
                        .align_right(34),
                ]
                .spacing(tok::M)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            );
        }
        cores = cores.push(r);
    }
    column![
        grid,
        crate::ui::card(s, container(cores).padding([tok::XS, tok::XXS]))
    ]
    .spacing(tok::M)
    .into()
}

/// Disks (DMS DisksView, DriveHealth): each drive, its health from its own SMART readings, and
/// the filesystems on it.
fn disks<'a>(s: Scheme, st: &'a State) -> Element<'a, AppMessage> {
    let Some((drives, volumes)) = &st.drives else {
        return crate::ui::waiting(s, Icon::HardDrive, "Reading the drives…");
    };
    if drives.is_empty() {
        return ui::empty_reason(s, "udisks2 lists no drives.".into(), 120.0);
    }
    let mut c = column![].spacing(tok::M);
    for dr in drives {
        let healthy = dr.warnings.is_empty();
        let status = if !dr.smart && dr.temp.is_none() {
            ("No health readings from this drive".to_string(), s.ink_dim)
        } else if healthy {
            ("Healthy".to_string(), s.good)
        } else {
            (format!("Warning: {}", dr.warnings.join(", ")), s.bad)
        };
        let mut facts: Vec<(&'static str, String)> = Vec::new();
        if let Some(t) = dr.temp {
            facts.push(("Temperature", format!("{t:.0} °C")));
        }
        if let Some(w) = dr.wear {
            facts.push(("Wear", format!("{w}% of rated writes")));
        }
        if let Some((spare, warn)) = dr.spare {
            facts.push(("Spare", format!("{spare}% (warns at {warn}%)")));
        }
        if let Some(hrs) = dr.power_on_hours {
            facts.push(("Powered on", format!("{hrs} hours")));
        }
        if let Some(b) = dr.data_written {
            facts.push(("Written", format!("{:.1} TB", b as f64 / 1e12)));
        }
        if let Some(b) = dr.data_read {
            facts.push(("Read", format!("{:.1} TB", b as f64 / 1e12)));
        }
        if let Some(n) = dr.unsafe_shutdowns {
            facts.push(("Unsafe shutdowns", n.to_string()));
        }
        if let Some(n) = dr.media_errors {
            facts.push(("Media errors", n.to_string()));
        }
        let mut grid = column![].spacing(tok::XS);
        for pair in facts.chunks(2) {
            let mut r = row![].spacing(tok::XL);
            for (k, v) in pair {
                r = r.push(
                    row![
                        label(*k, text::SMALL, s.ink_dim).width(Length::Fill),
                        label(v.clone(), text::SMALL, s.ink)
                    ]
                    .width(Length::Fill),
                );
            }
            grid = grid.push(r);
        }
        let mut card = column![
            row![
                icon(Icon::HardDrive, 18.0, s.ink),
                column![
                    label_w(dr.model.clone(), text::BODY, s.ink, theme::SEMIBOLD),
                    label(
                        format!("{} · {}", dr.bus, gib(dr.size)),
                        text::SMALL,
                        s.ink_dim
                    ),
                ]
                .spacing(tok::XXS)
                .width(Length::Fill),
                label_w(status.0, text::SMALL, status.1, theme::MEDIUM),
            ]
            .spacing(tok::M)
            .align_y(Alignment::Center),
            grid,
        ]
        .spacing(tok::M);
        for v in volumes.iter().filter(|v| v.drive == dr.path) {
            let used = v.free.map(|f| v.size.saturating_sub(f));
            let line = match (used, &v.mount) {
                (Some(u), Some(m)) => format!("{m} · {} of {}", gib(u), gib(v.size)),
                (_, Some(m)) => format!("{m} · {}", gib(v.size)),
                _ => format!("not open · {}", gib(v.size)),
            };
            card = card.push(
                column![
                    row![
                        label(v.name.clone(), text::SMALL, s.ink).width(Length::Fill),
                        label(line, text::SMALL, s.ink_dim),
                    ],
                    bar(
                        s,
                        used.map_or(0.0, |u| u as f32 / v.size.max(1) as f32),
                        6.0
                    ),
                ]
                .spacing(tok::S),
            );
        }
        c = c.push(crate::ui::card(
            s,
            container(card).padding([tok::XS, tok::XXS]),
        ));
    }
    c.into()
}

/// System (DMS SystemView): what this computer is.
fn system<'a>(s: Scheme, st: &'a State) -> Element<'a, AppMessage> {
    let Some(a) = &st.about else {
        return crate::ui::waiting(s, Icon::Laptop, "Reading…");
    };
    let mut rows: Vec<(&'static str, String)> = vec![
        ("Device", a.device.clone()),
        ("Name", a.hostname.clone()),
        ("System", a.os.clone()),
        ("Kernel", a.kernel.clone()),
        ("Desktop", a.compositor.clone()),
        ("Processor", a.processor.clone()),
        ("Memory", a.memory.clone()),
    ];
    for g in &a.graphics {
        rows.push(("Graphics", g.clone()));
    }
    rows.push(("Disk", a.disk.clone()));
    rows.push(("Up for", a.uptime.clone()));
    let mut c = column![].spacing(tok::S);
    for (k, v) in rows.into_iter().filter(|(_, v)| !v.is_empty()) {
        c = c.push(
            row![
                label(k, text::SMALL, s.ink_dim).width(120),
                label(v, text::SMALL, s.ink).width(Length::Fill),
            ]
            .align_y(Alignment::Center),
        );
    }
    crate::ui::card(s, container(c).padding([tok::S, tok::XS]))
}
