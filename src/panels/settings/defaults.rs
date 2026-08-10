//! Settings › defaults: its page.

use super::*;
use crate::theme::tok;

/// An app's icon and name, small (default apps).
pub(super) fn app_chip<'a>(s: Scheme, a: &crate::services::apps::App) -> Element<'a, AppMessage> {
    let ic: Element<'a, AppMessage> = match crate::services::icons::lookup(&a.icon) {
        Some(p) => ui::image_file_faded(&p, 18.0, s.opacity),
        None => icon(Icon::AppWindow, tok::ICON, s.ink_dim).into(),
    };
    row![ic, label(a.name.clone(), text::SMALL, s.ink)]
        .spacing(tok::M)
        .align_y(Alignment::Center)
        .into()
}

pub(super) fn defaults_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    use crate::services::defaults::{KINDS, candidates};
    let st = &app.panels.settings;
    if st.apps.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    let mut rows = Vec::new();
    for (k, kind) in KINDS.iter().enumerate() {
        let now = st.defaults.get(k).cloned().flatten();
        let now_app = now
            .as_ref()
            .and_then(|id| st.apps.iter().find(|a| &a.id == id));
        let shown: Element<'_, AppMessage> = match now_app {
            Some(a) => app_chip(s, a),
            None => label("None", text::SMALL, s.ink_dim).into(),
        };
        let open = st.default_open == Some(k);
        rows.push(row_of(
            s,
            kind.name,
            None,
            control(
                row![shown, icon(Icon::ChevronDown, tok::ICON_S, s.ink_dim)]
                    .spacing(tok::S)
                    .align_y(Alignment::Center),
            )
            .padding([tok::XS, tok::L])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(msg(Message::DefaultOpen((!open).then_some(k))))
            .into(),
        ));
        if open {
            // The apps that can open it: a click makes one the default.
            for a in candidates(&st.apps, kind) {
                let on = now.as_deref() == Some(a.id.as_str());
                rows.push(Some(
                    control(
                        row![
                            app_chip(s, a),
                            iced::widget::Space::new().width(Length::Fill),
                            if on {
                                Element::from(icon(Icon::Check, tok::ICON, s.accent))
                            } else {
                                iced::widget::Space::new().into()
                            }
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::INDENT])
                    .height(36)
                    .width(Length::Fill)
                    .kind(s, crate::ui::control::Kind::Ghost)
                    .no_press_scale()
                    .on_press(msg(Message::DefaultSet(k, a.id.clone())))
                    .into(),
                ));
            }
        }
    }
    out.extend(group(s, None, rows));
    // The terminal (xdg-terminals.list) and the editor terminal apps open ($EDITOR).
    let mut more = Vec::new();
    if !st.terminals.is_empty() {
        let now = st.terminal.clone().unwrap_or_default();
        let opts: Vec<(&str, bool, AppMessage)> = st
            .terminals
            .iter()
            .map(|(id, name)| {
                (
                    name.as_str(),
                    *id == now,
                    msg(Message::TerminalSet(id.clone())),
                )
            })
            .collect();
        more.push(row_of(s, "Terminal", None, segments(s, &opts)));
    }
    if !st.editors.is_empty() {
        let now = app.config.editor.clone().unwrap_or_default();
        let opts: Vec<(&str, bool, AppMessage)> = st
            .editors
            .iter()
            .map(|e| (*e, now == *e, msg(Message::EditorSet((*e).to_string()))))
            .collect();
        more.push(row_of(
            s,
            "Editor in the terminal",
            None,
            segments(s, &opts),
        ));
    }
    out.extend(group(s, Some("Terminal"), more));
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
