//! Settings › startup: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

/// A file change for Startup apps, off the UI thread, then the list read again.
pub(super) fn startup_job(
    f: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Task<Message> {
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
        |(r, list)| Message::Batch(vec![Message::Startup(list, Vec::new()), Message::Done(r)]),
    )
}

pub(super) fn startup_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let mut rows: Vec<Option<Element<'_, Message>>> = st
        .startup
        .iter()
        .map(|e| {
            let ic: Element<'_, Message> = match crate::services::icons::lookup(&e.icon) {
                Some(p) => ui::image_file_faded(&p, 18.0, s.opacity),
                None => icon(Icon::AppWindow, tok::ICON, s.ink_dim).into(),
            };
            let remove: Element<'_, Message> = if e.user_only {
                crate::ui::remove_button(s, Message::StartupRemove(e.id.clone()))
            } else {
                iced::widget::Space::new().into()
            };
            Some(
                container(
                    row![
                        ic,
                        label(e.name.clone(), text::BODY, s.ink).width(Length::Fill),
                        remove,
                        ui::switch(s, e.enabled, Message::StartupOn(e.id.clone(), !e.enabled)),
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                )
                .padding([tok::NONE, tok::XL])
                .height(46)
                .align_y(Alignment::Center)
                .into(),
            )
        })
        .collect();
    rows.push(Some(
        control(
            row![
                icon(Icon::Plus, tok::ICON, s.ink_dim),
                label("Add an app", text::BODY, s.ink_dim)
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::XL])
        .height(42)
        .width(Length::Fill)
        .radius(tok::R_ROW)
        .kind(s, crate::ui::control::Kind::Ghost)
        .no_press_scale()
        .on_press(Message::StartupAddOpen(!st.startup_add))
        .into(),
    ));
    let mut out: Vec<Element<'_, Message>> = group(s, None, rows).into_iter().collect();
    if st.startup_add {
        let q = st.startup_query.trim().to_lowercase();
        let mut list = column![pick_search(
            s,
            "startup-search",
            &st.startup_query,
            Message::StartupQuery
        )]
        .spacing(tok::XXS);
        for a in st
            .apps
            .iter()
            .filter(|a| !st.startup.iter().any(|e| e.id == a.id))
            .filter(|a| q.is_empty() || a.name.to_lowercase().contains(&q))
            .take(30)
        {
            list = list.push(
                control(app_chip(s, a))
                    .padding([tok::NONE, tok::L])
                    .height(36)
                    .width(Length::Fill)
                    .radius(tok::R_ROW)
                    .kind(s, crate::ui::control::Kind::Ghost)
                    .no_press_scale()
                    .on_press(Message::StartupAdd(a.id.clone())),
            );
        }
        out.push(list.into());
    }
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
