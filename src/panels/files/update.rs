//! What the picker does: messages, keys, opening and choosing, the answer to the app.

use super::*;

pub(super) fn finish(st: &mut State, paths: Vec<PathBuf>) -> Act {
    if let Some(r) = st.req.take() {
        // Next time it opens where this file was (a search or a typed path may have found it
        // far from the folder on screen).
        let dir = paths
            .first()
            .and_then(|p| {
                if p.is_dir() {
                    Some(p.as_path())
                } else {
                    p.parent()
                }
            })
            .unwrap_or(&st.dir)
            .to_string_lossy()
            .into_owned();
        crate::state::update(|s| s.files.last_folder = Some(dir));
        if r.id == filechooser::OWN {
            // kestrel's own question: the answer goes back into the shell.
            if let (Some(then), Some(p)) = (st.then.take(), paths.into_iter().next()) {
                return Act::Close.and(Act::Reply(Box::new(then(p))));
            }
        } else {
            filechooser::answer(r.id, Answer::Chosen(paths));
        }
    }
    Act::Close
}

/// Thumbnails for the pictures in view (and a page either side) not made yet, made off the
/// drawing thread: decoding them while drawing stalled every frame of a Pictures folder.
pub(super) fn pictures(st: &State) -> Task<Message> {
    let list = shown(st);
    let want: Vec<PathBuf> = list
        .iter()
        .skip(st.top.saturating_sub(ROWS))
        .take(ROWS * 3)
        .filter_map(|&i| st.items.get(i))
        .filter(|it| matches!(kind_icon(it), Icon::Image) && it.size < 6_000_000)
        .filter(|it| ui::fitted_file_ready(&it.path, 28.0, 28.0).is_none())
        .map(|it| it.path.clone())
        .collect();
    if want.is_empty() {
        return Task::none();
    }
    Task::perform(
        async move {
            let _ = tokio::task::spawn_blocking(move || {
                for p in want {
                    let _ = ui::fitted_file(&p, 28.0, 28.0, 5.0);
                }
            })
            .await;
        },
        |()| Message::Pictures,
    )
}

pub fn update(st: &mut State, m: Message) -> Act {
    let pics = !matches!(m, Message::Pictures | Message::Mods(_));
    // A click anywhere (a row, a column, a place) leaves the keyboard where typing is expected:
    // the search, or the name when saving. It used to stay on nothing, so typing after a click
    // went nowhere.
    let refocus = matches!(
        m,
        Message::Go(_)
            | Message::Up
            | Message::Click(_)
            | Message::Back
            | Message::Forward
            | Message::Hidden
            | Message::SortBy(_)
            | Message::Recent
            | Message::Filter(_)
            | Message::CreateFolder
    );
    let mut act = update_inner(st, m);
    if pics {
        act = act.and(Act::Run(pictures(st)));
    }
    if refocus && st.req.is_some() && st.new_folder.is_none() {
        let save = st.req.as_ref().is_some_and(|r| r.save);
        act = act.and(Act::Run(Task::Iced(iced::widget::focus(if save {
            NAME_ID.clone()
        } else {
            SEARCH_ID.clone()
        }))));
    }
    act
}

pub(super) fn update_inner(st: &mut State, m: Message) -> Act {
    match m {
        Message::Pictures => {}
        Message::Go(p) => go(st, p),
        Message::Up => {
            if let Some(parent) = st.dir.parent().map(Path::to_path_buf) {
                go(st, parent);
            }
        }
        Message::Back => {
            if let Some(p) = st.back.pop() {
                st.forward.push(st.dir.clone());
                load(st, p);
            }
        }
        Message::Forward => {
            if let Some(p) = st.forward.pop() {
                st.back.push(st.dir.clone());
                load(st, p);
            }
        }
        Message::Hidden => {
            st.hidden = !st.hidden;
            let d = st.dir.clone();
            load(st, d);
        }
        Message::SortBy(by) => {
            st.sort = if st.sort.by == by {
                Sort {
                    by,
                    desc: !st.sort.desc,
                }
            } else {
                // Each column starts the useful way: names A to Z, biggest and newest first.
                Sort {
                    by,
                    desc: by != By::Name,
                }
            };
            let key = st.dir.to_string_lossy().into_owned();
            let value = st.sort.key();
            crate::state::update(|s| {
                s.files.sort.insert(key, value);
            });
            let keep = st
                .selected
                .and_then(|i| st.items.get(i))
                .map(|i| i.path.clone());
            sort(&mut st.listing, st.sort);
            sort(&mut st.items, st.sort);
            st.selected = keep.and_then(|p| st.items.iter().position(|i| i.path == p));
            st.top = 0;
            st.pos = 0.0;
        }
        Message::Recent => {
            st.items = recent();
            st.mode = Mode::Recent;
            st.searching = false;
            st.query.clear();
            new_list(st);
        }
        Message::Query(q) => {
            new_list(st);
            st.query = q.clone();
            st.searching = false;
            let t = q.trim();
            if t.is_empty() {
                st.items = st.listing.clone();
                st.mode = Mode::Folder;
            } else if t.starts_with('/') || t.starts_with('~') {
                // A path, typed or pasted: its folder's entries that start like it.
                st.items = completions(st, t);
                st.mode = Mode::Path;
            } else {
                // Here first, at once; the folders below follow when the search finishes.
                let low = t.to_lowercase();
                st.items = st
                    .listing
                    .iter()
                    .filter(|it| it.name.to_lowercase().contains(&low))
                    .cloned()
                    .collect();
                st.mode = Mode::Search;
                if t.chars().count() >= 2 {
                    st.searching = true;
                    let (dir, hidden, q2) = (st.dir.clone(), st.hidden, t.to_string());
                    return Act::Run(Task::perform(
                        search_below(dir, q2.clone(), hidden),
                        move |v| Message::Found(q2.clone(), v),
                    ));
                }
            }
        }
        Message::Found(q, paths) => {
            if st.mode == Mode::Search && st.query.trim() == q {
                st.searching = false;
                let here: std::collections::HashSet<PathBuf> =
                    st.items.iter().map(|i| i.path.clone()).collect();
                let base = st.dir.clone();
                st.items.extend(
                    paths
                        .into_iter()
                        .filter(|p| !here.contains(p))
                        .filter_map(|p| {
                            let rel = p
                                .parent()
                                .and_then(|d| d.strip_prefix(&base).ok())
                                .map(|d| d.display().to_string())
                                .filter(|d| !d.is_empty());
                            item(p, rel)
                        }),
                );
                // In the order chosen, like the folder itself.
                let keep = st
                    .selected
                    .and_then(|i| st.items.get(i))
                    .map(|i| i.path.clone());
                sort(&mut st.items, st.sort);
                st.selected = keep.and_then(|p| st.items.iter().position(|i| i.path == p));
            }
        }
        Message::NewFolder => {
            st.new_folder = Some(String::new());
            return Act::Run(Task::Iced(iced::widget::focus(NEW_FOLDER_ID.clone())));
        }
        Message::FolderName(n) => st.new_folder = Some(n),
        Message::CreateFolder => {
            if let Some(n) = st.new_folder.take().map(|n| n.trim().to_string())
                && !n.is_empty()
                && !n.contains('/')
            {
                let p = st.dir.join(&n);
                if std::fs::create_dir(&p).is_ok() {
                    let d = st.dir.clone();
                    load(st, d);
                    st.selected = st.items.iter().position(|i| i.path == p);
                }
            }
        }
        Message::Name(n) => {
            st.name = n;
            st.replace = None;
        }
        Message::Filter(i) => {
            st.filter = i;
            new_list(st);
        }
        Message::Scroll(lines) => {
            let n = shown(st).len();
            // By the pixel (a wheel's notches arrive already smoothed into small steps).
            let max = n.saturating_sub(ROWS) as f32;
            st.pos = (st.pos - lines).clamp(0.0, max);
            st.top = st.pos.floor() as usize;
        }
        Message::Click(i) => {
            let double = st
                .click
                .is_some_and(|(j, at)| j == i && at.elapsed().as_millis() < 450);
            st.click = Some((i, Instant::now()));
            st.selected = Some(i);
            let Some(it) = st.items.get(i).cloned() else {
                return Act::None;
            };
            let multiple = st.req.as_ref().is_some_and(|r| r.multiple);
            if multiple {
                pick(st, &it);
            }
            if st.req.as_ref().is_some_and(|r| r.save) && !it.dir {
                st.name = it.name.clone();
            }
            if double {
                return open(st, i);
            }
        }
        Message::Mods(m) => st.mods = m,
        Message::Accept => return accept(st),
        Message::Cancel => {
            closed(st);
            return Act::Close;
        }
    }
    Act::None
}

/// A click when several files may be chosen, as GTK's and macOS's pickers do: alone it picks
/// just that file; with Ctrl it is added or taken away; with Shift every file from the last
/// click to it (in the order shown) is picked.
fn pick(st: &mut State, it: &Item) {
    let m = st.mods;
    if m.shift()
        && let Some(from) = st.anchor.clone()
    {
        let list: Vec<&Item> = shown(st)
            .into_iter()
            .filter_map(|i| st.items.get(i))
            .collect();
        let a = list.iter().position(|x| x.path == from);
        let b = list.iter().position(|x| x.path == it.path);
        if let (Some(a), Some(b)) = (a, b) {
            let (lo, hi) = (a.min(b), a.max(b));
            st.picked = list[lo..=hi]
                .iter()
                .filter(|x| !x.dir)
                .map(|x| x.path.clone())
                .collect();
            return;
        }
    }
    st.anchor = Some(it.path.clone());
    if m.control() {
        if let Some(at) = st.picked.iter().position(|p| *p == it.path) {
            st.picked.remove(at);
        } else if !it.dir {
            st.picked.push(it.path.clone());
        }
        return;
    }
    st.picked.clear();
    if !it.dir {
        st.picked.push(it.path.clone());
    }
}

/// Opens item `i`: into a folder, or chooses the file.
pub(super) fn open(st: &mut State, i: usize) -> Act {
    let Some(it) = st.items.get(i).cloned() else {
        return Act::None;
    };
    let directory = st.req.as_ref().is_some_and(|r| r.directory);
    if it.dir {
        go(st, it.path);
        return Act::None;
    }
    if directory {
        return Act::None;
    }
    if st.req.as_ref().is_some_and(|r| r.save) {
        st.name = it.name;
        return accept(st);
    }
    let mut paths = st.picked.clone();
    if !paths.contains(&it.path) {
        paths.push(it.path);
    }
    finish(st, paths)
}

/// The confirm button: the typed name (saving), the chosen files, or this folder.
pub(super) fn accept(st: &mut State) -> Act {
    let Some(r) = st.req.clone() else {
        return Act::None;
    };
    // A typed or pasted path: a folder opens; a file is chosen (or saved to).
    if st.mode == Mode::Path {
        let path = expand(st.query.trim());
        if path.is_dir() {
            go(st, path);
            return Act::None;
        }
        if !r.save && !r.directory && path.is_file() {
            return finish(st, vec![path]);
        }
        if r.save
            && let (Some(dir), Some(name)) = (path.parent(), path.file_name())
            && dir.is_dir()
        {
            let (dir, name) = (dir.to_path_buf(), name.to_string_lossy().into_owned());
            go(st, dir);
            st.name = name;
            return accept(st);
        }
        return Act::None;
    }
    if r.save {
        let name = st.name.trim();
        if name.is_empty() {
            return Act::None;
        }
        let path = st.dir.join(name);
        // Saving over a file asks once: the button says Replace, the next press does it.
        if path.exists() && st.replace.as_ref() != Some(&path) {
            st.replace = Some(path);
            return Act::None;
        }
        return finish(st, vec![path]);
    }
    if r.directory {
        let p = st
            .selected
            .and_then(|i| st.items.get(i))
            .filter(|it| it.dir)
            .map_or_else(|| st.dir.clone(), |it| it.path.clone());
        return finish(st, vec![p]);
    }
    if !st.picked.is_empty() {
        let p = st.picked.clone();
        return finish(st, p);
    }
    match st.selected {
        Some(i) => open(st, i),
        None => {
            // Enter in the search: the first match.
            match shown(st).first().copied() {
                Some(i) => open(st, i),
                None => Act::None,
            }
        }
    }
}

pub fn key(st: &mut State, k: crate::panels::Key) -> Act {
    use crate::panels::Key;
    let list = shown(st);
    let n = list.len();
    let pos = st.selected.and_then(|i| list.iter().position(|&j| j == i));
    let to = match k {
        Key::Down => Some(pos.map_or(0, |p| (p + 1).min(n.saturating_sub(1)))),
        Key::Up => Some(pos.map_or(0, |p| p.saturating_sub(1))),
        Key::PageDown => Some(pos.map_or(0, |p| (p + ROWS).min(n.saturating_sub(1)))),
        Key::PageUp => Some(pos.map_or(0, |p| p.saturating_sub(ROWS))),
        Key::Home if st.query.is_empty() => Some(0),
        Key::End if st.query.is_empty() => Some(n.saturating_sub(1)),
        // Enter is handled here only (the text fields do not also submit, which made one
        // press act twice: into a folder, then on its first file).
        Key::Enter if st.new_folder.is_some() => return update(st, Message::CreateFolder),
        Key::Enter => return accept(st),
        Key::Left if st.query.is_empty() && !st.req.as_ref().is_some_and(|r| r.save) => {
            return update(st, Message::Up);
        }
        // Tab completes a typed path as far as it is unambiguous.
        Key::Tab if st.mode == Mode::Path => {
            // Completed in the form it was typed (`~/Dow` → `~/Downloads/`, not /home/…).
            let typed = st.query.trim().to_string();
            if !typed.contains('/') {
                // A bare `~`: into home.
                return update(st, Message::Query(format!("{typed}/"))).and(Act::Run(Task::Iced(
                    iced::widget::move_cursor_to_end(SEARCH_ID.clone()),
                )));
            }
            let stem = typed[..typed.rfind('/').map_or(0, |i| i + 1)].to_string();
            let names: Vec<&Item> = st.items.iter().collect();
            if let Some(first) = names.first() {
                let mut common = first.name.clone();
                for it in &names[1..] {
                    while !it.name.starts_with(&common) {
                        common.pop();
                    }
                }
                let mut q = format!("{stem}{common}");
                if names.len() == 1 && first.dir {
                    q.push('/');
                }
                return update(st, Message::Query(q)).and(Act::Run(Task::Iced(
                    iced::widget::move_cursor_to_end(SEARCH_ID.clone()),
                )));
            }
            None
        }
        Key::Right => {
            if let Some(i) = st.selected
                && st.items.get(i).is_some_and(|it| it.dir)
            {
                return open(st, i);
            }
            None
        }
        _ => None,
    };
    if let Some(p) = to
        && n > 0
    {
        st.selected = Some(list[p]);
        if p < st.top {
            st.top = p;
            st.pos = st.top as f32;
        } else if p >= st.top + ROWS {
            st.top = p + 1 - ROWS;
            st.pos = st.top as f32;
        }
    }
    Act::None
}
