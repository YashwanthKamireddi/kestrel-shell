//! Settings › defaults: its page.

use super::*;
use crate::theme::tok;

/// An app's icon and name, small (default apps).
pub(super) fn app_chip<'a>(s: Scheme, a: &crate::services::apps::App) -> Element<'a, Message> {
    let ic: Element<'a, Message> = match crate::services::icons::lookup(&a.icon) {
        Some(p) => ui::image_file_faded(&p, 18.0, s.opacity),
        None => icon(Icon::AppWindow, tok::ICON, s.ink_dim).into(),
    };
    row![ic, label(a.name.clone(), text::SMALL, s.ink)]
        .spacing(tok::M)
        .align_y(Alignment::Center)
        .into()
}

pub(super) fn defaults_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    use crate::services::defaults::{KINDS, candidates};
    if st.apps.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Element<'_, Message>> = Vec::new();
    let mut rows = Vec::new();
    for (k, kind) in KINDS.iter().enumerate() {
        let now = st.defaults.get(k).cloned().flatten();
        let now_app = now
            .as_ref()
            .and_then(|id| st.apps.iter().find(|a| &a.id == id));
        let shown: Element<'_, Message> = match now_app {
            Some(a) => app_chip(s, a),
            None => label("None", text::SMALL, s.ink_dim).into(),
        };
        let open = st.default_open == Some(k);
        rows.push(row_of(
            s,
            kind.name,
            None,
            dropdown(s, shown, open, Message::DefaultOpen((!open).then_some(k))),
        ));
        if open {
            // The apps that can open it: a click makes one the default.
            for a in candidates(&st.apps, kind) {
                let on = now.as_deref() == Some(a.id.as_str());
                rows.push(Some(choice(
                    s,
                    app_chip(s, a),
                    on,
                    Message::DefaultSet(k, a.id.clone()),
                )));
            }
        }
    }
    out.extend(group(s, None, rows));
    // The terminal (xdg-terminals.list) and the editor terminal apps open ($EDITOR).
    let mut more = Vec::new();
    if !st.terminals.is_empty() {
        let now = st.terminal.clone().unwrap_or_default();
        let opts: Vec<(&str, bool, Message)> = st
            .terminals
            .iter()
            .map(|(id, name)| (name.as_str(), *id == now, Message::TerminalSet(id.clone())))
            .collect();
        more.push(row_of(s, "Terminal", None, segments(s, &opts)));
    }
    if !st.editors.is_empty() {
        let now = cx.config.editor.clone().unwrap_or_default();
        let opts: Vec<(&str, bool, Message)> = st
            .editors
            .iter()
            .map(|e| (*e, now == *e, Message::EditorSet((*e).to_string())))
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
