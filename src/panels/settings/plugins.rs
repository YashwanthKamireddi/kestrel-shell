//! Settings › plugins: its page.

use super::*;

pub(super) fn plugins_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let rows = app
        .panels
        .settings
        .plugins
        .iter()
        .map(|(id, name, about, on)| {
            row_of(
                s,
                name,
                (!about.is_empty()).then_some(about.as_str()),
                ui::switch(s, *on, msg(Message::Plugin(id.clone(), !on))),
            )
        })
        .collect();
    // Where plugins go (each a folder with plugin.toml and its Lua), always reachable.
    let folder = crate::config::path().with_file_name("plugins");
    let mut out: Vec<Element<'_, AppMessage>> = group(s, None, rows).into_iter().collect();
    out.extend(group(
        s,
        None,
        vec![row_of(
            s,
            "Plugins folder",
            None,
            control(label("Open", text::SMALL, s.ink))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(msg(Message::OpenFile(folder)))
                .into(),
        )],
    ));
    out
}
