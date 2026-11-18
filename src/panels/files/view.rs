//! How the picker looks: places, the list and its columns, the name and file types.

use super::*;
use iced::widget::{column, row};

pub(super) fn kind_icon(it: &Item) -> Icon {
    if it.dir {
        return Icon::Folder;
    }
    let ext = Path::new(&it.name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "avif" | "heic" => Icon::Image,
        "mp4" | "mkv" | "webm" | "mov" | "avi" => Icon::Video,
        "mp3" | "flac" | "ogg" | "opus" | "wav" | "m4a" => Icon::Music,
        "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" | "msix" | "msixbundle" | "appx"
        | "appxbundle" | "msi" | "exe" | "deb" | "rpm" | "apk" | "appimage" | "dmg" | "iso" => {
            Icon::Package
        }
        "rs" | "py" | "js" | "ts" | "c" | "h" | "cpp" | "go" | "lua" | "sh" | "toml" | "json" => {
            Icon::Code
        }
        _ => Icon::File,
    }
}

pub(super) fn human_size(b: u64) -> String {
    match b {
        0..1_000 => format!("{b} B"),
        1_000..1_000_000 => format!("{:.0} KB", b as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b as f64 / 1e6),
        _ => format!("{:.1} GB", b as f64 / 1e9),
    }
}

/// "Today 09:37", "Yesterday 14:02", "12 Nov", "12 Nov 2025".
pub(super) fn when(t: Option<SystemTime>) -> String {
    let Some(t) = t else {
        return String::new();
    };
    let d: chrono::DateTime<chrono::Local> = t.into();
    let today = chrono::Local::now().date_naive();
    let day = d.date_naive();
    if day == today {
        d.format("Today %H:%M").to_string()
    } else if today.pred_opt() == Some(day) {
        d.format("Yesterday %H:%M").to_string()
    } else if day.format("%Y").to_string() == today.format("%Y").to_string() {
        d.format("%-d %b").to_string()
    } else {
        d.format("%-d %b %Y").to_string()
    }
}

/// `path` as people read it: `~/Downloads`.
pub(super) fn pretty(p: &Path) -> String {
    let h = home();
    match p.strip_prefix(&h) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

pub(super) fn folder_name(p: &Path) -> String {
    if p == home() {
        return "Home".into();
    }
    p.file_name()
        .map_or_else(|| "/".into(), |n| n.to_string_lossy().into_owned())
}

/// Why the list is empty, when it is the answer to something typed or chosen (an empty folder
/// just shows empty).
pub(super) fn why_empty(st: &State) -> Option<String> {
    let q = st.query.trim();
    match st.mode {
        Mode::Path => {
            let (dir, prefix) = split_typed(q);
            if !dir.is_dir() {
                Some(format!("No folder {}", pretty(&dir)))
            } else if prefix.is_empty() {
                None
            } else {
                Some(format!(
                    "Nothing in {} starts with “{prefix}”",
                    folder_name(&dir)
                ))
            }
        }
        Mode::Search if st.searching => None,
        Mode::Search => Some(format!("No results for “{q}” in {}", folder_name(&st.dir))),
        Mode::Recent => None,
        Mode::Folder => {
            let r = st.req.as_ref()?;
            if st.listing.is_empty() {
                return None;
            }
            let name = r.filters.get(st.filter)?.name.clone();
            Some(format!("No {name} here"))
        }
    }
}

/// Whether the confirm button has something to act on.
pub(super) fn can_accept(st: &State) -> bool {
    let Some(r) = &st.req else {
        return false;
    };
    if st.mode == Mode::Path {
        let p = expand(st.query.trim());
        return if r.save {
            p.parent().is_some_and(Path::is_dir) && p.file_name().is_some()
        } else {
            p.exists()
        };
    }
    if r.save {
        return !st.name.trim().is_empty();
    }
    r.directory || !st.picked.is_empty() || st.selected.is_some()
}

/// The sidebar's places, in groups: home and its usual folders; GTK bookmarks; mounted drives
/// and the whole computer.
pub(super) fn places() -> Vec<Vec<(String, Icon, PathBuf)>> {
    let h = home();
    let mut v = vec![("Home".to_string(), Icon::House, h.clone())];
    let mut marks: Vec<(String, Icon, PathBuf)> = Vec::new();
    let mut drives = Vec::new();
    for (name, i, sub) in [
        ("Desktop", Icon::Monitor, "Desktop"),
        ("Documents", Icon::File, "Documents"),
        ("Downloads", Icon::Download, "Downloads"),
        ("Pictures", Icon::Image, "Pictures"),
        ("Videos", Icon::Video, "Videos"),
        ("Music", Icon::Music, "Music"),
    ] {
        let p = h.join(sub);
        if p.is_dir() {
            v.push((name.to_string(), i, p));
        }
    }
    // Bookmarks the user made in any GTK file dialog or file manager.
    for f in [".config/gtk-3.0/bookmarks", ".config/gtk-4.0/bookmarks"] {
        for line in std::fs::read_to_string(h.join(f))
            .unwrap_or_default()
            .lines()
        {
            let mut it = line.splitn(2, ' ');
            let Some(path) = it.next().and_then(|u| u.strip_prefix("file://")) else {
                continue;
            };
            let p = PathBuf::from(decode(path));
            if p.is_dir() && !v.iter().chain(&marks).any(|(_, _, q)| *q == p) {
                let name = it
                    .next()
                    .map(str::to_string)
                    .or_else(|| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_default();
                marks.push((name, Icon::Pin, p));
            }
        }
    }
    // Mounted drives (USB sticks, the Windows partition).
    let user = std::env::var("USER").unwrap_or_default();
    for base in [
        PathBuf::from("/run/media").join(&user),
        PathBuf::from("/mnt"),
    ] {
        for e in std::fs::read_dir(base).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                drives.push((
                    e.file_name().to_string_lossy().into_owned(),
                    Icon::HardDrive,
                    e.path(),
                ));
            }
        }
    }
    drives.push(("Computer".into(), Icon::Laptop, PathBuf::from("/")));
    vec![v, marks, drives]
        .into_iter()
        .filter(|g| !g.is_empty())
        .collect()
}

pub(super) fn field<'a>(
    input: text_input::TextInput<'a, Message>,
    s: Scheme,
    lead: Icon,
) -> Element<'a, Message> {
    container(
        row![icon(lead, tok::ICON, s.ink_dim), input]
            .spacing(tok::XS)
            .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::L])
    .height(40)
    .center_y(40)
    .style(move |_| container::Style {
        background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
        border: iced::Border::default().rounded(tok::FULL),
        ..container::Style::default()
    })
    .into()
}

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let Some(r) = &st.req else {
        return Space::new().into();
    };
    let title = if !r.title.is_empty() {
        r.title.clone()
    } else if r.save {
        "Save".into()
    } else if r.directory {
        "Choose a folder".into()
    } else {
        "Open".into()
    };
    let replacing = r.save && st.replace.is_some();
    let accept_label = if replacing {
        "Replace".to_string()
    } else if !r.accept.is_empty() {
        r.accept.replace('_', "")
    } else if r.save {
        "Save".into()
    } else if r.directory {
        "Choose".into()
    } else if st.picked.len() > 1 {
        format!("Open {}", st.picked.len())
    } else {
        "Open".into()
    };
    let can = can_accept(st);
    // The kit's button (its disabled look); saving over a file asks with the danger fill.
    let accept = ui::button(
        s,
        accept_label,
        crate::ui::control::Pill::Primary,
        can.then_some(Message::Accept),
    )
    .padding([tok::S, tok::XL])
    .radius(tok::FULL);
    let accept = if replacing && can {
        accept.kind(s, crate::ui::control::Kind::Danger)
    } else {
        accept
    };
    let header = row![
        label_w(
            ui::fit(&title, text::HEADING, 560.0),
            text::HEADING,
            s.ink,
            theme::SEMIBOLD
        )
        .width(Length::Fill),
        control(label("Cancel", text::SMALL, s.ink))
            .padding([tok::S, tok::XL])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(Message::Cancel),
        accept,
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);

    // The sidebar: places in groups, the one you are in marked.
    let mut side = column![].spacing(tok::XXS).width(SIDEBAR);
    let place = |name: String, i: Icon, here: bool, m: Message| {
        control(
            row![
                icon(i, tok::ICON, if here { s.accent } else { s.ink_dim }),
                label_w(
                    ui::fit(&name, text::BODY, SIDEBAR - 52.0),
                    text::BODY,
                    s.ink,
                    if here { theme::SEMIBOLD } else { theme::MEDIUM }
                )
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::L])
        .height(tok::H_ROW)
        .width(Length::Fill)
        .radius(tok::R_ROW)
        .kind(
            s,
            if here {
                crate::ui::control::Kind::Chosen
            } else {
                crate::ui::control::Kind::Ghost
            },
        )
        .no_press_scale()
        .on_press(m)
    };
    side = side.push(place(
        "Recent".into(),
        Icon::Timer,
        st.mode == Mode::Recent,
        Message::Recent,
    ));
    for (k, group) in places().into_iter().enumerate() {
        if k > 0 {
            side = side.push(Space::new().height(tok::L));
        }
        for (name, i, p) in group {
            let here = st.dir == p && matches!(st.mode, Mode::Folder | Mode::Search);
            side = side.push(place(name, i, here, Message::Go(p)));
        }
    }
    let side = iced::widget::scrollable(side)
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0),
        ))
        .height(Length::Fill);

    // Back, forward, and where you are: the folder's parents, each one a step back. A typed
    // path shows the folder it points into.
    let nav = |i: Icon, m: Option<Message>| {
        control(
            container(icon(
                i,
                tok::ICON_M,
                if m.is_some() { s.ink } else { s.ink_faint },
            ))
            .center(28),
        )
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press_maybe(m)
    };
    let mut crumbs = row![
        nav(
            Icon::ChevronLeft,
            (!st.back.is_empty()).then_some(Message::Back)
        ),
        nav(
            Icon::ChevronRight,
            (!st.forward.is_empty()).then_some(Message::Forward)
        ),
    ]
    .spacing(tok::XXS)
    .align_y(Alignment::Center);
    let shown_dir = if st.mode == Mode::Path {
        // The deepest part of the typed path that exists (one that does not is not a place).
        let typed = split_typed(st.query.trim()).0;
        typed
            .ancestors()
            .find(|p| p.is_dir())
            .map_or_else(|| st.dir.clone(), Path::to_path_buf)
    } else {
        st.dir.clone()
    };
    let h = home();
    let mut parts: Vec<(String, PathBuf)> = Vec::new();
    let mut p = shown_dir.as_path();
    loop {
        parts.push((folder_name(p), p.to_path_buf()));
        if p == h {
            break;
        }
        match p.parent() {
            Some(q) => p = q,
            None => break,
        }
    }
    parts.reverse();
    // Recent files and search results are not a folder: say what they are instead.
    match st.mode {
        Mode::Recent => parts = vec![("Recent".into(), st.dir.clone())],
        Mode::Search => parts.push((format!("Results for “{}”", st.query.trim()), st.dir.clone())),
        _ => {}
    }
    // A deep path shows its last steps (the first is a way back to the rest).
    if parts.len() > 4 {
        let keep = parts.split_off(parts.len() - 3);
        let first = parts[parts.len() - 1].1.clone();
        parts = vec![("…".into(), first)];
        parts.extend(keep);
    }
    let last = parts.len().saturating_sub(1);
    for (k, (name, path)) in parts.into_iter().enumerate() {
        if k > 0 {
            crumbs = crumbs.push(icon(Icon::ChevronRight, tok::ICON_S, s.ink_faint));
        }
        let current = k == last;
        crumbs = crumbs.push(
            control(label_w(
                ui::fit(&name, text::SMALL, 180.0),
                text::SMALL,
                if current { s.ink } else { s.ink_dim },
                if current {
                    theme::SEMIBOLD
                } else {
                    theme::MEDIUM
                },
            ))
            .padding([tok::S, tok::M])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(Message::Go(path)),
        );
    }
    let search = text_input("Search, or type a path", &st.query)
        .id(SEARCH_ID.clone())
        .on_input(Message::Query)
        .padding([tok::M, tok::S])
        .size(text::BODY)
        .font(theme::FONT)
        .style(crate::ui::input_style(s));
    let tools = row![
        container(crumbs).width(Length::Fill).clip(true),
        container(field(search, s, Icon::Search)).width(260),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);

    // Column headings: a click sorts by that column, again turns it round.
    let heading = |name: &'static str, by: By, width: Length, right: bool| {
        let on = st.sort.by == by;
        let mut r = row![].spacing(tok::XS).align_y(Alignment::Center);
        let words = label_w(
            name,
            text::SMALL,
            if on { s.ink } else { s.ink_dim },
            if on { theme::SEMIBOLD } else { theme::MEDIUM },
        );
        let arrow = on.then(|| {
            icon(
                if st.sort.desc {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronUp
                },
                12.0,
                s.ink,
            )
        });
        if right {
            r = r.push(Space::new().width(Length::Fill));
        }
        r = r.push(words);
        if let Some(a) = arrow {
            r = r.push(a);
        }
        control(r)
            .padding([tok::NONE, tok::M])
            .height(26)
            .width(width)
            .radius(tok::R_CONTROL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .no_press_scale()
            .on_press(Message::SortBy(by))
    };
    let columns = row![
        Space::new().width(28 + 4),
        heading("Name", By::Name, Length::Fill, false),
        heading("Size", By::Size, Length::Fixed(SIZE_W + 8.0), true),
        heading("Modified", By::Modified, Length::Fixed(DATE_W + 8.0), true),
    ]
    .spacing(tok::XS)
    .align_y(Alignment::Center);

    // The list.
    let list = shown(st);
    let mut rows = column![].spacing(tok::XXS);
    if let Some(n) = &st.new_folder {
        let input = text_input("New folder name", n)
            .id(NEW_FOLDER_ID.clone())
            .on_input(Message::FolderName)
            .padding([tok::M, tok::S])
            .size(text::BODY)
            .font(theme::FONT)
            .style(crate::ui::input_style(s));
        rows = rows.push(
            container(
                row![
                    container(icon(Icon::Folder, tok::ICON_L, s.accent)).center(28),
                    input
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::M])
            .height(ROW_H),
        );
    }
    let name_w = WIDTH - 32.0 - SIDEBAR - 16.0 - 16.0 - 28.0 - 36.0 - SIZE_W - DATE_W - 8.0;
    for &i in list.iter().skip(st.top).take(ROWS + 1) {
        let it = &st.items[i];
        let selected = st.selected == Some(i) || st.picked.contains(&it.path);
        // Choosing a folder: files are only there to look at.
        let inert = r.directory && !it.dir;
        let ink = if inert { s.ink_faint } else { s.ink };
        let lead: Element<'_, Message> = match kind_icon(it) {
            Icon::Image if it.size < 6_000_000 => {
                // Only a thumbnail already made (`pictures` makes them off this thread).
                match ui::fitted_file_ready(&it.path, 28.0, 28.0).flatten() {
                    Some((h, w, ht)) => container(ui::snap::snap(
                        iced::widget::image(h)
                            .width(w)
                            .height(ht)
                            .opacity(s.opacity),
                    ))
                    .center(28)
                    .into(),
                    None => container(icon(Icon::Image, tok::ICON_M, s.ink_dim))
                        .center(28)
                        .into(),
                }
            }
            Icon::Folder => container(icon(Icon::Folder, tok::ICON_L, s.accent))
                .center(28)
                .into(),
            i => container(icon(i, tok::ICON_M, s.ink_dim)).center(28).into(),
        };
        let detail = if it.dir {
            String::new()
        } else {
            human_size(it.size)
        };
        let mut words = column![label(
            ui::fit(&it.name, text::BODY, name_w),
            text::BODY,
            ink
        )]
        .spacing(tok::HAIR)
        .width(Length::Fill);
        if let Some(rel) = &it.rel {
            words = words.push(label(
                ui::fit(rel, text::CAPTION, name_w),
                text::CAPTION,
                s.ink_dim,
            ));
        }
        rows = rows.push(
            control(
                row![
                    lead,
                    words,
                    label(detail, text::SMALL, s.ink_dim)
                        .width(SIZE_W)
                        .align_x(Alignment::End),
                    label(when(it.modified), text::SMALL, s.ink_dim)
                        .width(DATE_W)
                        .align_x(Alignment::End),
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::M])
            .height(ROW_H)
            .width(Length::Fill)
            .radius(tok::R_ROW)
            // Selected: the chosen fill (clear at a glance); under the pointer: the light lift.
            .kind(
                s,
                if selected {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .no_press_scale()
            .on_press_maybe((!inert).then_some(Message::Click(i))),
        );
    }
    // The rows in view (and the one coming in), moved up by the part of a row scrolled past,
    // inside a clip: it scrolls by the pixel, and thousands of files cost nothing.
    const STRIDE: f32 = ROW_H + 2.0;
    let view_h = ROWS as f32 * STRIDE;
    let body: Element<'_, Message> = match (list.is_empty() && st.new_folder.is_none())
        .then(|| why_empty(st))
        .flatten()
    {
        // Nothing to show because of what was typed or chosen: say so where the list would be.
        Some(why) => crate::ui::empty_reason(s, why, view_h),
        None => {
            let frac = (st.pos - st.top as f32) * STRIDE;
            let rows = container(
                iced::widget::stack![iced::widget::pin(rows).y(-frac).width(Length::Fill)]
                    .width(Length::Fill)
                    .height(view_h),
            )
            .clip(true)
            .height(view_h);
            ui::smooth(mouse_area(rows).on_scroll(|d| {
                Message::Scroll(match d {
                    iced::mouse::ScrollDelta::Lines { y, .. } => y * 1.3,
                    iced::mouse::ScrollDelta::Pixels { y, .. } => y / STRIDE,
                })
            }))
            .into()
        }
    };

    // The foot: the name (saving), the app's file types, hidden files, a new folder.
    let small = |text_: String, on: bool, m: Message| {
        control(label(
            ui::fit(&text_, text::SMALL, 160.0),
            text::SMALL,
            if on { s.ink } else { s.ink_dim },
        ))
        .padding([tok::S, tok::L])
        .radius(tok::FULL)
        .kind(
            s,
            if on {
                crate::ui::control::Kind::Chosen
            } else {
                crate::ui::control::Kind::Ghost
            },
        )
        .on_press(m)
    };
    let mut foot = row![].spacing(tok::S).align_y(Alignment::Center);
    if r.save {
        let name = text_input("Name", &st.name)
            .id(NAME_ID.clone())
            .on_input(Message::Name)
            .padding([tok::M, tok::S])
            .size(text::BODY)
            .font(theme::FONT)
            .style(crate::ui::input_style(s));
        foot = foot.push(container(field(name, s, Icon::File)).width(Length::Fill));
    } else {
        foot = foot.push(Space::new().width(Length::Fill));
    }
    if !r.directory {
        for (k, name) in filter_names(r).into_iter().enumerate() {
            foot = foot.push(small(name, k == st.filter, Message::Filter(k)));
        }
    }
    if r.save || r.directory {
        foot = foot.push(small("New folder".into(), false, Message::NewFolder));
    }
    foot = foot.push(small("Hidden files".into(), st.hidden, Message::Hidden));

    let main = column![tools, columns, body]
        .spacing(tok::M)
        .width(Length::Fill);
    let middle = row![
        container(side).height(Length::Fixed(view_h + 26.0 + 40.0 + 16.0)),
        main
    ]
    .spacing(tok::XL);
    container(column![header, middle, foot].spacing(tok::L))
        .padding(crate::theme::tok::PANEL_PAD)
        .into()
}
