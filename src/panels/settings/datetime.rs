//! Settings › datetime: its page.

use super::*;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn datetime_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let Some(t) = &st.time else {
        return Vec::new();
    };
    let zone_name = t.timezone.replace('_', " ");
    let mut out = Vec::new();
    let mut rows = vec![];
    if t.can_ntp {
        rows.push(row_of(
            s,
            "Set time automatically",
            None,
            ui::switch(s, t.ntp, msg(Message::Ntp(!t.ntp))),
        ));
    }
    let sunday = app.config.clock.week_starts_sunday;
    rows.push(row_of(
        s,
        "Week starts on",
        None,
        segments(
            s,
            &[
                ("Monday", !sunday, msg(Message::WeekStart(false))),
                ("Sunday", sunday, msg(Message::WeekStart(true))),
            ],
        ),
    ));
    rows.push(row_of(
        s,
        "Time zone",
        None,
        control(label(zone_name, text::SMALL, s.ink))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::ZonesOpen(!st.zones_open)))
            .into(),
    ));
    out.extend(group(s, None, rows));
    if st.zones_open {
        // Every zone, filtered as you type ("paris", "new york", "kolkata").
        let q = st.zone_query.trim().to_lowercase().replace(' ', "_");
        let search = container(
            text_input("Search", &st.zone_query)
                .id(iced::widget::Id::new("zone-search"))
                .on_input(|q| msg(Message::ZoneQuery(q)))
                .on_submit(msg(Message::Zone(
                    t.zones
                        .iter()
                        .find(|z| z.to_lowercase().contains(&q))
                        .cloned()
                        .unwrap_or_else(|| t.timezone.clone()),
                )))
                .padding([6, 10])
                .size(text::BODY)
                .font(theme::FONT)
                .style(move |_, _| text_input::Style {
                    background: Color::TRANSPARENT.into(),
                    border: iced::Border::default(),
                    icon: s.ink_dim,
                    placeholder: s.ink_faint,
                    value: s.ink,
                    selection: s.accent_soft,
                }),
        )
        .style(move |_| ui::field_style(s));
        let mut list = column![search].spacing(2);
        for z in t
            .zones
            .iter()
            .filter(|z| q.is_empty() || z.to_lowercase().contains(&q))
            .take(40)
        {
            let on = *z == t.timezone;
            list = list.push(
                control(
                    row![
                        label(
                            z.replace('_', " "),
                            text::BODY,
                            if on { s.accent } else { s.ink }
                        )
                        .width(Length::Fill),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([0, 12])
                .height(34)
                .width(Length::Fill)
                .radius(size::ROW_RADIUS)
                .kind(s, crate::ui::control::Kind::Ghost)
                .no_press_scale()
                .on_press(msg(Message::Zone(z.clone()))),
            );
        }
        out.push(list.into());
    }
    // Region formats: how dates, numbers and money read in apps (locale.conf's LC_* values).
    if !st.regions.is_empty() {
        let (lang, formats) = &st.region_now;
        let norm = |l: &str| l.to_lowercase().replace(".utf-8", "").replace(".utf8", "");
        let name_of = |l: &str| {
            st.regions
                .iter()
                .find(|r| norm(&r.locale) == norm(l))
                .map_or_else(|| l.to_string(), |r| r.name.clone())
        };
        let mut rows = vec![row_of(
            s,
            "Formats",
            Some("Dates, numbers and money in apps, from the next login"),
            control(
                row![
                    label(name_of(formats), text::SMALL, s.ink),
                    icon(
                        if st.region_open {
                            Icon::ChevronUp
                        } else {
                            Icon::ChevronDown
                        },
                        14.0,
                        s.ink_dim
                    )
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([4, 10])
            .radius(14.0)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(msg(Message::RegionOpen(!st.region_open)))
            .into(),
        )];
        if st.region_open {
            let q = st.region_query.trim().to_lowercase();
            let mut list = column![
                container(pick_search(
                    s,
                    "region-search",
                    &st.region_query,
                    Message::RegionQuery
                ))
                .padding([4, 0])
            ]
            .spacing(2);
            // The language's own formats first (no LC_* values).
            if norm(formats) != norm(lang) && q.is_empty() {
                list = list.push(pick_row(
                    s,
                    format!("Same as the language ({})", name_of(lang)),
                    false,
                    Message::RegionReset,
                ));
            }
            for r in st
                .regions
                .iter()
                .filter(|r| {
                    q.is_empty()
                        || r.name.to_lowercase().contains(&q)
                        || r.locale.to_lowercase().contains(&q)
                })
                .take(40)
            {
                list = list.push(pick_row(
                    s,
                    r.name.clone(),
                    norm(&r.locale) == norm(formats),
                    Message::RegionSet(r.clone()),
                ));
            }
            rows.push(Some(container(list).padding([4, 12]).into()));
        }
        out.extend(group(s, Some("Region"), rows));
    }
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
