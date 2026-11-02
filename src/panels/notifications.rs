//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::action::{Action as Act, Note};
use super::cx::Cx;
use crate::services::icons;
use crate::services::notifications::{Image, Notification};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Element, Length};
use std::collections::BTreeSet;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub enum Message {
    Dismiss(u32),
    Action(u32, String),
    ClearAll,
    /// A group (by app) opened to show every message as its own card, or folded back.
    Open(String, bool),
    /// Every message of an app dismissed (an open group's Dismiss).
    DismissGroup(String),
    /// A message's cut text shown in full, or cut again (its arrow).
    More(u32, bool),
    /// A pop-up taken off the screen; it stays in the list (its ✕).
    Hide(u32),
    /// The pointer came onto a pop-up or left it: its ✕ shows and its time stops meanwhile.
    Hover(u32, bool),
}

/// The panel's own state: how the list is shown. The notifications themselves are the
/// shell's (the bar's banners, the history and Lua see them too); dismissing, clearing and an
/// app's buttons are asked of the shell (`Note`).
#[derive(Debug, Default)]
pub struct State {
    /// Apps whose group is open (each message its own card).
    open: BTreeSet<String>,
    /// Messages whose cut text is shown in full.
    more: BTreeSet<u32>,
}

impl State {
    /// Forgets what is about notifications no longer listed (called by the shell after every
    /// change to the list, however it changed).
    pub fn keep(&mut self, list: &[Notification]) {
        self.open.retain(|app| list.iter().any(|n| &n.app == app));
        self.more.retain(|id| list.iter().any(|n| n.id == *id));
    }
}

pub fn update(st: &mut State, m: Message) -> Act<Message> {
    match m {
        Message::Dismiss(id) => Act::Note(Note::Dismiss(id)),
        Message::Action(id, key) => Act::Note(Note::Invoke(id, key)),
        Message::Open(group, on) => {
            if on {
                st.open.insert(group);
            } else {
                st.open.remove(&group);
            }
            Act::None
        }
        Message::DismissGroup(group) => {
            st.open.remove(&group);
            Act::Note(Note::DismissApp(group))
        }
        Message::More(id, on) => {
            if on {
                st.more.insert(id);
            } else {
                st.more.remove(&id);
            }
            Act::None
        }
        Message::Hide(id) => Act::Note(Note::HideBanner(id)),
        Message::Hover(id, on) => Act::Note(Note::HoldBanner(id, on)),
        Message::ClearAll => Act::Note(Note::ClearAll),
    }
}

fn ago(t: SystemTime) -> String {
    let s = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{} min", s / 60),
        3600..=86_399 => format!("{} h", s / 3600),
        _ => format!("{} d", s / 86_400),
    }
}

/// The app's own icon, large enough to tell at a glance who it is from (its name is tried
/// when it sent no icon: "vesktop", "kitty").
fn app_icon<'a>(s: Scheme, n: &Notification) -> Element<'a, Message> {
    icon_sized(s, n, AVATAR)
}

/// A notification's app icon `size` px across.
fn icon_sized<'a>(s: Scheme, n: &Notification, size: f32) -> Element<'a, Message> {
    let k = size / AVATAR;
    // Kestrel's own entries: its own glyph on a soft round, like its other icons.
    if n.app == "kestrel" {
        // By what it is about: a screenshot, a recording, the battery; anything else (a config
        // error, a part that stopped) the plain info mark, not a camera.
        let glyph = match n.app_icon.as_str() {
            "camera" => Icon::Camera,
            "media-record" => Icon::Video,
            i if i.starts_with("battery") => Icon::Battery,
            _ => Icon::Info,
        };
        return container(icon(glyph, 17.0 * k, s.ink))
            .center(size)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(size / 2.0),
                ..container::Style::default()
            })
            .into();
    }
    let found = icons::lookup(&n.app_icon)
        .or_else(|| icons::lookup(&n.app.to_lowercase()))
        .or_else(|| icons::lookup(&n.app.to_lowercase().replace(' ', "-")))
        // By its desktop entry, as everywhere else ("Thunar" → org.xfce.thunar).
        .or_else(|| icons::lookup(&crate::services::apps::icon_for_class(&n.app)));
    match found {
        Some(p) => container(ui::image_file_faded(&p, size, s.opacity))
            .center(size)
            .into(),
        None => container(icon(Icon::Bell, 18.0 * k, s.ink))
            .center(size)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(size / 2.0),
                ..container::Style::default()
            })
            .into(),
    }
}

/// `text` cut to at most `lines` lines of `width` at `size` (wrapping by word), with an
/// ellipsis when cut.
fn lines_of(text: &str, size: f32, width: f32, lines: usize) -> String {
    let per = (width / (size * 0.55)).floor().max(8.0) as usize;
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut cut = false;
    for (i, w) in words.iter().enumerate() {
        let next = if cur.is_empty() {
            w.len()
        } else {
            cur.len() + 1 + w.len()
        };
        if next > per && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            if out.len() == lines {
                cut = i < words.len();
                break;
            }
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() && out.len() < lines {
        out.push(cur);
    }
    let mut s = out.join(" ");
    if cut || s.chars().count() > per * lines {
        s = ui::ellipsize(&s, per * lines);
        if !s.ends_with('…') {
            s.push('…');
        }
    }
    s
}

// --- The card: DMS's (Android 16's) notification card (design call 2026-11-16) -----------------
//
// Copied from DankMaterialShell (quickshell/Modules/Notifications/NotificationCard.qml,
// Center/NotificationCard.qml, Popup/NotificationPopup.qml, NotificationMetrics.qml):
// - Each message its own card: the app's icon (36 px) at the left; "App · time" small and
//   grey with the controls at its right; the title (one line); the text (two lines); a small
//   picture at the right; the app's buttons as plain text, then Dismiss.
// - Swiped sideways it goes (ui::swipe); a tap does the app's default action.
// - The list is one grouped list: 2 px apart, round only at its ends. An app's messages fold
//   into its newest card with a count chip; opened, a header (the app's name, Dismiss, the
//   chip) over each message's own card.
// - Pop-ups: the same card; ✕ and the arrow show on hover, and the time stops meanwhile.

/// The app icon (DMS avatarSize) and the space after it (DMS iconSpacing).
const AVATAR: f32 = 32.0;
/// A picture at the card's right (DMS thumbnailSize, buttonHeightM).
const THUMB: f32 = 56.0;
/// Lines of text a folded card shows (DMS collapsedLines).
const LINES: usize = 2;
/// Messages an open group shows (DMS expandedLimit).
const LIMIT: usize = 10;

/// The text column's width on a card `width` wide.
fn text_width(width: f32) -> f32 {
    width - 2.0 * (tok::M + tok::XXS) - AVATAR - tok::M
}

fn flat(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn title_of(n: &Notification) -> &str {
    let t = n.summary.trim();
    if t.is_empty() { &n.app } else { t }
}

fn app_name(n: &Notification) -> &str {
    if n.app.is_empty() {
        "Notification"
    } else {
        &n.app
    }
}

/// The notification's picture, a rounded square (DMS: at most THUMB, its own shape).
fn thumb<'a>(n: &Notification) -> Option<Element<'a, Message>> {
    match n.image.as_ref()? {
        Image::Path(p) => ui::rounded_file(std::path::Path::new(p), THUMB, THUMB, tok::R_CARD())
            .map(|h| image(h).width(THUMB).height(THUMB).into()),
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), THUMB, THUMB, tok::R_CARD())),
    }
}

/// The app's buttons the card shows ("default" is the card's own tap).
fn actions(n: &Notification) -> impl Iterator<Item = &(String, String)> {
    n.actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
}

/// What a tap on a card does: the app's default action (DMS: "default", else its first).
fn default_action(n: &Notification) -> Option<Message> {
    n.actions
        .iter()
        .find(|(k, _)| k == "default" || k.starts_with("kestrel-open:"))
        .or_else(|| n.actions.first())
        .map(|(k, _)| Message::Action(n.id, k.clone()))
}

/// How a card sits in its list and what it offers.
#[derive(Clone, Copy)]
struct Place {
    width: f32,
    /// A folded group's count (1 for a lone message) and whether it is open.
    count: usize,
    /// A pop-up: ✕ and the arrow only under the pointer; "Clear" instead of "Dismiss".
    popup: bool,
    more: bool,
}

/// One notification's card, as macOS draws it: the app's icon, the title with the time at its
/// right, the text under it (two lines; all of it when opened), a picture at the right; under
/// the pointer a ✕ in place of the time and the app's own buttons under the text. A tap does
/// what the app asks (a folded stack: opens it); swiped sideways it goes.
fn card_of<'a>(s: Scheme, n: &Notification, p: Place) -> Element<'a, Message> {
    let tw = text_width(p.width);
    let pic = thumb(n);
    // The text always the full width: a picture goes under it, so the time and ✕ stay in the
    // same place on every card (they moved left on cards with a picture).
    let body_w = tw;
    let body = flat(&n.body);
    let title = title_of(n).to_string();
    let cut_title = ui::fit(&title, text::BODY, body_w - 80.0);
    let cut_body = lines_of(&body, text::SMALL, body_w, LINES);
    let can_expand = p.count <= 1 && (cut_title != title || cut_body != body);

    // Title, the time, and ✕ always in the same top-right spot (nothing appears or moves under
    // the pointer: the card never changes shape).
    let close = if p.count > 1 {
        Message::DismissGroup(n.app.clone())
    } else if p.popup {
        Message::Hide(n.id)
    } else {
        Message::Dismiss(n.id)
    };
    let trail = row![
        label(ago(n.time), text::CAPTION, s.ink_faint),
        control(icon(Icon::X, tok::ICON_XS, s.ink_dim))
            .padding(tok::XS)
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Tonal)
            .on_press(close),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    let head = row![
        label_w(
            if p.more { title.clone() } else { cut_title },
            text::BODY,
            s.ink,
            theme::SEMIBOLD
        )
        .width(Length::Fill),
        trail,
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    let mut words = column![head].spacing(tok::XXS);
    if !body.is_empty() {
        words = words.push(
            iced::widget::text(if p.more { body } else { cut_body })
                .size(text::SMALL)
                .color(s.ink_dim)
                .font(theme::FONT)
                .line_height(iced::widget::text::LineHeight::Relative(1.25))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(Length::Fill),
        );
    }
    // A picture: a small square under the text.
    if let Some(pic) = pic {
        words = words.push(container(pic).padding(iced::Padding::ZERO.top(tok::XS)));
    }
    // The app's own buttons, always there (appearing under the pointer made the card jump).
    if p.count <= 1 {
        let acts: Vec<_> = actions(n).take(3).collect();
        if !acts.is_empty() {
            let room = (tw / acts.len() as f32 - 2.0 * tok::M).max(48.0);
            let mut r = row![].spacing(tok::XS);
            for (k, l) in acts {
                r = r.push(
                    control(label_w(
                        ui::fit(l, text::SMALL, room),
                        text::SMALL,
                        s.ink,
                        theme::MEDIUM,
                    ))
                    .padding([tok::XS, tok::M])
                    .radius(tok::FULL)
                    .kind(s, crate::ui::control::Kind::Tonal)
                    .on_press(Message::Action(n.id, k.clone())),
                );
            }
            words = words.push(container(r).padding(iced::Padding::ZERO.top(tok::XS)));
        }
    }
    let inner = row![app_icon(s, n), words.width(Length::Fill)]
        .spacing(tok::M)
        .align_y(Alignment::Start);
    let surface = control(inner)
        .identity(u64::from(n.id))
        .padding(tok::M + tok::XXS)
        .width(Length::Fill)
        .radius(tok::R_CARD())
        .kind(
            s,
            if p.popup {
                crate::ui::control::Kind::Ghost
            } else {
                crate::ui::control::Kind::Tonal
            },
        )
        .no_press_scale()
        .on_enter(Message::Hover(n.id, true))
        .on_exit(Message::Hover(n.id, false));
    let tap = if p.count > 1 {
        Some(Message::Open(n.app.clone(), true))
    } else {
        default_action(n).or_else(|| can_expand.then_some(Message::More(n.id, !p.more)))
    };
    let card: Element<'a, Message> = ui::swipe(surface)
        .identity(u64::from(n.id))
        .on_tap_maybe(tap)
        .on_dismiss(if p.count > 1 {
            Message::DismissGroup(n.app.clone())
        } else {
            Message::Dismiss(n.id)
        })
        .into();
    if p.count <= 1 {
        return card;
    }
    // A folded stack (macOS): the newest card, and the next ones peeking under it as thinner,
    // fainter edges (one for two, two for three or more).
    let layer = |inset: f32, alpha: f32| -> Element<'a, Message> {
        container(
            container(Space::new())
                .width(Length::Fill)
                .height(tok::S)
                .style(move |_| container::Style {
                    background: Some(s.ink.scale_alpha(tok::TONAL * alpha * s.opacity).into()),
                    border: iced::Border {
                        radius: iced::border::Radius {
                            top_left: tok::NONE,
                            top_right: tok::NONE,
                            bottom_right: tok::R_CARD(),
                            bottom_left: tok::R_CARD(),
                        },
                        ..iced::Border::default()
                    },
                    ..container::Style::default()
                }),
        )
        .padding([0.0, inset])
        .into()
    };
    let mut stack = column![card];
    stack = stack.push(layer(tok::M, 0.8));
    if p.count > 2 {
        stack = stack.push(layer(2.0 * tok::M, 0.5));
    }
    stack.into()
}

/// A pop-up (the same card, ✕ and the arrow on hover).
pub fn popup<'a>(st: &State, s: Scheme, n: &Notification, width: f32) -> Element<'a, Message> {
    card_of(
        s,
        n,
        Place {
            width,
            count: 1,
            popup: true,
            more: st.more.contains(&n.id),
        },
    )
}

/// The notifications panel at the island: the list, or a quiet bell when there is none.
pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    if cx.notifications.is_empty() {
        // Compact: one quiet line (a big box with a lone bell looked like a glitch).
        return container(
            row![
                icon(Icon::Bell, tok::ICON_S, s.ink_faint),
                label("No notifications", text::SMALL, s.ink_dim)
            ]
            .spacing(tok::S)
            .align_y(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding(tok::L)
        .into();
    }
    let room = cx
        .screen
        .map_or(f32::INFINITY, |o| o.height - tok::BAR - 2.0 * tok::XL)
        // A compact panel: about five cards, then it scrolls (it ran down the whole screen).
        .min(LIST_MAX);
    container(section(st, cx, room))
        .padding(tok::PANEL_PAD)
        .into()
}

/// How tall the fade at a long list's bottom is.
const FADE: f32 = 36.0;

/// The notifications list's most height before it scrolls (about five cards).
const LIST_MAX: f32 = 380.0;

/// The list's width (the panel less its padding).
const LIST_W: f32 = tok::PANEL_M - 2.0 * tok::PANEL_PAD;

/// Notifications inside the control centre: one grouped list, an app's messages folded into
/// its newest card (DMS); it scrolls in `room` (the tiles above stay still). Nothing at all
/// when there are none.
fn section<'a>(st: &'a State, cx: &Cx<'a>, room: f32) -> Element<'a, Message> {
    let s = cx.scheme;
    if cx.notifications.is_empty() {
        return Space::new().into();
    }
    let header = row![
        label_w("Notifications", text::BODY, s.ink, theme::SEMIBOLD).width(Length::Fill),
        control(label("Clear all", text::SMALL, s.ink_dim))
            .pill(s, crate::ui::control::Pill::Quiet)
            .on_press(Message::ClearAll),
    ]
    .align_y(Alignment::Center)
    .padding([tok::NONE, tok::XS]);
    // One group per app, the newest first (DMS, ii).
    let mut groups: Vec<Vec<&Notification>> = Vec::new();
    for n in cx.notifications.iter().rev() {
        match groups.iter_mut().find(|g| g[0].app == n.app) {
            Some(g) => g.push(n),
            None => groups.push(vec![n]),
        }
    }
    let mut list = column![].spacing(tok::S);
    for g in &groups {
        let n = g[0];
        let open = g.len() > 1 && st.open.contains(&n.app);
        if !open {
            list = list.push(card_of(
                s,
                n,
                Place {
                    width: LIST_W,
                    count: g.len(),
                    popup: false,
                    more: st.more.contains(&n.id),
                },
            ));
            continue;
        }
        // Open (macOS): the app's name, Show less, ✕ for all of them, then each card.
        let head = row![
            label_w(
                ui::fit(app_name(n), text::TITLE, LIST_W / 2.0),
                text::TITLE,
                s.ink,
                theme::SEMIBOLD
            )
            .width(Length::Fill),
            control(label("Show less", text::SMALL, s.ink_dim))
                .pill(s, crate::ui::control::Pill::Quiet)
                .on_press(Message::Open(n.app.clone(), false)),
            control(icon(Icon::X, tok::ICON_XS, s.ink_dim))
                .padding(tok::XS + tok::XXS)
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Tonal)
                .on_press(Message::DismissGroup(n.app.clone())),
        ]
        .spacing(tok::XS)
        .align_y(Alignment::Center)
        .padding([tok::NONE, tok::XS]);
        let mut group = column![head].spacing(tok::S);
        for m in g.iter().take(LIMIT) {
            group = group.push(card_of(
                s,
                m,
                Place {
                    width: LIST_W,
                    count: 1,
                    popup: false,
                    more: st.more.contains(&m.id),
                },
            ));
        }
        list = list.push(group);
    }
    let list = container(crate::ui::smooth(
        iced::widget::scrollable(list)
            .height(Length::Shrink)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(0)
                    .scroller_width(0),
            )),
    ))
    .max_height(room);
    // More than fits: the list's bottom fades into the panel, so a card cut by the edge reads
    // as "more below", not as broken.
    let cards: usize = groups
        .iter()
        .map(|g| {
            if st.open.contains(&g[0].app) {
                g.len().min(LIMIT)
            } else {
                1
            }
        })
        .sum();
    // Five fit in LIST_MAX; only a sixth runs past its edge.
    let list: Element<'_, Message> = if cards > 5 && room <= LIST_MAX {
        let tint = if cx.config.material == crate::config::Material::Frosted {
            s.base.scale_alpha(0.42 * s.opacity)
        } else {
            s.base
        };
        let fade = container(Space::new())
            .width(Length::Fill)
            .height(FADE)
            .style(move |_| container::Style {
                background: Some(
                    iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                        .add_stop(0.0, iced::Color { a: 0.0, ..tint })
                        .add_stop(1.0, tint)
                        .into(),
                ),
                ..container::Style::default()
            });
        // Placed by its own top padding, not a Fill height: inside a panel that measures its
        // content, Fill made the list infinitely tall and the panel never showed (the user's clicks
        // on the count did nothing).
        iced::widget::stack![
            list,
            container(fade).padding(iced::Padding::ZERO.top((room - FADE).max(0.0)))
        ]
        .into()
    } else {
        list.into()
    };
    column![header, list].spacing(tok::M).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u32, app: &str) -> Notification {
        Notification {
            id,
            app: app.into(),
            app_icon: String::new(),
            summary: String::new(),
            body: String::new(),
            actions: Vec::new(),
            image: None,
            urgency: 1,
            timeout: -1,
            time: SystemTime::now(),
            sound: Default::default(),
            replaced: false,
        }
    }

    /// Open groups and expanded texts go with their notifications, however those went.
    #[test]
    fn keeps_only_what_is_listed() {
        let mut st = State::default();
        let _ = update(&mut st, Message::Open("Mail".into(), true));
        let _ = update(&mut st, Message::Open("Signal".into(), true));
        let _ = update(&mut st, Message::More(1, true));
        let _ = update(&mut st, Message::More(2, true));
        st.keep(&[note(2, "Signal")]);
        assert_eq!(st.open.iter().collect::<Vec<_>>(), ["Signal"]);
        assert_eq!(st.more.iter().collect::<Vec<_>>(), [&2]);
    }
}
