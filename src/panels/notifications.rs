//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Element, Length, Task};
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

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Notifications(m))
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let task = update_inner(app, m);
    // The last one gone while the list is open: the list closes at once (an empty list says
    // nothing).
    if app.notifications.is_empty()
        && app
            .panel
            .as_ref()
            .is_some_and(|p| p.open && p.kind == super::PanelKind::Notifications)
    {
        // Closed in this same update (closing a frame later drew it empty first: a flicker).
        return Task::batch([task, app.close()]);
    }
    task
}

fn update_inner(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Dismiss(id) => {
            app.notifications.retain(|n| n.id != id);
            app.toasts.retain(|t| t.id != id);
            Task::future(notifications::closed(id, Reason::Dismissed)).discard()
        }
        // Kestrel's own entries: open a file or folder, or save a copied picture.
        Message::Action(id, key) if key.starts_with("kestrel-") => {
            app.toasts.retain(|t| t.id != id);
            if let Some(path) = key.strip_prefix("kestrel-open:") {
                let open = crate::services::compositor::Action::Open(path.into());
                return Task::done(AppMessage::Dispatch(open));
            } else if let Some(path) = key.strip_prefix("kestrel-copy:") {
                copy_file(path);
            } else if let Some(path) = key.strip_prefix("kestrel-save:")
                && let Some(saved) = save_picture(path)
            {
                {
                    // The entry now points at the saved file.
                    if let Some(n) = app.notifications.iter_mut().find(|n| n.id == id) {
                        n.body = format!(
                            "Saved in {}",
                            saved
                                .parent()
                                .map_or(String::new(), |p| p.display().to_string())
                        );
                        n.actions =
                            vec![(format!("kestrel-open:{}", saved.display()), "Open".into())];
                    }
                }
            }
            Task::none()
        }
        Message::Action(id, key) => {
            app.notifications.retain(|n| n.id != id);
            app.toasts.retain(|t| t.id != id);
            Task::future(notifications::invoke(id, key)).discard()
        }
        Message::Open(group, on) => {
            if on {
                app.notif_open.insert(group);
            } else {
                app.notif_open.remove(&group);
            }
            Task::none()
        }
        Message::DismissGroup(group) => {
            let ids: Vec<u32> = app
                .notifications
                .iter()
                .filter(|n| n.app == group)
                .map(|n| n.id)
                .collect();
            app.notifications.retain(|n| n.app != group);
            app.toasts.retain(|t| !ids.contains(&t.id));
            app.notif_open.remove(&group);
            Task::batch(
                ids.into_iter()
                    .map(|id| Task::future(notifications::closed(id, Reason::Dismissed)).discard()),
            )
        }
        Message::More(id, on) => {
            if on {
                app.notif_more.insert(id);
            } else {
                app.notif_more.remove(&id);
            }
            Task::none()
        }
        Message::Hide(id) => {
            app.toasts.retain(|t| t.id != id);
            Task::none()
        }
        Message::Hover(id, on) => {
            if on {
                app.toast_hover = Some(id);
            } else if app.toast_hover == Some(id) {
                app.toast_hover = None;
                // Its time starts again in full once the pointer leaves (DMS restarts it).
                if let Some(t) = app.toasts.iter_mut().find(|t| t.id == id) {
                    t.until = std::time::Instant::now() + t.secs;
                }
            }
            Task::none()
        }
        Message::ClearAll => {
            let ids: Vec<u32> = app.notifications.iter().map(|n| n.id).collect();
            app.notifications.clear();
            app.toasts.clear();
            Task::batch(
                ids.into_iter()
                    .map(|id| Task::future(notifications::closed(id, Reason::Dismissed)).discard()),
            )
        }
    }
}

/// Puts a file on the clipboard: a picture as the picture itself (pastes into any chat or
/// editor), anything else (a recording) as the file (pastes into a chat, a folder, an upload).
fn copy_file(path: &str) {
    let p = std::path::Path::new(path);
    let picture = matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    );
    let (mime, data) = if picture {
        let mime = match p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        };
        match std::fs::read(p) {
            Ok(d) => (mime, d),
            Err(_) => return,
        }
    } else {
        (
            "text/uri-list",
            format!("{}\r\n", crate::services::filechooser::uri(p)).into_bytes(),
        )
    };
    crate::services::clipboard::copy(&[mime], data);
}

/// Copies a kept picture into Pictures/Screenshots under a dated name.
fn save_picture(from: &str) -> Option<std::path::PathBuf> {
    let dir = crate::services::xdg::user_dir("PICTURES").join("Screenshots");
    std::fs::create_dir_all(&dir).ok()?;
    let ext = std::path::Path::new(from)
        .extension()
        .map_or("png".into(), |e| e.to_string_lossy().into_owned());
    let to = dir.join(format!(
        "Screenshot_{}.{ext}",
        chrono::Local::now().format("%Y-%m-%d_%H.%M.%S")
    ));
    std::fs::copy(from, &to).ok()?;
    Some(to)
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
fn app_icon<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
    icon_sized(s, n, AVATAR)
}

/// A notification's app icon `size` px across (the island shows them small).
pub fn icon_sized<'a>(s: Scheme, n: &Notification, size: f32) -> Element<'a, AppMessage> {
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
fn thumb<'a>(n: &Notification) -> Option<Element<'a, AppMessage>> {
    match n.image.as_ref()? {
        Image::Path(p) => ui::rounded_file(std::path::Path::new(p), THUMB, THUMB, tok::R_CARD)
            .map(|h| image(h).width(THUMB).height(THUMB).into()),
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), THUMB, THUMB, tok::R_CARD)),
    }
}

/// The app's buttons the card shows ("default" is the card's own tap).
fn actions(n: &Notification) -> impl Iterator<Item = &(String, String)> {
    n.actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
}

/// What a tap on a card does: the app's default action (DMS: "default", else its first).
fn default_action(n: &Notification) -> Option<AppMessage> {
    n.actions
        .iter()
        .find(|(k, _)| k == "default" || k.starts_with("kestrel-open:"))
        .or_else(|| n.actions.first())
        .map(|(k, _)| msg(Message::Action(n.id, k.clone())))
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
fn card_of<'a>(s: Scheme, n: &Notification, p: Place) -> Element<'a, AppMessage> {
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
            .on_press(msg(close)),
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
                    .on_press(msg(Message::Action(n.id, k.clone()))),
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
        .radius(tok::R_CARD)
        .kind(
            s,
            if p.popup {
                crate::ui::control::Kind::Ghost
            } else {
                crate::ui::control::Kind::Tonal
            },
        )
        .no_press_scale()
        .on_enter(msg(Message::Hover(n.id, true)))
        .on_exit(msg(Message::Hover(n.id, false)));
    let tap = if p.count > 1 {
        Some(msg(Message::Open(n.app.clone(), true)))
    } else {
        default_action(n).or_else(|| can_expand.then(|| msg(Message::More(n.id, !p.more))))
    };
    let card: Element<'a, AppMessage> = ui::swipe(surface)
        .identity(u64::from(n.id))
        .on_tap_maybe(tap)
        .on_dismiss(msg(if p.count > 1 {
            Message::DismissGroup(n.app.clone())
        } else {
            Message::Dismiss(n.id)
        }))
        .into();
    if p.count <= 1 {
        return card;
    }
    // A folded stack (macOS): the newest card, and the next ones peeking under it as thinner,
    // fainter edges (one for two, two for three or more).
    let layer = |inset: f32, alpha: f32| -> Element<'a, AppMessage> {
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
                            bottom_right: tok::R_CARD,
                            bottom_left: tok::R_CARD,
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
pub fn popup<'a>(app: &App, s: Scheme, n: &Notification, width: f32) -> Element<'a, AppMessage> {
    card_of(
        s,
        n,
        Place {
            width,
            count: 1,
            popup: true,
            more: app.notif_more.contains(&n.id),
        },
    )
}

/// The notifications panel at the island: the list, or a quiet bell when there is none.
pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    if app.notifications.is_empty() {
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
    let room = app
        .panel
        .as_ref()
        .and_then(|p| app.output(p.output))
        .map_or(f32::INFINITY, |o| o.size.height - tok::BAR - 2.0 * tok::XL)
        // A compact panel: about five cards, then it scrolls (it ran down the whole screen).
        .min(LIST_MAX);
    container(section(app, s, room))
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
pub fn section(app: &App, s: Scheme, room: f32) -> Element<'_, AppMessage> {
    if app.notifications.is_empty() {
        return Space::new().into();
    }
    let header = row![
        label_w("Notifications", text::BODY, s.ink, theme::SEMIBOLD).width(Length::Fill),
        control(label("Clear all", text::SMALL, s.ink_dim))
            .pill(s, crate::ui::control::Pill::Quiet)
            .on_press(msg(Message::ClearAll)),
    ]
    .align_y(Alignment::Center)
    .padding([tok::NONE, tok::XS]);
    // One group per app, the newest first (DMS, ii).
    let mut groups: Vec<Vec<&Notification>> = Vec::new();
    for n in app.notifications.iter().rev() {
        match groups.iter_mut().find(|g| g[0].app == n.app) {
            Some(g) => g.push(n),
            None => groups.push(vec![n]),
        }
    }
    let mut list = column![].spacing(tok::S);
    for g in &groups {
        let n = g[0];
        let open = g.len() > 1 && app.notif_open.contains(&n.app);
        if !open {
            list = list.push(card_of(
                s,
                n,
                Place {
                    width: LIST_W,
                    count: g.len(),
                    popup: false,
                    more: app.notif_more.contains(&n.id),
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
                .on_press(msg(Message::Open(n.app.clone(), false))),
            control(icon(Icon::X, tok::ICON_XS, s.ink_dim))
                .padding(tok::XS + tok::XXS)
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Tonal)
                .on_press(msg(Message::DismissGroup(n.app.clone()))),
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
                    more: app.notif_more.contains(&m.id),
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
            if app.notif_open.contains(&g[0].app) {
                g.len().min(LIMIT)
            } else {
                1
            }
        })
        .sum();
    // Five fit in LIST_MAX; only a sixth runs past its edge.
    let list: Element<'_, AppMessage> = if cards > 5 && room <= LIST_MAX {
        let tint = if app.config.material == crate::config::Material::Frosted {
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
