//! Settings › startup: its page.

use super::*;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A file change for Startup apps, off the UI thread, then the list read again.
pub(super) fn startup_job(
    f: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Task<AppMessage> {
    Task::perform(
        async move {
            let r = tokio::task::spawn_blocking(f)
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let list = tokio::task::spawn_blocking(crate::services::autostart::list)
                .await
                .unwrap_or_default();
            (r, list)
        },
        |(r, list)| {
            AppMessage::Batch(vec![
                msg(Message::Startup(list, Vec::new())),
                msg(Message::Done(r)),
            ])
        },
    )
}

pub(super) fn startup_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let mut rows: Vec<Option<Element<'_, AppMessage>>> = st
        .startup
        .iter()
        .map(|e| {
            let ic: Element<'_, AppMessage> = match crate::services::icons::lookup(&e.icon) {
                Some(p) => ui::image_file_faded(&p, 18.0, s.opacity),
                None => icon(Icon::AppWindow, 16.0, s.ink_dim).into(),
            };
            let remove: Element<'_, AppMessage> = if e.user_only {
                control(icon(Icon::X, 15.0, s.ink_dim))
                    .padding(5)
                    .radius(13.0)
                    .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
                    .on_press(msg(Message::StartupRemove(e.id.clone())))
                    .into()
            } else {
                iced::widget::Space::new().into()
            };
            Some(
                container(
                    row![
                        ic,
                        label(e.name.clone(), text::BODY, s.ink).width(Length::Fill),
                        remove,
                        ui::switch(
                            s,
                            e.enabled,
                            msg(Message::StartupOn(e.id.clone(), !e.enabled))
                        ),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([0, 16])
                .height(46)
                .align_y(Alignment::Center)
                .into(),
            )
        })
        .collect();
    rows.push(Some(
        control(
            row![
                icon(Icon::Plus, 16.0, s.ink_dim),
                label("Add an app", text::BODY, s.ink_dim)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([0, 16])
        .height(42)
        .width(Length::Fill)
        .radius(size::ROW_RADIUS)
        .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
        .no_press_scale()
        .on_press(msg(Message::StartupAddOpen(!st.startup_add)))
        .into(),
    ));
    let mut out: Vec<Element<'_, AppMessage>> = group(s, None, rows).into_iter().collect();
    if st.startup_add {
        let q = st.startup_query.trim().to_lowercase();
        let mut list = column![pick_search(
            s,
            "startup-search",
            &st.startup_query,
            Message::StartupQuery
        )]
        .spacing(2);
        for a in st
            .apps
            .iter()
            .filter(|a| !st.startup.iter().any(|e| e.id == a.id))
            .filter(|a| q.is_empty() || a.name.to_lowercase().contains(&q))
            .take(30)
        {
            list = list.push(
                control(app_chip(s, a))
                    .padding([0, 12])
                    .height(36)
                    .width(Length::Fill)
                    .radius(size::ROW_RADIUS)
                    .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
                    .no_press_scale()
                    .on_press(msg(Message::StartupAdd(a.id.clone()))),
            );
        }
        out.push(list.into());
    }
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
