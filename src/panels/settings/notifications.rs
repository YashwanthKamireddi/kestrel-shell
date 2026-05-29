//! Settings › notifications: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn notifications_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let c = &app.config;
    let quiet = c
        .quiet_from
        .clone()
        .filter(|t| !t.is_empty())
        .zip(c.quiet_to.clone().filter(|t| !t.is_empty()));
    let mut dnd_rows = vec![
        row_of(
            s,
            "Do not disturb",
            None,
            ui::switch(
                s,
                app.dnd,
                AppMessage::Panel(PanelMessage::Control(crate::panels::control::Message::Dnd)),
            ),
        ),
        row_of(
            s,
            "Quiet hours",
            None,
            ui::switch(
                s,
                quiet.is_some(),
                msg(Message::Quiet(if quiet.is_some() {
                    None
                } else {
                    Some(("22:00".into(), "07:00".into()))
                })),
            ),
        ),
    ];
    if let Some((from, to)) = &quiet {
        let st = &app.panels.settings;
        for (which, name, now) in [(0u8, "From", from), (1u8, "Until", to)] {
            let open = st.quiet_open == Some(which);
            dnd_rows.push(row_of(
                s,
                name,
                None,
                control(
                    row![
                        label(now.clone(), text::SMALL, s.ink),
                        icon(Icon::ChevronDown, 14.0, s.ink_dim)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([4, 10])
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Ghost)
                .on_press(msg(Message::QuietOpen((!open).then_some(which))))
                .into(),
            ));
            if open {
                // Every half hour of the day, a few per line.
                let mut grid = column![].spacing(2);
                let times: Vec<String> = (0..48)
                    .map(|h| format!("{:02}:{:02}", h / 2, (h % 2) * 30))
                    .collect();
                for chunk in times.chunks(6) {
                    let mut r = row![].spacing(2);
                    for t in chunk {
                        let on = t == now;
                        let pick = if which == 0 {
                            (t.clone(), to.clone())
                        } else {
                            (from.clone(), t.clone())
                        };
                        r = r.push(
                            control(
                                container(label(
                                    t.clone(),
                                    text::SMALL,
                                    if on { s.on_accent } else { s.ink },
                                ))
                                .center_x(Length::Fill),
                            )
                            .padding([5, 0])
                            .width(Length::Fill)
                            .radius(tok::FULL)
                            .kind(
                                s,
                                if on {
                                    crate::ui::control::Kind::Primary
                                } else {
                                    crate::ui::control::Kind::Ghost
                                },
                            )
                            .no_press_scale()
                            .on_press(msg(Message::Quiet(Some(pick)))),
                        );
                    }
                    grid = grid.push(r);
                }
                dnd_rows.push(Some(container(grid).padding([4, 12]).into()));
            }
        }
    }
    let dnd = group(s, None, dnd_rows);
    let rules = &app.config.notifications;
    let banner_opts: Vec<(&str, bool, AppMessage)> = [("Short", 3i64), ("Normal", 5), ("Long", 10)]
        .into_iter()
        .map(|(l, secs)| {
            (
                l,
                rules.banner == secs as u64,
                msg(Message::NotifyOpt("banner", secs.into())),
            )
        })
        .collect();
    let banners = group(
        s,
        None,
        vec![
            row_of(
                s,
                "Sound",
                None,
                ui::switch(
                    s,
                    rules.sound,
                    msg(Message::NotifyOpt("sound", (!rules.sound).into())),
                ),
            ),
            row_of(s, "Banner time", None, segments(s, &banner_opts)),
            row_of(
                s,
                "Urgent ones in Do not disturb",
                None,
                ui::switch(
                    s,
                    rules.urgent_in_dnd,
                    msg(Message::NotifyOpt(
                        "urgent_in_dnd",
                        (!rules.urgent_in_dnd).into(),
                    )),
                ),
            ),
        ],
    );
    // Every app that has sent something: banners and list, list only, or nothing; and whether
    // its banners sound.
    let apps = crate::state::get(|st| st.notifiers.clone());
    let rows = apps
        .into_iter()
        .filter(|name| matches(name) || matches("notifications"))
        .map(|name| {
            let now = if rules.muted.contains(&name) {
                2
            } else if rules.quiet.contains(&name) {
                1
            } else {
                0
            };
            let opts: Vec<(&str, bool, AppMessage)> = [("Show", 0u8), ("Quiet", 1), ("Off", 2)]
                .into_iter()
                .map(|(l, r)| (l, now == r, msg(Message::NotifyRule(name.clone(), r))))
                .collect();
            let ic: Element<'_, AppMessage> =
                match crate::services::icons::lookup(&crate::services::apps::icon_for_class(&name))
                {
                    Some(p) => ui::image_file_faded(&p, 18.0, s.opacity),
                    None => icon(Icon::Bell, 16.0, s.ink_dim).into(),
                };
            Some(
                container(
                    row![
                        ic,
                        label(name.clone(), text::BODY, s.ink).width(Length::Fill),
                        sound_toggle(s, &name, now, rules),
                        segments(s, &opts)
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
    [dnd, banners, group(s, Some("Apps"), rows)]
        .into_iter()
        .flatten()
        .collect()
}

/// An app's sound on/off: a speaker that strikes through when its banners are silent. Only for
/// apps whose banners show (rule 0) with sounds on; otherwise the same space stays empty, so the
/// rows line up.
pub(super) fn sound_toggle<'a>(
    s: Scheme,
    name: &str,
    rule: u8,
    rules: &config::NotificationRules,
) -> Element<'a, AppMessage> {
    if rule != 0 || !rules.sound {
        return iced::widget::Space::new().width(30).into();
    }
    let on = !rules.silent.iter().any(|a| a == name);
    control(icon(
        if on { Icon::Volume2 } else { Icon::VolumeX },
        16.0,
        if on { s.ink } else { s.ink_dim },
    ))
    .padding(7)
    .radius(tok::FULL)
    .kind(s, crate::ui::control::Kind::Ghost)
    .on_press(msg(Message::NotifySilent(name.to_string(), !on)))
    .into()
}
