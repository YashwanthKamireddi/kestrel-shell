//! Settings › access: its page.

use super::*;

pub(super) fn access_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let ts = st.text_scale;
    let sizes: Vec<(&str, bool, AppMessage)> =
        [("Default", 1.0f32), ("Large", 1.25), ("Larger", 1.5)]
            .into_iter()
            .map(|(n, f)| (n, (ts - f).abs() < 0.01, msg(Message::TextScale(f))))
            .collect();
    let cur = app.config.cursor_size.unwrap_or(24);
    let cursors: Vec<(&str, bool, AppMessage)> =
        [("Default", 24u32), ("Large", 32), ("Larger", 48)]
            .into_iter()
            .map(|(n, v)| (n, cur == v, msg(Message::CursorSize(v))))
            .collect();
    let mut rows = vec![
        row_of(s, "Text size in apps", None, segments(s, &sizes)),
        row_of(s, "Pointer size", None, segments(s, &cursors)),
    ];
    rows.extend(
        crate::hypr_settings::ACCESS
            .iter()
            .map(|o| hypr_row(app, s, *o)),
    );
    if let Some(on) = app.panels.settings.mono {
        rows.push(row_of(
            s,
            "Mono audio",
            None,
            ui::switch(s, on, msg(Message::MonoSet(!on))),
        ));
    }
    group(s, None, rows).into_iter().collect()
}
