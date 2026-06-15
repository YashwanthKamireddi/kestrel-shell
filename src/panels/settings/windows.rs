//! Settings › windows: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn windows_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    use crate::hypr_settings::{WINDOW_BEHAVIOUR, WINDOW_LOOK};
    let mut out = Vec::new();
    out.extend(group(
        s,
        Some("Look"),
        WINDOW_LOOK.iter().map(|o| hypr_row(app, s, *o)).collect(),
    ));
    out.extend(group(
        s,
        Some("Behaviour"),
        WINDOW_BEHAVIOUR
            .iter()
            .map(|o| hypr_row(app, s, *o))
            .collect(),
    ));
    out.extend(group(s, Some("App rules"), rule_rows(app, s)));
    out
}

/// An app by its window class: its icon and its name.
pub(super) fn class_chip<'a>(s: Scheme, class: &str) -> (Element<'a, AppMessage>, String) {
    use crate::services::apps;
    let icon_el = ui::app_icon(s, class, 20.0, Icon::AppWindow);
    (icon_el, apps::name_for_class(class))
}

/// App rules: each app's rule (a click opens it), the open one's choices, and adding one by
/// choosing an app (open windows first, any installed app by search).
pub(super) fn rule_rows(app: &App, s: Scheme) -> Vec<Option<Element<'_, AppMessage>>> {
    let st = &app.panels.settings;
    let mut rows = Vec::new();
    for r in &st.rules {
        let (ic, name) = class_chip(s, &r.class);
        if !matches(&name) && !matches("rule") {
            continue;
        }
        let open = st.rule_edit.as_ref().is_some_and(|e| e.class == r.class);
        rows.push(Some(
            control(
                row![
                    ic,
                    column![
                        label(name, text::BODY, s.ink),
                        label(r.summary(), text::CAPTION, s.ink_dim)
                    ]
                    .spacing(tok::XXS)
                    .width(Length::Fill),
                    icon(
                        if open {
                            Icon::ChevronUp
                        } else {
                            Icon::ChevronDown
                        },
                        14.0,
                        s.ink_dim
                    ),
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::XL])
            .height(56)
            .width(Length::Fill)
            .kind(s, crate::ui::control::Kind::Ghost)
            .no_press_scale()
            .on_press(msg(Message::RuleOpen((!open).then(|| r.class.clone()))))
            .into(),
        ));
        if open && let Some(e) = &st.rule_edit {
            rows.extend(rule_editor(app, s, e));
        }
    }
    // A new rule being made (an app with none yet).
    if let Some(e) = &st.rule_edit
        && !st.rules.iter().any(|r| r.class == e.class)
    {
        let (ic, name) = class_chip(s, &e.class);
        rows.push(Some(
            container(
                row![ic, label(name, text::BODY, s.ink).width(Length::Fill)]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::XL])
            .height(46)
            .align_y(Alignment::Center)
            .into(),
        ));
        rows.extend(rule_editor(app, s, e));
    }
    match &st.rule_pick {
        None => rows.push(row_of(
            s,
            "Add a rule for an app",
            None,
            control(label("Choose…", text::SMALL, s.ink))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(msg(Message::RulePick(Some(String::new()))))
                .into(),
        )),
        Some(q) => {
            rows.push(Some(
                container(
                    container(
                        text_input("Search apps", q)
                            .id(iced::widget::Id::new("rule-search"))
                            .on_input(|t| msg(Message::RulePick(Some(t))))
                            .padding([tok::M, tok::L])
                            .size(text::BODY)
                            .font(theme::FONT)
                            .style(crate::ui::input_style(s)),
                    )
                    .width(Length::Fill)
                    .style(move |_| ui::field_style(s)),
                )
                .padding([tok::M, tok::XL])
                .into(),
            ));
            // Open windows first (their class is exact), then installed apps that match.
            let ql = q.trim().to_lowercase();
            let mut classes: Vec<String> = Vec::new();
            let free = |c: &str| !st.rules.iter().any(|r| r.class == c);
            for w in &app.hypr.windows {
                if !w.class.is_empty() && !classes.contains(&w.class) && free(&w.class) {
                    classes.push(w.class.clone());
                }
            }
            if !ql.is_empty() {
                for a in &st.rule_apps {
                    let class = if a.wm_class.is_empty() {
                        a.id.trim_end_matches(".desktop").to_string()
                    } else {
                        a.wm_class.clone()
                    };
                    if !classes.contains(&class) && free(&class) {
                        classes.push(class);
                    }
                }
            }
            let mut shown = 0;
            for c in classes {
                let (ic, name) = class_chip(s, &c);
                if !ql.is_empty()
                    && !name.to_lowercase().contains(&ql)
                    && !c.to_lowercase().contains(&ql)
                {
                    continue;
                }
                shown += 1;
                if shown > 8 {
                    break;
                }
                // The class only where it says more than the name (two apps named alike).
                let same = name.eq_ignore_ascii_case(&c);
                let mut line = row![ic, label(name, text::BODY, s.ink).width(Length::Fill)]
                    .spacing(tok::L)
                    .align_y(Alignment::Center);
                if !same {
                    line = line.push(label(c.clone(), text::CAPTION, s.ink_faint));
                }
                rows.push(Some(
                    control(line)
                        .padding([tok::NONE, tok::XL])
                        .height(44)
                        .width(Length::Fill)
                        .kind(s, crate::ui::control::Kind::Ghost)
                        .no_press_scale()
                        .on_press(msg(Message::RuleOpen(Some(c))))
                        .into(),
                ));
            }
            rows.push(Some(
                container(
                    row![
                        iced::widget::Space::new().width(Length::Fill),
                        control(label("Cancel", text::SMALL, s.ink))
                            .pill(s, crate::ui::control::Pill::Plain)
                            .on_press(msg(Message::RulePick(None))),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([tok::M, tok::XL])
                .into(),
            ));
        }
    }
    rows
}

/// The choices of the rule being edited, each applied at once.
pub(super) fn rule_editor<'a>(
    app: &'a App,
    s: Scheme,
    e: &crate::hypr_settings::WindowRule,
) -> Vec<Option<Element<'a, AppMessage>>> {
    use crate::hypr_settings::RULE_SIZES;
    let set = |f: &dyn Fn(&mut crate::hypr_settings::WindowRule)| {
        let mut r = e.clone();
        f(&mut r);
        msg(Message::RuleSet(r))
    };
    let mut rows = vec![row_of(
        s,
        "Opens",
        None,
        segments(
            s,
            &[
                ("As usual", e.float.is_none(), set(&|r| r.float = None)),
                (
                    "Floating",
                    e.float == Some(true),
                    set(&|r| r.float = Some(true)),
                ),
                (
                    "Tiled",
                    e.float == Some(false),
                    set(&|r| r.float = Some(false)),
                ),
            ],
        ),
    )];
    if e.float == Some(true) {
        let mut sizes = vec![("As it opens", e.size.is_none(), set(&|r| r.size = None))];
        for (key, name, ..) in RULE_SIZES {
            sizes.push((
                name,
                e.size.as_deref() == Some(key),
                set(&|r| r.size = Some(key.to_string())),
            ));
        }
        rows.push(row_of(s, "Size", None, segments(s, &sizes)));
        rows.push(row_of(
            s,
            "In the middle of the screen",
            None,
            ui::switch(s, e.center, set(&|r| r.center = !e.center)),
        ));
        rows.push(row_of(
            s,
            "On top, on every workspace",
            None,
            ui::switch(s, e.pin, set(&|r| r.pin = !e.pin)),
        ));
    }
    let count = app.config.workspaces.clamp(1, 10);
    let mut ws: Vec<(&str, bool, AppMessage)> =
        vec![("Any", e.workspace.is_none(), set(&|r| r.workspace = None))];
    for n in 1..=count {
        ws.push((
            ws_name(n),
            e.workspace == Some(n),
            set(&|r| r.workspace = Some(n)),
        ));
    }
    rows.push(row_of(s, "Workspace", None, segments(s, &ws)));
    let opacity: Vec<(&str, bool, AppMessage)> =
        [("100%", 100u32), ("95%", 95), ("90%", 90), ("80%", 80)]
            .into_iter()
            .map(|(n, v)| {
                (
                    n,
                    e.opacity.unwrap_or(100) == v,
                    set(&|r| r.opacity = (v < 100).then_some(v)),
                )
            })
            .collect();
    rows.push(row_of(s, "Opacity", None, segments(s, &opacity)));
    let saved = app.panels.settings.rules.iter().any(|r| r.class == e.class);
    let mut buttons = row![iced::widget::Space::new().width(Length::Fill)].spacing(tok::M);
    if saved {
        buttons = buttons.push(
            control(label("Remove rule", text::SMALL, s.ink))
                .pill(s, crate::ui::control::Pill::Plain)
                .on_press(msg(Message::RuleRemove(e.class.clone()))),
        );
    }
    buttons = buttons.push(
        control(label("Done", text::SMALL, s.ink))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::RuleOpen(None))),
    );
    rows.push(Some(
        container(buttons.align_y(Alignment::Center))
            .padding([tok::M, tok::XL])
            .into(),
    ));
    rows
}

/// "1" to "10" as static text (segment labels borrow them).
pub(super) fn ws_name(n: u32) -> &'static str {
    const NAMES: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
    NAMES[(n.clamp(1, 10) - 1) as usize]
}
