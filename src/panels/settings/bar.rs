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
    for (sec, title) in ["Left side", "Centre", "Right side"]
        .into_iter()
        .enumerate()
    {
        let list = &l[sec];
        let mut rows = Vec::new();
        for (at, id) in list.iter().enumerate() {
            let info = crate::modules::info(id);
            let mi = crate::modules::MODULES.iter().position(|m| m.id == id);
            let name = info.map_or_else(|| format!("{id} (Lua)"), |m| m.name.to_string());
            if !matches(&name) && !matches("bar") {
                continue;
            }
            let lead: Element<'_, AppMessage> =
                icon(info.map_or(Icon::Code, |m| m.icon), 18.0, s.ink).into();
            let idc = id.clone();
            let mv = |d: i32, ok: bool| ok.then(|| msg(Message::BarMove(idc.clone(), d)));
            // Where it sits, said in words (three arrows to the sides read as random).
            let place = segments(
                s,
                &[
                    (
                        "Left",
                        sec == 0,
                        msg(Message::BarSection(id.clone(), 0 - sec as i32)),
                    ),
                    (
                        "Centre",
                        sec == 1,
                        msg(Message::BarSection(id.clone(), 1 - sec as i32)),
                    ),
                    (
                        "Right",
                        sec == 2,
                        msg(Message::BarSection(id.clone(), 2 - sec as i32)),
                    ),
                ],
            );
            rows.push(Some(
                container(
                    row![
                        lead,
                        iced::widget::column![
                            label(name, text::BODY, s.ink),
                            label(describe(id), text::SMALL, s.ink_dim),
                        ]
                        .spacing(tok::XXS)
                        .width(Length::Fill),
                        place,
                        // Order within its side: earlier (left) or later (right).
                        mini(s, Icon::ChevronLeft, mv(-1, at > 0)),
                        mini(s, Icon::ChevronRight, mv(1, at + 1 < list.len())),
                        mini(s, Icon::X, Some(msg(Message::BarRemove(id.clone())))),
                    ]
                    .spacing(tok::M)
                    .align_y(Alignment::Center),
                )
                .padding([tok::M, tok::XL])
                .align_y(Alignment::Center)
                .into(),
            ));
            // Its options, always shown under it (they were behind a hidden arrow).
            if let (Some(m), Some(mi)) = (info, mi) {
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
                        icon(m.icon, 18.0, s.ink),
                        iced::widget::column![
                            label(m.name, text::BODY, s.ink),
                            label(describe(m.id), text::SMALL, s.ink_dim),
                        ]
                        .spacing(tok::XXS)
                        .width(Length::Fill),
                        icon(Icon::Plus, 16.0, s.accent),
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                )
                .padding([tok::M, tok::XL])
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

/// What a bar item is, in a few words (its name alone said too little: "Time and island").
fn describe(id: &str) -> &'static str {
    match id {
        "workspaces" => "Your workspaces; click one to go there",
        "app" => "The name of the app you are using",
        "island" => "The time and date; music, alerts and levels show here too",
        "clock" => "The time and date",
        "media" => "The song playing, with its cover",
        "weather" => "The temperature now; click for the forecast",
        "airpods" => "AirPods battery while they are connected",
        "tray" => "Icons of apps running in the background",
        "system" => "How busy the processor and memory are",
        "updates" => "How many updates are waiting",
        "privacy" => "Shows when the microphone, camera or screen is in use",
        "network" => "Wi-Fi or wired connection",
        "bluetooth" => "Bluetooth and connected devices",
        "volume" => "Sound volume and outputs",
        "battery" => "Battery level and charging",
        "notifications" => "Unread notifications (inside the island when it is in the bar)",
        "power" => "Lock, sleep, restart and shut down",
        _ => "A widget from a Lua script",
    }
}
