//! Settings › bar: its page.

use super::*;
use crate::theme::tok;
use iced::widget::{Space, column, mouse_area};

/// The bar's three sections as configured (old names already read in today's terms).
pub(super) fn layout(cx: &Cx<'_>) -> [Vec<String>; 3] {
    let b = &cx.config.bar;
    [b.left.clone(), b.center.clone(), b.right.clone()]
}

/// Where `id` is: (section, place).
pub(super) fn find_in(l: &[Vec<String>; 3], id: &str) -> Option<(usize, usize)> {
    l.iter()
        .enumerate()
        .find_map(|(s, list)| list.iter().position(|m| m == id).map(|p| (s, p)))
}

/// The bar's sections, as the editor changes them. Each move gives the new layout, or None
/// when nothing would change (nothing is then written).
pub(super) type Layout = [Vec<String>; 3];

/// `id` one or more places along its section (-1: towards its start, 1: towards its end).
pub(super) fn moved(l: &Layout, id: &str, d: i32) -> Option<Layout> {
    let (sec, at) = find_in(l, id)?;
    let to = usize::try_from(at as i32 + d).ok()?;
    if to >= l[sec].len() || to == at {
        return None;
    }
    let mut l = l.clone();
    l[sec].swap(at, to);
    Some(l)
}

/// `id` into another section, `d` sections over (-1: left, 1: right), at that section's near
/// end (going right: its start; going left: its end).
pub(super) fn to_section(l: &Layout, id: &str, d: i32) -> Option<Layout> {
    let (sec, at) = find_in(l, id)?;
    let to = sec as i32 + d;
    if !(0..3).contains(&to) {
        return None;
    }
    let mut l = l.clone();
    let m = l[sec].remove(at);
    if d > 0 {
        l[to as usize].insert(0, m);
    } else {
        l[to as usize].push(m);
    }
    Some(l)
}

/// `id`, not in the bar yet, added to `lane`'s end (the right side: before Power, its place).
pub(super) fn added(l: &Layout, lane: usize, id: &str) -> Option<Layout> {
    if find_in(l, id).is_some() || lane > 2 {
        return None;
    }
    let mut l = l.clone();
    let at = if lane == 2 {
        l[2].iter().position(|m| m == "power").unwrap_or(l[2].len())
    } else {
        l[lane].len()
    };
    l[lane].insert(at, id.to_string());
    Some(l)
}

/// `id` dragged and let go before place `to` of `lane` (`to` counted with `id` still where it
/// was, as the drop targets are drawn).
pub(super) fn dropped(l: &Layout, id: &str, lane: usize, to: usize) -> Option<Layout> {
    let (sec, from) = find_in(l, id)?;
    // Back where it was: nothing to save.
    if lane > 2 || (sec == lane && (to == from || to == from + 1)) {
        return None;
    }
    let mut l = l.clone();
    l[sec].remove(from);
    let to = if sec == lane && from < to { to - 1 } else { to };
    let to = to.min(l[lane].len());
    l[lane].insert(to, id.to_string());
    Some(l)
}

/// `id` taken out of the bar.
pub(super) fn removed(l: &Layout, id: &str) -> Layout {
    let mut l = l.clone();
    for list in &mut l {
        list.retain(|m| m != id);
    }
    l
}

pub(super) fn save_layout(l: &[Vec<String>; 3]) -> Act<Message> {
    Act::Change(Change::Settings(
        ["left", "center", "right"]
            .into_iter()
            .zip(l)
            .map(|(name, list)| config::Edit::new(&["bar"], name, config::list(list)))
            .collect(),
    ))
}

/// A small round icon button in a layout row.
pub(super) fn mini<'a>(s: Scheme, i: Icon, m: Option<Message>) -> Element<'a, Message> {
    let on = m.is_some();
    let mut c = control(icon(i, tok::ICON_S, if on { s.ink } else { s.ink_faint }))
        .padding(tok::S)
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost);
    if let Some(m) = m {
        c = c.on_press(m);
    }
    c.into()
}

/// A module's options (in its inspector).
pub(super) fn module_options<'a>(
    st: &'a State,
    cx: &Cx<'a>,
    mi: usize,
    m: &'static crate::modules::Info,
) -> Vec<Option<Element<'a, Message>>> {
    let s = cx.scheme;
    let doc = &st.doc;
    m.options
        .iter()
        .enumerate()
        .map(|(oi, o)| {
            let v = o.value(doc);
            let control: Element<'_, Message> = match o.kind {
                crate::modules::OptKind::Switch(_) => {
                    let b = v.as_bool().unwrap_or(false);
                    ui::switch(s, b, Message::Opt(mi, oi, OptValue::Bool(!b)))
                }
                crate::modules::OptKind::Choice(choices) => {
                    let now = v.as_str().unwrap_or(choices[0].0).to_string();
                    let opts: Vec<(&str, bool, Message)> = choices
                        .iter()
                        .map(|(val, label)| {
                            (
                                *label,
                                *val == now,
                                Message::Opt(mi, oi, OptValue::Str(val)),
                            )
                        })
                        .collect();
                    segments(s, &opts)
                }
            };
            row_of(s, o.label, None, control)
        })
        .collect()
}

/// The three lanes' names, left to right.
const LANES: [&str; 3] = ["Left", "Centre", "Right"];

/// A lane's width: the page's reading width shared by three lanes and their gaps.
const LANE_W: f32 = (READING_W - 2.0 * tok::M) / 3.0;

/// Settings › Bar, as Noctalia's bar editor (src/shell/settings/bar_widget_editor.cpp): the
/// style, then the three lanes side by side, each a column of compact cards (a grip to drag,
/// the module, its options ⚙, remove ✕) with "+" for the modules not in the bar; a card is
/// dragged within or between lanes, an accent line showing where it lands. A module's options
/// and its place (for the keyboard) show only when it is picked. Design call 2026-11-17: the old
/// page's Left/Centre/Right, arrows and ✕ on every row were "many clicks, cluttered".
pub(super) fn bar_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let c = &cx.config;
    let pills = c.bar.style == BarStyle::Pills;
    let mut out: Vec<Element<'_, Message>> = Vec::new();
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
                        ("Pills", pills, Message::Style(BarStyle::Pills)),
                        ("Full", !pills, Message::Style(BarStyle::Full)),
                    ],
                ),
            ),
            row_of(s, "Workspaces", None, stepper(s, i64::from(c.workspaces))),
        ],
    ));
    if !matches("bar") && !matches("layout") && !matches("modules") {
        return out;
    }
    let l = layout(cx);
    let lanes = row((0..3).map(|lane| lane_view(st, cx, &l, lane)))
        .spacing(tok::M)
        .align_y(Alignment::Start);
    out.push(
        column![
            container(label_w("Layout", text::SMALL, s.ink_dim, theme::SEMIBOLD))
                .padding([tok::NONE, tok::XS]),
            lanes,
            container(label(
                "Drag a module by its grip to move it. ⚙ shows its options.",
                text::CAPTION,
                s.ink_faint,
            ))
            .padding([tok::NONE, tok::XS]),
        ]
        .spacing(tok::S)
        .into(),
    );
    if let Some(lane) = st.bar_adding {
        out.extend(group(s, Some(ADD_TITLES[lane]), missing(cx, &l, lane)));
    }
    if let Some(id) = st.bar_pick.as_deref()
        && let Some(sec) = find_in(&l, id).map(|(sec, _)| sec)
    {
        out.extend(inspector(st, cx, &l, id, sec));
    }
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
            .on_press(Message::BarReset)
            .into(),
        )],
    ));
    out
}

const ADD_TITLES: [&str; 3] = ["Add to the left", "Add to the centre", "Add to the right"];

/// A thin accent line where a dragged card would land (clear when it would not).
fn drop_line<'a>(s: Scheme, show: bool) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(tok::XXS)
        .style(move |_| container::Style {
            background: show.then(|| s.accent.into()),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        })
        .into()
}

/// One lane: its name and "+", its cards in order, and room below to drop at its end.
fn lane_view<'a>(
    st: &'a State,
    cx: &Cx<'a>,
    l: &[Vec<String>; 3],
    lane: usize,
) -> Element<'a, Message> {
    let s = cx.scheme;
    let dragging = st.bar_drag.is_some();
    let at = |i: usize| dragging && st.bar_target == Some((lane, i));
    let adding = st.bar_adding == Some(lane);
    let head = row![
        label_w(LANES[lane], text::SMALL, s.ink, theme::SEMIBOLD).width(Length::Fill),
        control(icon(
            Icon::Plus,
            tok::ICON_S,
            if adding { s.accent } else { s.ink_dim }
        ))
        .padding(tok::XS)
        .radius(tok::FULL)
        .kind(
            s,
            if adding {
                crate::ui::control::Kind::Chosen
            } else {
                crate::ui::control::Kind::Ghost
            },
        )
        .on_press(Message::BarAddOpen((!adding).then_some(lane))),
    ]
    .align_y(Alignment::Center)
    .padding([tok::NONE, tok::XS]);
    let mut col = column![head].spacing(tok::XXS);
    let list = &l[lane];
    for (i, id) in list.iter().enumerate() {
        col = col.push(drop_line(s, at(i)));
        col = col.push(card(st, cx, lane, i, id));
    }
    col = col.push(drop_line(s, at(list.len())));
    // Room under the cards: dropped there, it goes to the lane's end.
    let end = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(tok::H_ROW),
    )
    .on_enter(Message::BarOver(lane, list.len()));
    container(col.push(end))
        .width(Length::Fill)
        .padding(tok::S)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            border: iced::Border::default().rounded(tok::R_CARD()),
            ..container::Style::default()
        })
        .into()
}

/// One module's card: grip, icon, name, ⚙ (when it has options), ✕. While a card is dragged,
/// each card's upper half places it before and its lower half after.
fn card<'a>(st: &'a State, cx: &Cx<'a>, lane: usize, i: usize, id: &str) -> Element<'a, Message> {
    let s = cx.scheme;
    let info = crate::modules::info(id);
    let name = info.map_or_else(|| format!("{id} (Lua)"), |m| m.name.to_string());
    let moving = st.bar_drag.as_deref() == Some(id);
    let picked = st.bar_pick.as_deref() == Some(id);
    let ink = if moving { s.ink_faint } else { s.ink };
    let grip = mouse_area(
        container(icon(Icon::Grip, tok::ICON_S, s.ink_faint)).padding([tok::XS, tok::XXS]),
    )
    .on_press(Message::BarDrag(id.to_string()))
    .interaction(iced::mouse::Interaction::Grab);
    let mut r = row![
        grip,
        icon(info.map_or(Icon::Code, |m| m.icon), tok::ICON_S, ink),
        label(
            ui::fit(&name, text::SMALL, LANE_W - CARD_CHROME),
            text::SMALL,
            ink
        )
        .width(Length::Fill),
    ]
    .spacing(tok::XS)
    .align_y(Alignment::Center);
    if info.is_some_and(|m| !m.options.is_empty()) {
        r = r.push(mini(
            s,
            Icon::Settings,
            Some(Message::BarPick((!picked).then(|| id.to_string()))),
        ));
    }
    r = r.push(mini(s, Icon::X, Some(Message::BarRemove(id.to_string()))));
    let body = container(r)
        .padding([tok::XXS, tok::XS])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                s.ink
                    .scale_alpha(if picked { tok::SELECTED } else { tok::FILL } * s.opacity)
                    .into(),
            ),
            border: iced::Border::default().rounded(tok::R_ROW()),
            ..container::Style::default()
        });
    if st.bar_drag.is_none() {
        return body.into();
    }
    let half = |to: usize| {
        mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_enter(Message::BarOver(lane, to))
    };
    iced::widget::stack![body, column![half(i), half(i + 1)]].into()
}

/// Room a card's grip, icon, buttons and gaps take beside its name.
const CARD_CHROME: f32 = 3.0 * tok::H_SMALL + 2.0 * tok::ICON_S + 4.0 * tok::XS;

/// The modules not in the bar, to add to `lane` (at its end; on the right, before Power).
fn missing<'a>(
    cx: &Cx<'a>,
    l: &[Vec<String>; 3],
    lane: usize,
) -> Vec<Option<Element<'a, Message>>> {
    let s = cx.scheme;
    crate::modules::MODULES
        .iter()
        .filter(|m| find_in(l, m.id).is_none())
        .map(|m| {
            Some(
                control(
                    row![
                        icon(m.icon, tok::ICON_M, s.ink),
                        column![
                            label(m.name, text::BODY, s.ink),
                            label(describe(m.id), text::SMALL, s.ink_dim),
                        ]
                        .spacing(tok::XXS)
                        .width(Length::Fill),
                        icon(Icon::Plus, tok::ICON, s.accent),
                    ]
                    .spacing(tok::L)
                    .align_y(Alignment::Center),
                )
                .padding([tok::M, tok::XL])
                .width(Length::Fill)
                .kind(s, crate::ui::control::Kind::Ghost)
                .no_press_scale()
                .on_press(Message::BarAddTo(lane, m.id.to_string()))
                .into(),
            )
        })
        .collect()
}

/// The picked module: what it is, where it sits (moved from here with the keyboard too), and
/// its options.
fn inspector<'a>(
    st: &'a State,
    cx: &Cx<'a>,
    l: &[Vec<String>; 3],
    id: &'a str,
    sec: usize,
) -> Option<Element<'a, Message>> {
    let s = cx.scheme;
    let info = crate::modules::info(id);
    let title: &'static str = info.map_or("Lua widget", |m| m.name);
    let at = l[sec].iter().position(|m| m == id).unwrap_or(0);
    let len = l[sec].len();
    let place = segments(
        s,
        &[
            (
                "Left",
                sec == 0,
                Message::BarSection(id.to_string(), -(sec as i32)),
            ),
            (
                "Centre",
                sec == 1,
                Message::BarSection(id.to_string(), 1 - sec as i32),
            ),
            (
                "Right",
                sec == 2,
                Message::BarSection(id.to_string(), 2 - sec as i32),
            ),
        ],
    );
    let order = row![
        mini(
            s,
            Icon::ChevronLeft,
            (at > 0).then(|| Message::BarMove(id.to_string(), -1))
        ),
        mini(
            s,
            Icon::ChevronRight,
            (at + 1 < len).then(|| Message::BarMove(id.to_string(), 1))
        ),
    ]
    .spacing(tok::XXS);
    let mut rows = vec![
        row_of(s, "What it shows", Some(describe(id)), Space::new().into()),
        row_of(s, "Side", None, place),
        row_of(s, "Order", None, order.into()),
    ];
    if let (Some(m), Some(mi)) = (
        info,
        crate::modules::MODULES.iter().position(|m| m.id == id),
    ) {
        rows.extend(module_options(st, cx, mi, m));
    }
    group(s, Some(title), rows)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(left: &[&str], center: &[&str], right: &[&str]) -> Layout {
        let v = |ids: &[&str]| ids.iter().map(|s| (*s).to_string()).collect();
        [v(left), v(center), v(right)]
    }

    fn sample() -> Layout {
        bar(
            &["workspaces", "app"],
            &["island"],
            &["network", "volume", "power"],
        )
    }

    #[test]
    fn a_module_moves_along_its_section() {
        let l = sample();
        assert_eq!(
            moved(&l, "volume", -1),
            Some(bar(
                &["workspaces", "app"],
                &["island"],
                &["volume", "network", "power"]
            ))
        );
        assert_eq!(
            moved(&l, "workspaces", 1).unwrap()[0],
            ["app", "workspaces"]
        );
    }

    #[test]
    fn a_module_at_its_sections_end_stays() {
        let l = sample();
        assert_eq!(moved(&l, "workspaces", -1), None);
        assert_eq!(moved(&l, "power", 1), None);
        assert_eq!(moved(&l, "island", 1), None, "alone in its section");
        assert_eq!(moved(&l, "volume", 5), None);
        assert_eq!(moved(&l, "volume", 0), None);
    }

    #[test]
    fn a_module_not_in_the_bar_moves_nowhere() {
        let l = sample();
        assert_eq!(moved(&l, "tray", 1), None);
        assert_eq!(to_section(&l, "tray", 1), None);
        assert_eq!(dropped(&l, "tray", 0, 0), None);
    }

    #[test]
    fn going_right_lands_at_the_sections_start() {
        let l = to_section(&sample(), "app", 1).unwrap();
        assert_eq!(
            l,
            bar(
                &["workspaces"],
                &["app", "island"],
                &["network", "volume", "power"]
            )
        );
        // Two sections over (the "Right" segment from the left).
        let l = to_section(&sample(), "app", 2).unwrap();
        assert_eq!(l[2], ["app", "network", "volume", "power"]);
    }

    #[test]
    fn going_left_lands_at_the_sections_end() {
        let l = to_section(&sample(), "network", -1).unwrap();
        assert_eq!(l[1], ["island", "network"]);
        assert_eq!(l[2], ["volume", "power"]);
        let l = to_section(&sample(), "network", -2).unwrap();
        assert_eq!(l[0], ["workspaces", "app", "network"]);
    }

    #[test]
    fn no_section_past_the_edges() {
        let l = sample();
        assert_eq!(to_section(&l, "app", -1), None);
        assert_eq!(to_section(&l, "power", 1), None);
        assert_eq!(to_section(&l, "island", 2), None);
    }

    #[test]
    fn the_last_module_of_a_section_can_leave_it() {
        let l = to_section(&sample(), "island", -1).unwrap();
        assert!(l[1].is_empty());
        assert_eq!(l[0], ["workspaces", "app", "island"]);
    }

    #[test]
    fn added_on_the_right_goes_before_power() {
        let l = added(&sample(), 2, "battery").unwrap();
        assert_eq!(l[2], ["network", "volume", "battery", "power"]);
        // Without a power button: at the end.
        let l = added(&bar(&[], &[], &["clock"]), 2, "battery").unwrap();
        assert_eq!(l[2], ["clock", "battery"]);
        // Elsewhere: at the end too.
        let l = added(&sample(), 0, "tray").unwrap();
        assert_eq!(l[0], ["workspaces", "app", "tray"]);
        let l = added(&bar(&[], &[], &[]), 1, "clock").unwrap();
        assert_eq!(l[1], ["clock"]);
    }

    #[test]
    fn a_module_is_never_added_twice_or_off_the_bar() {
        let l = sample();
        assert_eq!(added(&l, 0, "volume"), None, "already on the right");
        assert_eq!(added(&l, 3, "tray"), None);
    }

    #[test]
    fn dropped_where_it_was_saves_nothing() {
        let l = sample();
        // Before itself, or before the one after it: the same place.
        assert_eq!(dropped(&l, "volume", 2, 1), None);
        assert_eq!(dropped(&l, "volume", 2, 2), None);
    }

    #[test]
    fn dropped_further_along_its_own_section() {
        // Places count with the module still there: before "power" (2) is after "volume".
        let l = dropped(&sample(), "network", 2, 2).unwrap();
        assert_eq!(l[2], ["volume", "network", "power"]);
        // Past the end.
        let l = dropped(&sample(), "network", 2, 3).unwrap();
        assert_eq!(l[2], ["volume", "power", "network"]);
        // Earlier.
        let l = dropped(&sample(), "power", 2, 0).unwrap();
        assert_eq!(l[2], ["power", "network", "volume"]);
    }

    #[test]
    fn dropped_into_another_section() {
        let l = dropped(&sample(), "app", 2, 1).unwrap();
        assert_eq!(l[0], ["workspaces"]);
        assert_eq!(l[2], ["network", "app", "volume", "power"]);
        // A place past the end of a shorter section: at its end.
        let l = dropped(&sample(), "power", 1, 9).unwrap();
        assert_eq!(l[1], ["island", "power"]);
        // Into an empty section.
        let l = dropped(&bar(&["a"], &[], &["b"]), "a", 1, 0).unwrap();
        assert_eq!(l, bar(&[], &["a"], &["b"]));
    }

    #[test]
    fn a_drop_on_no_section_is_ignored() {
        assert_eq!(dropped(&sample(), "app", 3, 0), None);
    }

    #[test]
    fn removed_from_wherever_it_is() {
        let l = removed(&sample(), "volume");
        assert_eq!(l[2], ["network", "power"]);
        assert_eq!(removed(&sample(), "tray"), sample());
    }

    #[test]
    fn every_move_keeps_each_module_once() {
        // Any sequence of moves keeps the same modules, each once.
        let mut l = sample();
        let mut all: Vec<String> = l.concat();
        all.sort();
        for (id, d) in [("app", 1), ("power", -2), ("island", 1), ("app", -1)] {
            if let Some(n) = to_section(&l, id, d) {
                l = n;
            }
            if let Some(n) = moved(&l, id, 1) {
                l = n;
            }
            if let Some(n) = dropped(&l, id, 1, 0) {
                l = n;
            }
            let mut now: Vec<String> = l.concat();
            now.sort();
            assert_eq!(now, all);
        }
    }
}
