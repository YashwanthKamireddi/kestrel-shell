//! The window, the sidebar and the parts every page is built from (rows, groups,
//! fields, pick lists, Hyprland options).

use super::*;
use iced::widget::{column, row};

thread_local! {
    /// The Settings search, lowercased, while a view is being built (empty: everything).
    static QUERY: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Whether `name` is shown for the search being typed (always when not searching).
pub(super) fn matches(name: &str) -> bool {
    QUERY.with(|q| {
        let q = q.borrow();
        q.is_empty() || name.to_lowercase().contains(q.as_str())
    })
}

/// A setting: its name (and a line under it) on the left, its control on the right.
pub(super) fn row_of<'a>(
    s: Scheme,
    name: &'a str,
    note: Option<&'a str>,
    control_: Element<'a, Message>,
) -> Option<Element<'a, Message>> {
    // While searching, only the settings whose name matches.
    if !QUERY.with(|q| {
        let q = q.borrow();
        q.is_empty() || name.to_lowercase().contains(q.as_str())
    }) {
        return None;
    }
    let mut words = column![label(name, text::BODY, s.ink)].spacing(tok::XXS);
    if let Some(n) = note {
        words = words.push(label(n, text::CAPTION, s.ink_dim));
    }
    Some(
        container(
            row![container(words).width(Length::Fill), control_]
                .spacing(tok::L)
                .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::XL])
        .height(if note.is_some() { 56 } else { 46 })
        .align_y(Alignment::Center)
        .into(),
    )
}

/// Rows grouped on one rounded card, a hairline between them, a heading above if given.
pub(super) fn group<'a>(
    s: Scheme,
    title: Option<&'a str>,
    rows: Vec<Option<Element<'a, Message>>>,
) -> Option<Element<'a, Message>> {
    let rows: Vec<Element<'a, Message>> = rows.into_iter().flatten().collect();
    if rows.is_empty() {
        return None;
    }
    let mut card = column![].spacing(tok::NONE);
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        card = card.push(r);
        if i + 1 < n {
            card = card.push(
                container(
                    container(iced::widget::Space::new())
                        .width(Length::Fill)
                        .height(1)
                        .style(move |_| container::Style {
                            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
                            ..container::Style::default()
                        }),
                )
                .padding([tok::NONE, tok::XL]),
            );
        }
    }
    let card = container(card)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            border: iced::Border::default().rounded(tok::R_CARD()),
            ..container::Style::default()
        });
    Some(match title {
        Some(t) => column![crate::ui::section(s, t, tok::XS), card]
            .spacing(tok::M)
            .into(),
        None => card.into(),
    })
}

/// Two or more choices, one of them chosen.
pub(super) fn segments<'a>(
    s: Scheme,
    options: &[(&'a str, bool, Message)],
) -> Element<'a, Message> {
    let mut r = row![].spacing(tok::XXS);
    for (name, on, m) in options {
        r = r.push(
            control(label(
                *name,
                text::SMALL,
                if *on { s.on_accent } else { s.ink },
            ))
            .padding([tok::S, tok::L])
            .radius(tok::FULL)
            .kind(
                s,
                if *on {
                    crate::ui::control::Kind::Primary
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .on_press(m.clone()),
        );
    }
    container(r)
        .padding(tok::XXS)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        })
        .into()
}

/// Settings > Sound is showing (its live levels run then).
pub fn on_sound(st: &State) -> bool {
    st.page == Page::Sound && st.query.is_empty()
}

/// Settings as its own window: the same view on the shell's solid surface colour.
pub fn window<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme.faded(1.0);
    let body = container(view(st, &cx.with_scheme(s)))
        .id(ROOT.clone())
        .width(Length::Fill)
        .height(Length::Fill);
    // The shell's look: Black is solid; Frosted is the panels' own glass (the compositor's
    // blur of what is behind, under the same dark tint).
    let look: Element<'a, Message> = if crate::theme::glass() {
        iced::blur_container(body)
            .style(move |_| container::Style {
                background: Some(s.base.scale_alpha(0.42).into()),
                ..container::Style::default()
            })
            .into()
    } else {
        body.style(move |_| container::Style {
            background: Some(s.base.into()),
            ..container::Style::default()
        })
        .into()
    };
    // Its first frame says so: the search takes the focus then (iced's own sensor, at the
    // root so it fires once per window).
    iced::widget::sensor(look)
        .on_show(|_| Message::Shown)
        .into()
}

fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let page = st.page;
    let shell_pages: Vec<Page> = SHELL
        .into_iter()
        .filter(|p| *p != Page::Plugins || !st.plugins.is_empty())
        .collect();
    // Sidebar: the pages, the open one on a soft neutral fill.
    let search = container(
        row![
            icon(Icon::Search, tok::ICON, s.ink_dim),
            text_input("Search", &st.query)
                .id(SEARCH_ID.clone())
                .on_input(Message::Query)
                .padding([tok::S, tok::XS])
                .size(text::BODY)
                .font(theme::FONT)
                .style(crate::ui::input_style(s))
        ]
        .spacing(tok::XS)
        .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::L])
    .height(34)
    .center_y(34)
    .style(move |_| {
        let f = ui::field_style(s);
        container::Style {
            border: iced::Border {
                radius: 17.0.into(),
                ..f.border
            },
            ..f
        }
    });
    let mut side = column![
        container(label_w("Settings", text::HEADING, s.ink, theme::SEMIBOLD))
            .padding([tok::S, tok::L]),
        container(search).padding(iced::Padding {
            bottom: tok::M,
            ..iced::Padding::ZERO
        }),
    ]
    .spacing(tok::XXS);
    let heading = |t: &'static str| {
        container(label_w(t, text::CAPTION, s.ink_faint, theme::SEMIBOLD)).padding(iced::Padding {
            top: tok::L,
            right: tok::M,
            bottom: tok::XS,
            left: tok::M,
        })
    };
    let mut entries: Vec<Option<Page>> = shell_pages.into_iter().map(Some).collect();
    entries.push(None);
    entries.extend(SYSTEM.into_iter().map(Some));
    for p in entries {
        let Some(p) = p else {
            side = side.push(heading("System"));
            continue;
        };
        let on = p == page;
        let fg = if on { s.ink } else { s.ink_dim };
        side = side.push(
            control(
                row![
                    icon(p.icon(), tok::ICON, fg),
                    label_w(
                        p.name(),
                        text::BODY,
                        fg,
                        if on { theme::SEMIBOLD } else { theme::MEDIUM }
                    )
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::L])
            .height(36)
            .width(Length::Fill)
            .radius(tok::R_ROW())
            .kind(
                s,
                if on {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .no_press_scale()
            .on_press(Message::Page(p)),
        );
    }
    let side = container(
        iced::widget::scrollable(side).direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0),
        )),
    )
    .width(SIDEBAR)
    .height(Length::Fill)
    .padding([tok::L, tok::L]);

    // Searching: every page's matching settings at once.
    let q = st.query.trim().to_lowercase();
    QUERY.with(|cell| *cell.borrow_mut() = q.clone());
    let body = if q.is_empty() {
        column![label_w(page.name(), text::HEADING, s.ink, theme::SEMIBOLD)]
            .spacing(tok::XL)
            .extend(match page {
                Page::Bar => bar_page(st, cx),
                Page::Appearance => appearance_page(st, cx),
                Page::Time => time_page(st, cx),
                Page::Plugins => plugins_page(st, cx),
                Page::Keyboard => keyboard_page(st, cx),
                Page::Mouse => mouse_page(st, cx),
                Page::Defaults => defaults_page(st, cx),
                Page::Power => power_page(st, cx),
                Page::Access => access_page(st, cx),
                Page::Shortcuts => shortcuts_page(st, cx),
                Page::Displays => displays_page(st, cx),
                Page::Windows => windows_page(st, cx),
                Page::Notifications => notifications_page(st, cx),
                Page::Privacy => privacy_page(st, cx),
                Page::Startup => startup_page(st, cx),
                Page::Updates => updates_page(st, cx),
                Page::Storage => storage_page(st, cx),
                Page::Network => network_page(st, cx),
                Page::Bluetooth => bluetooth_page(st, cx),
                Page::Account => account_page(st, cx),
                Page::Sound => sound_page(st, cx),
                Page::DateTime => datetime_page(st, cx),
                Page::Developer => developer_page(st, cx),
                Page::About => about_page(st, cx),
            })
    } else {
        // Pages first: by name or by what they hold ("wifi" → Network, "blue" → Bluetooth), the
        // same words the launcher finds them by; a click opens the page.
        let pages: Vec<Page> = Page::all()
            .filter(|p| {
                p.name().to_lowercase().contains(&q)
                    || p.words().split_whitespace().any(|w| w.starts_with(&q))
            })
            .collect();
        let page_rows = (!pages.is_empty()).then(|| {
            let mut c = column![].spacing(tok::XXS);
            for p in pages {
                c = c.push(
                    control(
                        row![
                            icon(p.icon(), tok::ICON, s.ink_dim),
                            label_w(p.name(), text::BODY, s.ink, theme::MEDIUM).width(Length::Fill),
                            icon(Icon::ChevronRight, tok::ICON_S, s.ink_faint),
                        ]
                        .spacing(tok::L)
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::L])
                    .height(40)
                    .width(Length::Fill)
                    .radius(tok::R_ROW())
                    .kind(s, crate::ui::control::Kind::Tonal)
                    .no_press_scale()
                    .on_press(Message::Page(p)),
                );
            }
            Element::from(c)
        });
        column![label_w("Search", text::HEADING, s.ink, theme::SEMIBOLD)]
            .spacing(tok::XL)
            .extend(page_rows)
            .extend(bar_page(st, cx))
            .extend(appearance_page(st, cx))
            .extend(time_page(st, cx))
            .extend(plugins_page(st, cx))
            // The system pages that have been read (searching reads the quick ones).
            .extend(power_page(st, cx))
            .extend(access_page(st, cx))
            .extend(keyboard_page(st, cx))
            .extend(mouse_page(st, cx))
            .extend(windows_page(st, cx))
            .extend(datetime_page(st, cx))
            .extend(notifications_page(st, cx))
    };
    QUERY.with(|cell| cell.borrow_mut().clear());
    // A comfortable reading width however wide the window (rows and sliders stretched over a
    // whole screen read badly).
    let body = iced::widget::scrollable(
        // The limit on the padded box itself: on the column alone the box still stretched to the
        // window and the content hugged the left of a wide window.
        container(
            container(body)
                .padding([tok::XXL, tok::XXL])
                .max_width(READING_W + 2.0 * tok::XXL),
        )
        .center_x(Length::Fill),
    )
    .id(BODY_ID.clone())
    .height(Length::Fill)
    .width(Length::Fill)
    .direction(iced::widget::scrollable::Direction::Vertical(
        iced::widget::scrollable::Scrollbar::new()
            .width(0)
            .scroller_width(0),
    ));
    let body = crate::ui::smooth(body);
    let divider = container(iced::widget::Space::new())
        .width(1)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            ..container::Style::default()
        });
    row![side, divider, body].height(Length::Fill).into()
}

pub(super) fn stepper<'a>(s: Scheme, n: i64) -> Element<'a, Message> {
    let arrow = |i: Icon, m: Message| {
        control(container(icon(i, tok::ICON, s.ink)).center(28))
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(m)
    };
    row![
        arrow(Icon::ChevronLeft, Message::Workspaces(n - 1)),
        container(label_w(n.to_string(), text::BODY, s.ink, theme::SEMIBOLD)).center_x(28),
        arrow(Icon::ChevronRight, Message::Workspaces(n + 1)),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A setting that only shows a value (About): its name and the value, right-aligned.
pub(super) fn value_row<'a>(
    s: Scheme,
    name: &'a str,
    value: String,
) -> Option<Element<'a, Message>> {
    row_of(s, name, None, label(value, text::BODY, s.ink_dim).into())
}

/// This computer's name (a hotspot is named after it at first).
pub(super) fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_string())
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "kestrel".into())
}

/// A hotspot password: 12 letters and digits, none that read alike (0/O, 1/l/I).
pub(super) fn new_password() -> String {
    const SET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 12];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes))
        .is_err()
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (t >> (i * 5)) as u8;
        }
    }
    bytes
        .iter()
        .map(|b| SET[*b as usize % SET.len()] as char)
        .collect()
}

/// A text field in a row: the same quiet capsule as every other field.
pub(super) fn field<'a>(
    s: Scheme,
    id: &'static str,
    placeholder: &'a str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    secure: bool,
) -> Element<'a, Message> {
    let mut t = text_input(placeholder, value)
        .id(iced::widget::Id::new(id))
        .on_input(on_input)
        .secure(secure)
        .padding([tok::S, tok::L])
        .size(text::BODY)
        .font(theme::FONT)
        .width(240)
        .style(crate::ui::input_style(s));
    if let Some(m) = on_submit {
        t = t.on_submit(m);
    }
    container(t).style(move |_| ui::field_style(s)).into()
}

/// A value that copies itself when clicked (addresses, passwords).
pub(super) fn copy_row<'a>(
    s: Scheme,
    name: &'a str,
    value: String,
) -> Option<Element<'a, Message>> {
    row_of(
        s,
        name,
        None,
        control(label(value.clone(), text::BODY, s.ink_dim))
            .padding([tok::XS, tok::L])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .no_press_scale()
            .on_press(Message::Copy(value))
            .into(),
    )
}

/// A connection's addresses (Wi-Fi or Ethernet), each copied by a click.
pub(super) fn detail_rows<'a>(
    s: Scheme,
    d: &crate::services::network::Details,
) -> Vec<Option<Element<'a, Message>>> {
    let mut rows = Vec::new();
    if let Some(ip) = &d.ipv4 {
        rows.push(copy_row(s, "IP address", ip.clone()));
    }
    if let Some(ip) = &d.ipv6 {
        rows.push(copy_row(s, "IPv6 address", ip.clone()));
    }
    if let Some(g) = &d.gateway {
        rows.push(copy_row(s, "Router", g.clone()));
    }
    if !d.dns.is_empty() {
        rows.push(copy_row(s, "DNS", d.dns.join(", ")));
    }
    rows
}

/// A search field for a pick list (time zones, layouts).
pub(super) fn pick_search<'a>(
    s: Scheme,
    id: &'static str,
    value: &'a str,
    on: fn(String) -> Message,
) -> Element<'a, Message> {
    container(
        text_input("Search", value)
            .id(iced::widget::Id::new(id))
            .on_input(on)
            .padding([tok::S, tok::L])
            .size(text::BODY)
            .font(theme::FONT)
            .style(crate::ui::input_style(s)),
    )
    .style(move |_| ui::field_style(s))
    .into()
}

/// One entry of a pick list.
pub(super) fn pick_row<'a>(s: Scheme, name: String, on: bool, m: Message) -> Element<'a, Message> {
    control(label(name, text::BODY, if on { s.accent } else { s.ink }))
        .padding([tok::NONE, tok::L])
        .height(34)
        .width(Length::Fill)
        .radius(tok::R_ROW())
        .kind(s, crate::ui::control::Kind::Ghost)
        .no_press_scale()
        .on_press(m)
        .into()
}

/// A row for one Hyprland input option: a switch, a choice, or a slider with its value.
pub(super) fn hypr_row<'a>(
    st: &'a State,
    cx: &Cx<'a>,
    o: crate::hypr_settings::Opt,
) -> Option<Element<'a, Message>> {
    let s = cx.scheme;
    use crate::hypr_settings::{Kind, Value};
    let v = st.hypr.get(o.option)?;
    let control_: Element<'_, Message> = match o.kind {
        Kind::Switch => {
            let on = v.as_bool();
            ui::switch(s, on, Message::HyprSet(o, Value::Bool(!on), true))
        }
        Kind::Choice(choices) => {
            let now = v.as_str();
            let opts: Vec<(&str, bool, Message)> = choices
                .iter()
                .map(|(val, name)| {
                    (
                        *name,
                        *val == now,
                        Message::HyprSet(o, Value::Str((*val).to_string()), true),
                    )
                })
                .collect();
            segments(s, &opts)
        }
        Kind::Numbers(choices) => {
            let now = v.as_num().round() as i64;
            let opts: Vec<(&str, bool, Message)> = choices
                .iter()
                .map(|(val, name)| {
                    (
                        *name,
                        *val == now,
                        Message::HyprSet(o, Value::Num(*val as f32), true),
                    )
                })
                .collect();
            segments(s, &opts)
        }
        Kind::Range {
            min,
            max,
            step,
            unit,
        } => {
            let n = v.as_num();
            let shown = if step >= 1.0 {
                format!("{n:.0}{unit}")
            } else if unit.is_empty() {
                format!("{n:+.2}")
            } else {
                format!("{n:.2}{unit}")
            };
            let bar = crate::ui::slider::slider(
                iced::widget::Space::new().width(Length::Fill),
                ((n - min) / (max - min)).clamp(0.0, 1.0),
                22.0,
                (s.ink.scale_alpha(tok::FILL * s.opacity), s.accent),
                move |t| {
                    let raw = min + t * (max - min);
                    let v = ((raw / step).round() * step).clamp(min, max);
                    Message::HyprSet(o, Value::Num(v), false)
                },
            )
            .on_release(Message::HyprKeep(o));
            row![
                container(bar).width(200),
                container(label(shown, text::SMALL, s.ink_dim))
                    .width(56)
                    .align_right(56),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center)
            .into()
        }
    };
    row_of(s, o.label, None, control_)
}

/// Seconds left before display changes go back (for the countdown).
pub fn display_countdown(st: &State) -> Option<u64> {
    st.pending
        .map(|(_, at)| KEEP_SECS.saturating_sub(at.elapsed().as_secs()))
}

/// "60 Hz" for a rate (a segment borrows its label).
pub(super) fn hz_name(r: f32) -> &'static str {
    kept(format!("{} Hz", r.round() as u32))
}

/// A label made at run time, kept for the life of the shell so a segment can borrow it. Each
/// distinct text is made once (rates, percents: a handful), so this never grows with redraws.
pub(super) fn kept(text: String) -> &'static str {
    static TEXTS: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut texts = TEXTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(t) = texts.iter().find(|t| **t == text) {
        return t;
    }
    let t: &'static str = Box::leak(text.into_boxed_str());
    texts.push(t);
    t
}

/// Whether Settings is recording a key combination (its keys go to the recorder).
pub fn recording(st: &State) -> bool {
    st.recording && st.page == Page::Shortcuts
}

/// A key press as Hyprland writes it ("SUPER + SHIFT + T"); None for modifiers alone and
/// keys a shortcut can't use.
pub fn combo(key: &iced::keyboard::Key, m: iced::keyboard::Modifiers) -> Option<String> {
    use iced::keyboard::Key;
    use iced::keyboard::key::Named;
    let name = match key {
        Key::Character(c) => {
            let c = c.to_uppercase();
            match c.as_str() {
                "/" => "Slash".into(),
                "." => "Period".into(),
                "," => "Comma".into(),
                ";" => "Semicolon".into(),
                "'" => "Apostrophe".into(),
                "[" => "BracketLeft".into(),
                "]" => "BracketRight".into(),
                "-" => "Minus".into(),
                "=" => "Equal".into(),
                "`" => "Grave".into(),
                "\\" => "Backslash".into(),
                c => c.to_string(),
            }
        }
        Key::Named(n) => match n {
            Named::Enter => "Return".into(),
            Named::Space => "Space".into(),
            Named::Tab => "Tab".into(),
            Named::Backspace => "BackSpace".into(),
            Named::Delete => "Delete".into(),
            Named::Home => "Home".into(),
            Named::End => "End".into(),
            Named::PageUp => "Prior".into(),
            Named::PageDown => "Next".into(),
            Named::ArrowLeft => "Left".into(),
            Named::ArrowRight => "Right".into(),
            Named::ArrowUp => "Up".into(),
            Named::ArrowDown => "Down".into(),
            Named::Insert => "Insert".into(),
            Named::PrintScreen => "Print".into(),
            n => {
                let d = format!("{n:?}");
                // F1…F24.
                if d.starts_with('F') && d[1..].parse::<u8>().is_ok() {
                    d
                } else {
                    return None;
                }
            }
        },
        _ => return None,
    };
    let mut parts: Vec<&str> = Vec::new();
    if m.logo() {
        parts.push("SUPER");
    }
    if m.control() {
        parts.push("CTRL");
    }
    if m.alt() {
        parts.push("ALT");
    }
    if m.shift() {
        parts.push("SHIFT");
    }
    // A shortcut needs a modifier (a plain letter would stop typing it), F-keys aside.
    if parts.is_empty() && !name.starts_with('F') {
        return None;
    }
    parts.push(&name);
    Some(parts.join(" + "))
}
