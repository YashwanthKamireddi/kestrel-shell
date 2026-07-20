//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Element, Length, Task, font};
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
                let _ = std::process::Command::new("xdg-open")
                    .arg(path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
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
    use std::io::Write;
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
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&data);
        }
        // Reaped in the background (wl-copy stays to serve the clipboard).
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Copies a kept picture into Pictures/Screenshots under a dated name.
fn save_picture(from: &str) -> Option<std::path::PathBuf> {
    let pictures = std::process::Command::new("xdg-user-dir")
        .arg("PICTURES")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|p| !p.is_empty())?;
    let dir = std::path::Path::new(&pictures).join("Screenshots");
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
    const D: f32 = AVATAR;
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
        return container(icon(glyph, 17.0, s.ink))
            .center(D)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(D / 2.0),
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
        Some(p) => container(ui::image_file_faded(&p, D, s.opacity))
            .center(D)
            .into(),
        None => container(icon(Icon::Bell, 18.0, s.ink))
            .center(D)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(D / 2.0),
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
const AVATAR: f32 = 36.0;
/// The header line's height (DMS controlSize, buttonHeightXS).
const HEAD: f32 = 32.0;
/// A picture at the card's right (DMS thumbnailSize, buttonHeightM).
const THUMB: f32 = 56.0;
/// Lines of text a folded card shows (DMS collapsedLines).
const LINES: usize = 2;
/// Messages an open group shows (DMS expandedLimit).
const LIMIT: usize = 10;

/// The text column's width on a card `width` wide.
fn text_width(width: f32) -> f32 {
    width - 2.0 * tok::L - AVATAR - tok::L
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

/// A plain text button (DMS "text" variant): accent text, a soft fill only under the pointer.
fn text_button<'a>(s: Scheme, l: &str, room: f32, m: AppMessage) -> Element<'a, AppMessage> {
    control(label_w(
        ui::fit(l, text::SMALL, room),
        text::SMALL,
        s.accent,
        theme::MEDIUM,
    ))
    .padding([tok::M + tok::XXS, tok::M])
    .radius(tok::FULL)
    .kind(s, crate::ui::control::Kind::Ghost)
    .on_press(m)
    .into()
}

/// A round icon button in the header (DMS DActionButton, controlSize).
fn icon_button<'a>(s: Scheme, i: Icon, m: AppMessage) -> Element<'a, AppMessage> {
    control(icon(i, 16.0, s.ink_dim))
        .width(HEAD)
        .height(HEAD)
        .center_x()
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(m)
        .into()
}

/// The count chip that opens and folds a group (DMS: the count and an arrow on a chip).
fn count_chip<'a>(s: Scheme, count: usize, open: bool, app: &str) -> Element<'a, AppMessage> {
    control(
        row![
            label(count.to_string(), text::SMALL, s.ink_dim),
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
        .spacing(tok::XXS)
        .align_y(Alignment::Center),
    )
    .height(HEAD)
    .padding([tok::NONE, tok::M])
    .radius(tok::FULL)
    .kind(s, crate::ui::control::Kind::Tonal)
    .on_press(msg(Message::Open(app.to_string(), !open)))
    .into()
}

/// How a card sits in its list and what it offers.
#[derive(Clone, Copy)]
struct Place {
    width: f32,
    /// Its top and bottom corners (round at a list's ends, tight between cards).
    top: f32,
    bottom: f32,
    /// A folded group's count (1 for a lone message) and whether it is open.
    count: usize,
    /// A pop-up: ✕ and the arrow only under the pointer; "Clear" instead of "Dismiss".
    popup: bool,
    hovered: bool,
    more: bool,
}

/// One notification's card (DMS NotificationCard).
fn card_of<'a>(s: Scheme, n: &Notification, p: Place) -> Element<'a, AppMessage> {
    let tw = text_width(p.width);
    let pic = thumb(n);
    let body_w = if pic.is_some() {
        tw - THUMB - tok::L
    } else {
        tw
    };
    let body = flat(&n.body);
    let title = title_of(n).to_string();
    // Cut when folded: the title to a line, the text to LINES lines (DMS canExpand).
    let cut_title = ui::fit(&title, text::TITLE, body_w);
    let cut_body = lines_of(&body, text::SMALL, body_w, LINES);
    let can_expand = p.count <= 1 && (cut_title != title || cut_body != body);
    let reveal = !p.popup || p.hovered;

    // The header: "App · time", then the controls at the right.
    let mut controls = row![].spacing(tok::XXS).align_y(Alignment::Center);
    if p.count > 1 {
        controls = controls.push(count_chip(s, p.count, false, &n.app));
    }
    if reveal && can_expand {
        controls = controls.push(icon_button(
            s,
            if p.more {
                Icon::ChevronUp
            } else {
                Icon::ChevronDown
            },
            msg(Message::More(n.id, !p.more)),
        ));
    }
    if p.popup && p.hovered {
        controls = controls.push(icon_button(s, Icon::X, msg(Message::Hide(n.id))));
    }
    let head = row![
        label(
            ui::fit(
                &format!("{} · {}", app_name(n), ago(n.time)),
                text::SMALL,
                tw - 2.0 * HEAD,
            ),
            text::SMALL,
            s.ink_dim,
        )
        .width(Length::Fill),
        controls,
    ]
    .height(HEAD)
    .align_y(Alignment::Center);

    let wrap = |t: String, size: f32, color, weight| {
        iced::widget::text(t)
            .size(size)
            .color(color)
            .font(theme::font(weight))
            .line_height(iced::widget::text::LineHeight::Relative(1.2))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .width(Length::Fill)
    };
    let mut words = column![].spacing(tok::XS);
    if !title.is_empty() {
        words = words.push(wrap(
            if p.more { title } else { cut_title },
            text::TITLE,
            s.ink,
            theme::MEDIUM,
        ));
    }
    if !body.is_empty() {
        words = words.push(wrap(
            if p.more { body } else { cut_body },
            text::SMALL,
            s.ink_dim,
            font::Weight::Normal,
        ));
    }
    let message: Element<'a, AppMessage> = match pic {
        Some(pic) => row![words.width(Length::Fill), pic]
            .spacing(tok::L)
            .align_y(Alignment::Start)
            .into(),
        None => words.into(),
    };

    // The app's buttons as plain text, then Dismiss (Clear on a pop-up), from the left edge.
    let acts: Vec<_> = actions(n).take(3).collect();
    let dismiss = if p.popup { "Clear" } else { "Dismiss" };
    // Each label at its own width; only when the row cannot hold them all are the longest
    // cut, to a shared cap (equal shares cut "Show in folder" while room was left over).
    let wide = |l: &str| l.chars().count() as f32 * text::SMALL * 0.56;
    let mut widths: Vec<f32> = acts.iter().map(|(_, l)| wide(l)).collect();
    widths.push(wide(dismiss));
    let free =
        tw + tok::M - tok::XS * (widths.len() - 1) as f32 - 2.0 * tok::M * widths.len() as f32;
    let mut cap = f32::INFINITY;
    if widths.iter().sum::<f32>() > free {
        let mut sorted = widths.clone();
        sorted.sort_by(f32::total_cmp);
        let mut left = free;
        for (i, w) in sorted.iter().enumerate() {
            let share = left / (sorted.len() - i) as f32;
            if *w > share {
                cap = share;
                break;
            }
            left -= w;
        }
    }
    let room = cap.max(40.0);
    let mut buttons = row![].spacing(tok::XS).align_y(Alignment::Center);
    for (k, l) in acts {
        buttons = buttons.push(text_button(
            s,
            l,
            room,
            msg(Message::Action(n.id, k.clone())),
        ));
    }
    buttons = buttons.push(text_button(
        s,
        dismiss,
        room,
        msg(if p.count > 1 {
            Message::DismissGroup(n.app.clone())
        } else {
            Message::Dismiss(n.id)
        }),
    ));

    let content = column![
        head,
        message,
        container(buttons).padding(iced::Padding::ZERO.left(-tok::M))
    ]
    .spacing(tok::XS)
    .width(tw);
    let inner = row![
        container(app_icon(s, n)).padding(iced::Padding::ZERO.top(tok::XXS)),
        content
    ]
    .spacing(tok::L)
    .align_y(Alignment::Start);
    let surface = control(inner)
        .identity(u64::from(n.id))
        .padding(tok::L)
        .width(Length::Fill)
        .corners(p.top, p.bottom)
        .kind(
            s,
            if p.popup {
                crate::ui::control::Kind::Ghost
            } else {
                crate::ui::control::Kind::Tonal
            },
        )
        .no_press_scale();
    let surface: Element<'a, AppMessage> = if p.popup {
        surface
            .on_enter(msg(Message::Hover(n.id, true)))
            .on_exit(msg(Message::Hover(n.id, false)))
            .into()
    } else {
        surface.into()
    };
    // A folded group's tap opens it; a lone message's does what its app asks.
    let tap = if p.count > 1 {
        Some(msg(Message::Open(n.app.clone(), true)))
    } else {
        default_action(n).or_else(|| can_expand.then(|| msg(Message::More(n.id, !p.more))))
    };
    ui::swipe(surface)
        .identity(u64::from(n.id))
        .on_tap_maybe(tap)
        .on_dismiss(msg(if p.count > 1 {
            Message::DismissGroup(n.app.clone())
        } else {
            Message::Dismiss(n.id)
        }))
        .into()
}

/// A pop-up (the same card, ✕ and the arrow on hover).
pub fn popup<'a>(app: &App, s: Scheme, n: &Notification, width: f32) -> Element<'a, AppMessage> {
    card_of(
        s,
        n,
        Place {
            width,
            top: tok::R_CARD,
            bottom: tok::R_CARD,
            count: 1,
            popup: true,
            hovered: app.toast_hover == Some(n.id),
            more: app.notif_more.contains(&n.id),
        },
    )
}

/// The control centre's list width (the panel's 392 less its padding).
const LIST_W: f32 = 392.0 - 2.0 * size::PANEL_PAD;

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
    let last = groups.len() - 1;
    let end = |round: bool| if round { tok::R_CARD } else { tok::R_JOIN };
    let mut list = column![].spacing(tok::XXS);
    for (i, g) in groups.iter().enumerate() {
        let n = g[0];
        let open = g.len() > 1 && app.notif_open.contains(&n.app);
        if !open {
            list = list.push(card_of(
                s,
                n,
                Place {
                    width: LIST_W,
                    top: end(i == 0),
                    bottom: end(i == last),
                    count: g.len(),
                    popup: false,
                    hovered: false,
                    more: app.notif_more.contains(&n.id),
                },
            ));
            continue;
        }
        // Open: the app's name, Dismiss (all of them) and the chip, over each message's card.
        let head = row![
            label_w(
                ui::fit(app_name(n), text::TITLE, LIST_W / 2.0),
                text::TITLE,
                s.accent,
                theme::MEDIUM
            )
            .width(Length::Fill),
            text_button(
                s,
                "Dismiss",
                LIST_W / 3.0,
                msg(Message::DismissGroup(n.app.clone()))
            ),
            count_chip(s, g.len(), true, &n.app),
        ]
        .spacing(tok::XS)
        .align_y(Alignment::Center)
        .padding([tok::L, tok::L]);
        list = list.push(head);
        let shown = g.len().min(LIMIT);
        for (j, m) in g.iter().take(LIMIT).enumerate() {
            list = list.push(card_of(
                s,
                m,
                Place {
                    width: LIST_W,
                    top: end(j == 0),
                    bottom: end(j + 1 == shown),
                    count: 1,
                    popup: false,
                    hovered: false,
                    more: app.notif_more.contains(&m.id),
                },
            ));
        }
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
    column![header, list].spacing(tok::M).into()
}
