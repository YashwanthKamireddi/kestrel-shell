//! Settings › access: its page.

use super::*;

pub(super) fn access_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let ts = st.text_scale;
    let sizes: Vec<(&str, bool, Message)> = [("Default", 1.0f32), ("Large", 1.25), ("Larger", 1.5)]
        .into_iter()
        .map(|(n, f)| (n, (ts - f).abs() < 0.01, Message::TextScale(f)))
        .collect();
    let cur = cx.config.cursor_size.unwrap_or(24);
    let cursors: Vec<(&str, bool, Message)> = [("Default", 24u32), ("Large", 32), ("Larger", 48)]
        .into_iter()
        .map(|(n, v)| (n, cur == v, Message::CursorSize(v)))
        .collect();
    let mut rows = vec![
        row_of(s, "Text size in apps", None, segments(s, &sizes)),
        row_of(s, "Pointer size", None, segments(s, &cursors)),
    ];
    rows.extend(
        crate::hypr_settings::ACCESS
            .iter()
            .map(|o| hypr_row(st, cx, *o)),
    );
    if let Some(on) = st.mono {
        rows.push(row_of(
            s,
            "Mono audio",
            None,
            ui::switch(s, on, Message::MonoSet(!on)),
        ));
    }
    group(s, None, rows).into_iter().collect()
}
