//! Settings › keyboard: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// The keyboard layouts option (read with the others; changed as a list).
pub(super) const LAYOUTS: crate::hypr_settings::Opt = crate::hypr_settings::Opt {
    label: "Layouts",
    option: "input:kb_layout",
    path: &["input", "kb_layout"],
    kind: crate::hypr_settings::Kind::Choice(&[]),
};

/// The keyboard layouts in use, in order (the first is the default).
pub(super) fn current_layouts(st: &State) -> Vec<String> {
    st.hypr
        .get(LAYOUTS.option)
        .map(crate::hypr_settings::Value::as_str)
        .unwrap_or_else(|| "us".into())
        .split(',')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// Adds or removes a keyboard layout (the last one stays).
pub(super) fn change_layouts(app: &mut App, code: String, add: bool) -> Task<AppMessage> {
    let st = &mut app.panels.settings;
    let mut now = current_layouts(st);
    if add {
        if !now.contains(&code) {
            now.push(code);
        }
        st.layouts_open = false;
    } else if now.len() > 1 {
        now.retain(|c| *c != code);
    } else {
        return Task::none();
    }
    let v = crate::hypr_settings::Value::Str(now.join(","));
    st.hypr.insert(LAYOUTS.option, v.clone());
    Task::future(crate::hypr_settings::save(LAYOUTS, v)).discard()
}

pub(super) fn keyboard_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    if st.hypr.is_empty() {
        return Vec::new();
    }
    let all = crate::hypr_settings::layouts_cached();
    let name_of = |code: &str| {
        all.iter()
            .find(|(c, _)| c == code)
            .map_or_else(|| code.to_string(), |(_, n)| n.clone())
    };
    let now = current_layouts(st);
    // Layouts: each by name, removable while there is more than one, more added from the
    // system's list. The first is the one at start.
    let mut rows: Vec<Option<Element<'_, AppMessage>>> = Vec::new();
    for code in &now {
        let remove: Element<'_, AppMessage> = if now.len() > 1 {
            crate::ui::remove_button(s, msg(Message::LayoutRemove(code.clone())))
        } else {
            iced::widget::Space::new().into()
        };
        rows.push(Some(
            container(
                row![
                    label(name_of(code), text::BODY, s.ink).width(Length::Fill),
                    remove
                ]
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::XL])
            .height(46)
            .align_y(Alignment::Center)
            .into(),
        ));
    }
    rows.push(Some(
        control(
            row![
                icon(Icon::Plus, 16.0, s.ink_dim),
                label("Add a layout", text::BODY, s.ink_dim)
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::XL])
        .height(42)
        .width(Length::Fill)
        .radius(size::ROW_RADIUS)
        .kind(s, crate::ui::control::Kind::Ghost)
        .no_press_scale()
        .on_press(msg(Message::LayoutsOpen(!st.layouts_open)))
        .into(),
    ));
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    if matches("keyboard layouts") {
        out.extend(group(s, Some("Layouts"), rows));
    }
    if st.layouts_open {
        let q = st.layout_query.trim().to_lowercase();
        let mut list = column![pick_search(
            s,
            "layout-search",
            &st.layout_query,
            Message::LayoutQuery
        )]
        .spacing(tok::XXS);
        for (code, name) in all
            .iter()
            .filter(|(c, n)| {
                !now.contains(c)
                    && (q.is_empty() || n.to_lowercase().contains(&q) || c.to_lowercase() == q)
            })
            .take(40)
        {
            list = list.push(pick_row(
                s,
                name.clone(),
                false,
                Message::LayoutAdd(code.clone()),
            ));
        }
        out.push(list.into());
    }
    let mut typing: Vec<Option<Element<'_, AppMessage>>> = crate::hypr_settings::KEYBOARD
        .iter()
        .map(|o| hypr_row(app, s, *o))
        .collect();
    // Keys with a job: Caps Lock, switching layouts (with more than one), a compose key. Each
    // owns its part of the one xkb options list.
    let options = st
        .hypr
        .get(crate::hypr_settings::KB_OPTIONS.option)
        .map(crate::hypr_settings::Value::as_str)
        .unwrap_or_default();
    for (k, c) in crate::hypr_settings::XKB.iter().enumerate() {
        if c.layouts && now.len() < 2 {
            continue;
        }
        let cur = crate::hypr_settings::xkb_value(&options, c);
        // Switching needs some key: the first is chosen when none is set.
        let cur = if c.layouts && cur.is_empty() {
            c.values[0].0.to_string()
        } else {
            cur
        };
        let opts: Vec<(&str, bool, AppMessage)> = c
            .values
            .iter()
            .map(|(v, l)| (*l, *v == cur, msg(Message::Xkb(k, (*v).to_string()))))
            .collect();
        typing.push(row_of(s, c.label, None, segments(s, &opts)));
    }
    out.extend(group(s, Some("Typing"), typing));
    out
}
