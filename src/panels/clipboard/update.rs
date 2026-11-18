//! The clipboard panel's messages and keys.

use super::*;

pub fn update(st: &mut State, m: Message) -> Act {
    // A click on a button (Pin, Delete, Clear all, the scrollbar) leaves the keyboard on the
    // search, so typing still filters (it went nowhere after a click).
    let refocus = matches!(
        m,
        Message::Pin(_) | Message::Delete(_) | Message::Clear | Message::BarPress
    );
    let act = update_inner(st, m);
    if refocus {
        act.and(Act::Run(Task::Iced(iced::widget::focus(INPUT_ID.clone()))))
    } else {
        act
    }
}

pub(super) fn update_inner(st: &mut State, m: Message) -> Act {
    match m {
        Message::Loaded(c) => {
            st.clips = Some(c);
            return Act::Run(settle(st));
        }
        Message::PictureReady => {}
        Message::Thumbs(v) => {
            if let Some(clips) = &mut st.clips {
                for (id, path) in v {
                    st.pending.remove(&id);
                    if let Some(c) = clips.iter_mut().find(|c| c.id() == id) {
                        c.thumb = path;
                    }
                }
            }
            // Their row pictures, made off the drawing thread too.
            let rows = row_pictures(st);
            return Act::Run(Task::batch([rows, preview_for(st)]));
        }
        Message::Query(q) => {
            // Typing is choosing: the top match is marked (Enter pastes it).
            st.query = q;
            st.selected = 0;
            st.top = 0;
            st.glide.snap(0.0);
            st.confirm_clear = false;
            return Act::Run(settle(st));
        }
        Message::Move(d) => {
            let n = filtered(st).len() as i32;
            if n > 0 {
                st.selected = (st.selected as i32 + d).clamp(0, n - 1) as usize;
            }
            return Act::Run(settle(st));
        }
        Message::Hover(i) => {
            st.selected = i;
            return Act::Run(preview_for(st));
        }
        Message::Texts(v) => {
            if st.texts.len() > 200 {
                st.texts.clear();
            }
            for (id, text) in v {
                st.reading.remove(&id);
                st.texts.insert(id, text);
            }
            // The selected row's text may be among them.
            return Act::Run(preview_for(st));
        }
        Message::Preview(id, text) => {
            if st.texts.len() > 200 {
                st.texts.clear();
            }
            st.texts.insert(id.clone(), text.clone());
            st.preview = Some((id, text));
        }
        // The selection stays: the preview beside the list shows it (a highlight that went
        // with the pointer left the preview pointing at nothing).
        Message::LeaveRows => {}
        Message::BarPress => {
            st.dragging = true;
            st.anchor = None;
            return update(st, Message::BarMove(st.bar_at));
        }
        Message::BarRelease => {
            st.dragging = false;
            st.anchor = None;
        }
        Message::DragMove(y) => {
            if st.dragging {
                let h = ROWS as f32 * (ROW_H + 2.0);
                let (y0, f0) = *st.anchor.get_or_insert((y, st.bar_at));
                return update(st, Message::BarMove(f0 + (y - y0) / h));
            }
        }
        Message::BarMove(f) => {
            st.bar_at = f;
            if st.dragging {
                let n = filtered(st).len();
                scroll_to(st, f.clamp(0.0, 1.0) * max_scroll(n), false);
                return Act::Run(thumbs_for_view(st));
            }
        }
        Message::Scroll(px, smooth) => {
            // By the pixel: a touchpad follows the fingers exactly; a wheel step glides.
            let n = filtered(st).len();
            let to = (st.glide.target - px).clamp(0.0, max_scroll(n));
            scroll_to(st, to, smooth);
            return Act::Run(thumbs_for_view(st));
        }
        Message::Copy(i) | Message::CopyOnly(i) => {
            let paste = matches!(m, Message::Copy(_));
            if let Some(c) = filtered(st).get(i).and_then(|&k| st.clips.as_ref()?.get(k)) {
                // Pasted into the window that had focus before the panel (it never takes it).
                let key = c.id().to_string();
                return Act::PasteClip { key, paste }.and(Act::Close);
            }
        }
        Message::Delete(i) => {
            let Some(k) = filtered(st).get(i).copied() else {
                return Act::None;
            };
            if let Some(clips) = &mut st.clips {
                let c = clips.remove(k);
                if let Some(t) = &c.thumb {
                    let _ = std::fs::remove_file(t);
                }
                return Act::Run(Task::batch([
                    Task::future(clip_delete(c.id().to_string())).discard(),
                    settle(st),
                ]));
            }
        }
        Message::Clear => {
            if !st.confirm_clear {
                st.confirm_clear = true;
                return Act::None;
            }
            st.confirm_clear = false;
            // Everything but the pinned entries.
            let pins = pinned();
            let (keep, gone): (Vec<Clip>, Vec<Clip>) = st
                .clips
                .take()
                .unwrap_or_default()
                .into_iter()
                .partition(|c| pins.iter().any(|p| p == c.id()));
            st.clips = Some(keep);
            for c in &gone {
                if let Some(t) = &c.thumb {
                    let _ = std::fs::remove_file(t);
                }
            }
            return Act::Run(
                Task::future(async {
                    let _ = tokio::task::spawn_blocking(crate::services::clips::clear).await;
                })
                .discard(),
            );
        }
        Message::Pin(i) => {
            if let Some(id) = filtered(st)
                .get(i)
                .and_then(|&k| st.clips.as_ref()?.get(k))
                .map(|c| c.id().to_string())
            {
                if let Ok(n) = id.parse::<u64>() {
                    let on = !pinned().contains(&id);
                    crate::services::clips::set_pinned(n, on);
                }
                // The selection follows the entry to its new place (it stayed at the old
                // position, so the buttons then acted on whatever moved into it).
                let list = filtered(st);
                if let Some(pos) = list.iter().position(|&k| {
                    st.clips
                        .as_ref()
                        .and_then(|c| c.get(k))
                        .is_some_and(|c| c.id() == id)
                }) {
                    st.selected = pos;
                    return Act::Run(settle(st));
                }
            }
        }
    }
    Act::None
}

/// Keys while the panel is open (Esc is handled by the shell).
pub fn key(st: &mut State, k: crate::panels::Key) -> Act {
    use crate::panels::Key;
    let sel = st.selected;
    match k {
        Key::Up => update(st, Message::Move(-1)),
        Key::Down | Key::Tab => update(st, Message::Move(1)),
        Key::PageUp => update(st, Message::Move(-(ROWS as i32))),
        Key::PageDown => update(st, Message::Move(ROWS as i32)),
        Key::Home => update(st, Message::Move(i32::MIN / 2)),
        Key::End => update(st, Message::Move(i32::MAX / 2)),
        Key::Delete => update(st, Message::Delete(sel)),
        Key::Enter => update(st, Message::Copy(sel)),
        Key::ShiftEnter => update(st, Message::CopyOnly(sel)),
        _ => Act::None,
    }
}
