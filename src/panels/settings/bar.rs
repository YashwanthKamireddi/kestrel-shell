//! Settings › bar: its page.

use super::*;
use crate::theme::tok;

/// The bar's three sections as configured (old names already read in today's terms).
pub(super) fn layout(app: &App) -> [Vec<String>; 3] {
    let b = &app.config.bar;
    [b.left.clone(), b.center.clone(), b.right.clone()]
}

/// Where `id` is: (section, place).
pub(super) fn find_in(l: &[Vec<String>; 3], id: &str) -> Option<(usize, usize)> {
    l.iter()
        .enumerate()
        .find_map(|(s, list)| list.iter().position(|m| m == id).map(|p| (s, p)))
}

pub(super) fn save_layout(l: &[Vec<String>; 3]) {
    for (name, list) in ["left", "center", "right"].into_iter().zip(l) {
        config::set(&["bar"], name, config::list(list));
    }
}

/// A small round icon button in a layout row.
pub(super) fn mini<'a>(s: Scheme, i: Icon, m: Option<AppMessage>) -> Element<'a, AppMessage> {
    let on = m.is_some();
    let mut c = control(icon(i, 14.0, if on { s.ink } else { s.ink_faint }))
        .padding(tok::S)
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost);
    if let Some(m) = m {
        c = c.on_press(m);
    }
    c.into()
}

/// A module's options, under its row.
pub(super) fn module_options<'a>(
    app: &'a App,
    s: Scheme,
    mi: usize,
    m: &'static crate::modules::Info,
) -> Vec<Option<Element<'a, AppMessage>>> {
    let doc = &app.panels.settings.doc;
    m.options
        .iter()
        .enumerate()
        .map(|(oi, o)| {
            let v = o.value(doc);
            let control: Element<'_, AppMessage> = match o.kind {
                crate::modules::OptKind::Switch(_) => {
                    let b = v.as_bool().unwrap_or(false);
                    ui::switch(s, b, msg(Message::Opt(mi, oi, OptValue::Bool(!b))))
                }
                crate::modules::OptKind::Choice(choices) => {
                    let now = v.as_str().unwrap_or(choices[0].0).to_string();
                    let opts: Vec<(&str, bool, AppMessage)> = choices
                        .iter()
                        .map(|(val, label)| {
                            (
                                *label,
                                *val == now,
                                msg(Message::Opt(mi, oi, OptValue::Str(val))),
                            )
                        })
                        .collect();
                    segments(s, &opts)
                }
            };
            // Indented under the module's name: it belongs to it.
            row_of(s, o.label, None, control).map(|e| {
                container(e)
                    .padding(iced::Padding {
                        left: tok::INDENT,
                        ..iced::Padding::ZERO
                    })
                    .into()
            })
        })
        .collect()
}

/// Settings › Bar: the style, then the layout: Left, Centre and Right, each module in its
/// order with its controls (up, down, to the next section, remove) and its options under it;
/// then the modules not in the bar, to add; and the default layout back.
pub(super) fn bar_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let c = &app.config;
    let st = &app.panels.settings;
    let pills = c.bar.style == BarStyle::Pills;
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    out.extend(group(
        s,
        None,
        vec![
            row_of(
                s,
                "Style",
                None,
                segments(
                    s,
                    &[
                        ("Pills", pills, msg(Message::Style(BarStyle::Pills))),
                        ("Full", !pills, msg(Message::Style(BarStyle::Full))),
                    ],
                ),
            ),
            row_of(s, "Workspaces", None, stepper(s, i64::from(c.workspaces))),
        ],
    ));
    let l = layout(app);
    for (sec, title) in ["Left", "Centre", "Right"].into_iter().enumerate() {
        let list = &l[sec];
        let mut rows = Vec::new();
        for (at, id) in list.iter().enumerate() {
            let info = crate::modules::info(id);
            let mi = crate::modules::MODULES.iter().position(|m| m.id == id);
            let name = info.map_or_else(|| format!("{id} (Lua)"), |m| m.name.to_string());
            if !matches(&name) && !matches("bar") {
                continue;
            }
            let has_options = info.is_some_and(|m| !m.options.is_empty());
            let open = st.bar_open.as_deref() == Some(id.as_str());
            let lead: Element<'_, AppMessage> =
                icon(info.map_or(Icon::Code, |m| m.icon), 16.0, s.ink_dim).into();
            let title_el: Element<'_, AppMessage> = if has_options {
                control(
                    row![
                        label(name, text::BODY, s.ink),
                        icon(
                            if open {
                                Icon::ChevronUp
                            } else {
                                Icon::ChevronDown
                            },
                            14.0,
                            s.ink_dim
                        )
                    ]
                    .spacing(tok::S)
                    .align_y(Alignment::Center),
                )
                .padding([tok::XS, tok::M])
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Ghost)
                .no_press_scale()
                .on_press(msg(Message::BarOpen((!open).then(|| id.clone()))))
                .into()
            } else {
                container(label(name, text::BODY, s.ink))
                    .padding([tok::XS, tok::M])
                    .into()
            };
            let idc = id.clone();
            let mv = |d: i32, ok: bool| ok.then(|| msg(Message::BarMove(idc.clone(), d)));
            let sv = |d: i32, ok: bool| ok.then(|| msg(Message::BarSection(idc.clone(), d)));
            rows.push(Some(
                container(
                    row![
                        lead,
                        container(title_el).width(Length::Fill),
                        mini(s, Icon::ChevronUp, mv(-1, at > 0)),
                        mini(s, Icon::ChevronDown, mv(1, at + 1 < list.len())),
                        mini(s, Icon::ChevronLeft, sv(-1, sec > 0)),
                        mini(s, Icon::ChevronRight, sv(1, sec < 2)),
                        mini(s, Icon::X, Some(msg(Message::BarRemove(id.clone())))),
                    ]
                    .spacing(tok::XS)
                    .align_y(Alignment::Center),
                )
                .padding([tok::NONE, tok::XL])
                .height(46)
                .align_y(Alignment::Center)
                .into(),
            ));
            if open && let (Some(m), Some(mi)) = (info, mi) {
                rows.extend(module_options(app, s, mi, m));
            }
        }
        out.extend(group(s, Some(title), rows));
    }
    // Modules not in the bar: a click adds one to its usual section.
    let missing: Vec<Option<Element<'_, AppMessage>>> = crate::modules::MODULES
        .iter()
        .filter(|m| find_in(&l, m.id).is_none())
        .filter(|m| matches(m.name) || matches("bar"))
        .map(|m| {
            Some(
                control(
                    row![
                        icon(m.icon, 16.0, s.ink_dim),
                        label(m.name, text::BODY, s.ink).width(Length::Fill),
                        icon(Icon::Plus, 16.0, s.ink_dim),
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                )
                .padding([tok::NONE, tok::XL])
                .height(44)
                .width(Length::Fill)
                .kind(s, crate::ui::control::Kind::Ghost)
                .no_press_scale()
                .on_press(msg(Message::BarAdd(m.id.to_string())))
                .into(),
            )
        })
        .collect();
    out.extend(group(s, Some("Add to the bar"), missing));
    out.extend(group(
        s,
        None,
        vec![row_of(
            s,
            "Default layout",
            None,
            control(label(
                if st.bar_reset_armed {
                    "Reset?"
                } else {
                    "Reset"
                },
                text::SMALL,
                if st.bar_reset_armed { s.bad } else { s.ink },
            ))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::BarReset))
            .into(),
        )],
    ));
    out
}
