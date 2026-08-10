//! Settings › power: its page.

use super::*;
use crate::theme::tok;

/// "30 min", "1 hour", "1 h 30 min".
pub(super) fn idle_time(secs: u64) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (1, 0) => "1 hour".into(),
        (h, 0) => format!("{h} hours"),
        (h, m) => format!("{h} h {m} min"),
    }
}

pub(super) fn power_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    use crate::services::idle::Step;
    use crate::services::power::Profile;
    let st = &app.panels.settings;
    let mut out: Vec<Element<'_, AppMessage>> = Vec::new();
    if let Some(p) = app.power.profile {
        out.extend(group(
            s,
            None,
            vec![row_of(
                s,
                "Power mode",
                None,
                segments(
                    s,
                    &[Profile::PowerSaver, Profile::Balanced, Profile::Performance]
                        .map(|q| (q.label(), q == p, msg(Message::Profile(q)))),
                ),
            )],
        ));
    }
    if app.power.has_battery {
        let mut rows = vec![value_row(
            s,
            "Charge",
            format!(
                "{:.0}%{}",
                app.power.percent,
                if app.power.full {
                    " · Full"
                } else if app.power.charging {
                    " · Charging"
                } else if app.power.held {
                    " · Charging on hold"
                } else {
                    ""
                }
            ),
        )];
        if let Some(h) = &st.health {
            rows.push(value_row(s, "Health", format!("{}% of new", h.percent)));
            if let Some(c) = h.cycles {
                rows.push(value_row(s, "Charge cycles", c.to_string()));
            }
        }
        // Charging: the firmware's modes (Stop at 80% keeps a battery that lives on the
        // charger from wearing out).
        if let Some((offered, now)) = &st.charging {
            let opts: Vec<(&str, bool, AppMessage)> = offered
                .iter()
                .map(|c| (kept(c.label()), c == now, msg(Message::Charging(*c))))
                .collect();
            rows.push(row_of(s, "Charging", None, segments(s, &opts)));
        }
        // Low battery (UPower's policy): when to warn, and what happens when nearly empty.
        if let Some(l) = &st.low {
            let action = l.action.clone();
            let warn: Vec<(&str, bool, AppMessage)> = [10u32, 15, 20, 25, 30]
                .into_iter()
                .map(|p| {
                    (
                        kept(format!("{p}%")),
                        l.warn_at == p,
                        msg(Message::LowSet(p, action.clone())),
                    )
                })
                .collect();
            rows.push(row_of(
                s,
                "Warn when battery is at",
                None,
                segments(s, &warn),
            ));
            let suspend = matches!(l.action.as_str(), "Suspend" | "Sleep");
            let poweroff = l.action == "PowerOff";
            let mut ends = vec![
                (
                    "Power off",
                    poweroff,
                    msg(Message::LowSet(l.warn_at, "PowerOff".into())),
                ),
                (
                    "Suspend",
                    suspend,
                    msg(Message::LowSet(l.warn_at, "Suspend".into())),
                ),
            ];
            // An action set elsewhere (hibernate) stays shown as it is.
            if !suspend && !poweroff {
                ends.push((kept(l.action.clone()), true, msg(Message::DisplayTick)));
            }
            rows.push(row_of(
                s,
                kept(format!("When it reaches {}%", l.action_at)),
                None,
                segments(s, &ends),
            ));
        }
        out.extend(group(s, Some("Battery"), rows));
    }
    // Idle steps: each a list of times to pick from (the one set now kept even if unusual).
    const TIMES: [u64; 9] = [60, 120, 300, 600, 900, 1800, 2700, 3600, 5400];
    let mut rows = Vec::new();
    for (step, secs) in &st.idle {
        let Some(secs) = secs else { continue };
        let name = match step {
            Step::Lock => "Lock after",
            Step::ScreenOff => "Turn screen off after",
            Step::Suspend => "Suspend after",
        };
        let open = st.idle_open == Some(*step);
        rows.push(row_of(
            s,
            name,
            None,
            control(
                row![
                    label(idle_time(*secs), text::SMALL, s.ink),
                    icon(Icon::ChevronDown, tok::ICON_S, s.ink_dim)
                ]
                .spacing(tok::S)
                .align_y(Alignment::Center),
            )
            .padding([tok::XS, tok::L])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(msg(Message::IdleOpen((!open).then_some(*step))))
            .into(),
        ));
        if open {
            let mut times: Vec<u64> = TIMES.to_vec();
            if !times.contains(secs) {
                times.push(*secs);
                times.sort_unstable();
            }
            for t in times {
                let on = t == *secs;
                rows.push(Some(
                    control(
                        row![
                            label(idle_time(t), text::BODY, if on { s.accent } else { s.ink })
                                .width(Length::Fill),
                            if on {
                                Element::from(icon(Icon::Check, tok::ICON, s.accent))
                            } else {
                                iced::widget::Space::new().into()
                            }
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::INDENT])
                    .height(34)
                    .width(Length::Fill)
                    .kind(s, crate::ui::control::Kind::Ghost)
                    .no_press_scale()
                    .on_press(msg(Message::Idle(*step, t)))
                    .into(),
                ));
            }
        }
    }
    out.extend(group(s, Some("When idle"), rows));
    // The lid and the power button (logind): each row's choices, a change asks once.
    if let Some(l) = st.lid {
        use crate::services::power::Action as A;
        let pick = |now: A,
                    choices: &[(&'static str, A)],
                    put: fn(&mut crate::services::power::Lid, A)| {
            let opts: Vec<(&str, bool, AppMessage)> = choices
                .iter()
                .map(|(name, a)| {
                    let mut next = l;
                    put(&mut next, *a);
                    (*name, now == *a, msg(Message::LidSet(next)))
                })
                .collect();
            segments(s, &opts)
        };
        let lid_choices = [
            ("Suspend", A::Suspend),
            ("Lock", A::Lock),
            ("Nothing", A::Nothing),
        ];
        let key_choices = [
            ("Suspend", A::Suspend),
            ("Power off", A::PowerOff),
            ("Nothing", A::Nothing),
        ];
        out.extend(group(
            s,
            Some("Lid and power button"),
            vec![
                row_of(
                    s,
                    "Lid closed on battery",
                    None,
                    pick(l.battery, &lid_choices, |x, a| x.battery = a),
                ),
                row_of(
                    s,
                    "Lid closed while plugged in",
                    None,
                    pick(l.charging, &lid_choices, |x, a| x.charging = a),
                ),
                row_of(
                    s,
                    "Lid closed with a display connected",
                    None,
                    pick(l.docked, &lid_choices, |x, a| x.docked = a),
                ),
                row_of(
                    s,
                    "Power button",
                    None,
                    pick(l.power_key, &key_choices, |x, a| x.power_key = a),
                ),
            ],
        ));
    }
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
