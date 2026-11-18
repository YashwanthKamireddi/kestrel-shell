//! The launcher's messages and keys.

use super::*;

pub fn update(st: &mut State, m: Message) -> Act {
    match m {
        Message::Clips(c) => st.clips = Some(c),
        Message::Apps(v) => {
            if st.apps != v {
                st.apps = v;
            }
            // Every app's icon made in the background the first time the list is in (drawing
            // them when a keystroke first showed them cost 6–10 ms on the UI thread, a dropped
            // frame per letter). Here, not on the first open: the list may not be read yet.
            static WARMED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !st.apps.is_empty() && !WARMED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                let icons: Vec<String> = st.apps.iter().map(|a| a.icon.clone()).collect();
                return Act::Run(
                    Task::future(async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            for i in icons {
                                if let Some(p) = icons::lookup(&i) {
                                    ui::warm_picture(&p, 28.0);
                                }
                            }
                        })
                        .await;
                    })
                    .discard(),
                );
            }
        }
        Message::Procs(name, v) => {
            // Results for an older query are dropped.
            if st.query.trim().strip_prefix('!').map(str::trim) == Some(name.as_str()) {
                st.procs = (name, v);
            }
        }
        Message::Files(q, v) => {
            // Results for an older query are dropped.
            if st.query.trim() == q {
                st.files = (q, v);
            }
        }
        Message::Query(q) => {
            let wants_clips = q.starts_with(':') && st.clips.is_none();
            st.query = q.clone();
            st.selected = 0;
            st.armed = None;
            st.glide.snap(0.0);
            st.exe = on_path(q.split_whitespace().next().unwrap_or(""));
            if let Some(term) = q.trim().strip_prefix('/')
                && term.trim().len() >= 2
                && pasted_path(&q).is_none()
            {
                let (q2, term) = (q.trim().to_string(), term.trim().to_string());
                return Act::Run(Task::perform(find_files(term), move |v| {
                    Message::Files(q2.clone(), v)
                }));
            }
            if let Some(name) = q.trim().strip_prefix('!') {
                let name = name.trim().to_string();
                return Act::Run(Task::perform(find_processes(name.clone()), move |v| {
                    Message::Procs(name.clone(), v)
                }));
            }
            if wants_clips {
                st.clips = Some(Vec::new());
                return Act::Run(Task::perform(clip_list(), Message::Clips));
            }
        }
        Message::LeaveRows => {
            if st.by_pointer && st.query.trim().is_empty() {
                st.touched = false;
            }
        }
        Message::Hover(i) => {
            st.selected = i;
            st.touched = true;
            st.by_pointer = true;
            st.glide.snap(i as f32);
        }
        Message::Move(d) => {
            let n = entries(st).len();
            if n > 0 {
                // The first press shows where you are (the top, or the bottom going up).
                st.selected = if st.touched || !st.query.trim().is_empty() {
                    (st.selected as i32 + d).rem_euclid(n as i32) as usize
                } else if d > 0 {
                    0
                } else {
                    n - 1
                };
                st.touched = true;
                st.by_pointer = false;
                // It moves at once: a gliding highlight lagged behind the keys.
                st.glide.snap(st.selected as f32);
            }
        }
        Message::Submit => {
            let i = st.selected;
            return update(st, Message::Launch(i));
        }
        Message::Launch(i) => {
            let action = match entries(st).get(i) {
                Some(Entry::App(a)) => {
                    Some((Some(a.id.clone()), Action::Exec(apps::command_line(a))))
                }
                Some(Entry::Run(c)) => Some((None, Action::Exec(c.clone()))),
                Some(Entry::Clip(c)) => {
                    let line = c.id().to_string();
                    return Act::Run(Task::future(clip_copy(line)).discard()).and(Act::Close);
                }
                Some(Entry::Emoji(e, _)) => {
                    let e = e.to_string();
                    // Copied, and pasted into the window that had focus.
                    return Act::Type(e).and(Act::Close);
                }
                Some(Entry::Web(t)) => {
                    let url = format!("{SEARCH_URL}{}", encode(t));
                    Some((None, Action::Open(url)))
                }
                Some(Entry::File(p)) => Some((None, Action::Open(p.to_string_lossy().into()))),
                Some(Entry::Setting(p)) => {
                    let p = *p;
                    return Act::Close.and(Act::Settings(p));
                }
                Some(Entry::Switch(w)) => {
                    let w = *w;
                    return Act::Close.and(Act::Change(w.change()));
                }
                Some(Entry::Process(pid, _, _)) => {
                    // Asked once: the first press arms it ("End …?"), the second ends it. One
                    // stray Enter used to end an app on the spot, unsaved work and all.
                    let pid = *pid;
                    if st.armed != Some(pid) {
                        st.armed = Some(pid);
                        return Act::None;
                    }
                    st.armed = None;
                    // Safety: plain kill(2) with SIGTERM on a pid of this user.
                    unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                    }
                    // Read again (it went, or is going).
                    let q = st.query.clone();
                    st.procs.1.retain(|p| p.0 != pid);
                    return Act::Run(Task::done(Message::Query(q)));
                }
                Some(Entry::Calc(v)) => {
                    let v = v.clone();
                    return Act::Run(
                        Task::future(crate::panels::clipboard::copy_text(v)).discard(),
                    )
                    .and(Act::Close);
                }
                None => None,
            };
            if let Some((id, act)) = action {
                if let Some(id) = id {
                    *st.counts.entry(id).or_insert(0) += 1;
                    save_counts(&st.counts);
                }
                return Act::Dispatch(act).and(Act::Close);
            }
        }
    }
    Act::None
}

pub fn key(st: &mut State, k: crate::panels::Key) -> Act {
    use crate::panels::Key;
    match k {
        Key::Up | Key::BackTab => update(st, Message::Move(-1)),
        Key::Down | Key::Tab => update(st, Message::Move(1)),
        _ => Act::None,
    }
}
