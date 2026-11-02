//! How the clipboard panel looks: the list, its rows, the preview and the scrollbar.

use super::*;
use iced::widget::{column, row};

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let list = filtered(st);
    let total = st.clips.as_ref().map_or(0, Vec::len);

    let field = ui::search_field(
        s,
        "Search",
        &st.query,
        INPUT_ID.clone(),
        Message::Query,
        None,
    );

    // The kit's button (its disabled look while there is nothing to clear); asked once: the
    // confirm is the danger fill, as everywhere.
    let clear = if st.confirm_clear {
        ui::button(
            s,
            format!("Delete all {total}?"),
            crate::ui::control::Pill::Primary,
            Some(Message::Clear),
        )
        .kind(s, crate::ui::control::Kind::Danger)
    } else {
        ui::button(
            s,
            "Clear all",
            crate::ui::control::Pill::Quiet,
            (total > 0).then_some(Message::Clear),
        )
    }
    .padding([tok::NONE, tok::L])
    .height(28)
    .radius(tok::FULL);
    let header = row![
        label_w("Clipboard", text::HEADING, s.ink, theme::SEMIBOLD),
        Space::new().width(Length::Fill),
        clear,
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);

    let body: Element<'_, Message> = match &st.clips {
        // Nothing (yet, or nothing found): the list's space stays, empty, with no label.
        None => placeholder(),
        // A filter nothing matches says so (the list's space empty read as broken).
        Some(_) if list.is_empty() && !st.query.trim().is_empty() => crate::ui::empty_reason(
            s,
            format!("No matches for “{}”", st.query.trim()),
            ROWS as f32 * (ROW_H + 2.0),
        ),
        Some(_) if list.is_empty() => placeholder(),
        Some(clips) => {
            // Only the rows in view (and the one entering), moved up by the part of a row
            // already scrolled past, inside a clip: smooth, and thousands of entries cost
            // nothing.
            let px = st.glide.value.clamp(0.0, max_scroll(list.len()));
            let first = (px / STRIDE).floor() as usize;
            let mut rows = column![].spacing(tok::XXS);
            for (pos, &k) in list.iter().enumerate().skip(first).take(ROWS + 1) {
                rows = rows.push(clip_row(s, &clips[k], pos, pos == st.selected));
            }
            let shown = list.len().min(ROWS);
            let rows = container(
                iced::widget::stack![
                    iced::widget::pin(rows)
                        .y(-(px - first as f32 * STRIDE))
                        .width(Length::Fill)
                ]
                .width(Length::Fill)
                .height(shown as f32 * STRIDE),
            )
            .clip(true)
            .height(shown as f32 * STRIDE);
            let n = list.len();
            let mut r = row![rows].spacing(tok::S);
            if n > ROWS {
                let h = ROWS as f32 * (ROW_H + 2.0);
                r = r.push(
                    // A wide grab area around the slim bar: press to jump, drag to scroll.
                    mouse_area(
                        container(scrollbar(
                            s,
                            st.glide.value / max_scroll(n).max(1.0),
                            n,
                            st.dragging,
                        ))
                        .width(14)
                        .center_x(14),
                    )
                    .on_press(Message::BarPress)
                    .on_move(move |p| Message::BarMove(p.y / h)),
                );
            }
            let list_area = mouse_area(container(r).width(LIST_W))
                .on_exit(Message::LeaveRows)
                .on_scroll(|d| match d {
                    iced::mouse::ScrollDelta::Lines { y, .. } => {
                        Message::Scroll(y * STRIDE * 1.5, true)
                    }
                    iced::mouse::ScrollDelta::Pixels { y, .. } => Message::Scroll(y, false),
                });
            row![list_area, preview_pane(s, st)]
                .spacing(tok::L)
                .height(ROWS as f32 * (ROW_H + 2.0))
                .into()
        }
    };
    // While the scrollbar is held, the pointer is followed anywhere on screen and the drag
    // ends wherever the button is let go (`panels::subscription`).
    container(column![header, field, body].spacing(tok::L))
        .padding(tok::L)
        .into()
}

/// The selected entry at full size: the picture, or the whole text, then what it is and what
/// can be done with it.
pub(super) fn preview_pane<'a>(s: Scheme, st: &'a State) -> Element<'a, Message> {
    let Some(c) = selected_clip(st) else {
        return Space::new().width(Length::Fill).into();
    };
    let (pw, ph) = preview_size();
    let body: Element<'_, Message> = if c.is_image() {
        match c
            .thumb
            .as_ref()
            .and_then(|t| ui::fitted_file_ready(t, pw - 24.0, ph - 24.0).flatten())
        {
            Some((h, w, ht)) => container(crate::ui::snap::snap(
                iced::widget::image(h)
                    .width(w)
                    .height(ht)
                    .opacity(s.opacity),
            ))
            .center(Length::Fill)
            .into(),
            None => container(icon(Icon::Image, tok::ICON_XXL, s.ink_faint))
                .center(Length::Fill)
                .into(),
        }
    } else {
        let full = st
            .preview
            .as_ref()
            .filter(|(id, _)| id == c.id())
            .map_or(c.preview.as_str(), |(_, t)| t.as_str());
        let shown: String = full.chars().take(3000).collect();
        iced::widget::scrollable(
            container(
                iced::widget::text(shown)
                    .size(text::BODY)
                    .line_height(iced::widget::text::LineHeight::Relative(1.45))
                    .color(s.ink)
                    .font(theme::FONT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .padding(tok::L),
        )
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0),
        ))
        .height(Length::Fill)
        .into()
    };
    let info = if c.is_image() {
        c.image_info()
    } else {
        c.text_info()
    };
    let pos = st.selected;
    let button = |name: &'static str, primary: bool, m: Message| {
        control(label_w(
            name,
            text::SMALL,
            if primary { s.on_accent } else { s.ink },
            theme::MEDIUM,
        ))
        .pill(
            s,
            if primary {
                crate::ui::control::Pill::Primary
            } else {
                crate::ui::control::Pill::Plain
            },
        )
        .on_press(m)
    };
    let foot = row![
        container(label(info, text::CAPTION, s.ink_dim)).width(Length::Fill),
        button(
            if pinned().iter().any(|p| p == c.id()) {
                "Unpin"
            } else {
                "Pin"
            },
            false,
            Message::Pin(pos)
        ),
        button("Delete", false, Message::Delete(pos)),
        button("Copy", false, Message::CopyOnly(pos)),
        button("Paste", true, Message::Copy(pos)),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    column![
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
                border: iced::Border::default().rounded(theme::tok::R_CARD()),
                ..container::Style::default()
            }),
        foot
    ]
    .spacing(tok::L)
    .width(Length::Fill)
    .into()
}

pub(super) fn placeholder<'a>() -> Element<'a, Message> {
    iced::widget::Space::new()
        .width(Length::Fill)
        .height(ROWS as f32 * (ROW_H + 2.0))
        .into()
}

pub(super) fn clip_row(s: Scheme, c: &Clip, pos: usize, selected: bool) -> Element<'_, Message> {
    // Left: the picture itself (whole), or nothing for text. Then what it is: a picture's
    // type, size and weight; text's own words and what kind of text it is.
    let thumb: Option<Element<'_, Message>> = match (&c.thumb, c.is_image()) {
        // Only a ready picture is drawn (made off the drawing thread, `row_pictures`).
        (Some(t), _) => Some(match ui::fitted_file_ready(t, THUMB_W, THUMB_H).flatten() {
            Some((h, w, ht)) => container(crate::ui::snap::snap(
                iced::widget::image(h)
                    .width(w)
                    .height(ht)
                    .opacity(s.opacity),
            ))
            .center_x(THUMB_W)
            .center_y(THUMB_H)
            .into(),
            None => container(icon(Icon::Image, tok::ICON_M, s.ink_faint))
                .center_x(THUMB_W)
                .center_y(THUMB_H)
                .into(),
        }),
        (None, true) => Some(
            container(icon(Icon::Image, tok::ICON_M, s.ink_faint))
                .center_x(THUMB_W)
                .center_y(THUMB_H)
                .into(),
        ),
        (None, false) => None,
    };
    let text_w = LIST_W - 24.0 - 44.0 - if thumb.is_some() { THUMB_W + 12.0 } else { 0.0 };
    // What it is under it, quietly: a picture's size, a link's site, text's length.
    let words: Element<'_, Message> = if c.is_image() {
        column![
            label_w("Image", text::BODY, s.ink, theme::MEDIUM),
            label(c.image_info(), text::CAPTION, s.ink_dim),
        ]
        .spacing(tok::XXS)
        .into()
    } else {
        let flat = c.preview.split_whitespace().collect::<Vec<_>>().join(" ");
        column![
            label(ui::fit(&flat, text::BODY, text_w), text::BODY, s.ink),
            label(c.text_info(), text::CAPTION, s.ink_dim),
        ]
        .spacing(tok::XXS)
        .into()
    };
    let mut content = row![].spacing(tok::L).align_y(Alignment::Center);
    if let Some(th) = thumb {
        content = content.push(th);
    }
    let content = content.push(words);
    // Pinned: a small pin at the end of the row.
    let content = if pinned().iter().any(|p| p == c.id()) {
        content.push(icon(Icon::Pin, tok::ICON_S, s.ink_dim))
    } else {
        content
    };
    // Remove: only on the selected row, so the list stays calm.
    let delete: Element<'_, Message> = if selected {
        crate::ui::remove_button(s, Message::Delete(pos))
    } else {
        Space::new().width(tok::H_SMALL).into()
    };
    control(
        row![container(content).width(Length::Fill), delete]
            .spacing(tok::L)
            .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::XL])
    .height(ROW_H)
    .width(Length::Fill)
    // A capsule, like the search field above it.
    .radius(ROW_H / 2.0)
    .kind(
        s,
        if selected {
            crate::ui::control::Kind::Chosen
        } else {
            crate::ui::control::Kind::Ghost
        },
    )
    .no_press_scale()
    .on_enter(Message::Hover(pos))
    .on_press(Message::Copy(pos))
    .into()
}

/// Where the visible rows sit in the whole list.
pub(super) fn scrollbar<'a>(s: Scheme, at: f32, n: usize, held: bool) -> Element<'a, Message> {
    // An overlay scroller like macOS's: a rounded thumb on a faint track the list's height,
    // soft at rest and brighter while held (a bare 4 px line read as a stray mark).
    let h = VIEW_H;
    let thumb = (h * ROWS as f32 / n as f32).clamp(40.0, h);
    let y = (h - thumb) * at.clamp(0.0, 1.0);
    let knob = container(Space::new())
        .width(6)
        .height(thumb)
        .style(move |_| container::Style {
            background: Some(
                s.ink
                    .scale_alpha(if held { 0.5 } else { 0.24 } * s.opacity)
                    .into(),
            ),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        });
    container(column![Space::new().height(y), knob].height(h))
        .width(6)
        .height(h)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        })
        .into()
}
