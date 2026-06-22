//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Color, Element, Length, Task};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub enum Message {
    Dismiss(u32),
    /// Show every notification group, or only the newest few.
    ShowAll(bool),
    /// The pointer is on this card (None: on none).
    Hover(Option<u32>),
    Action(u32, String),
    ClearAll,
    /// A group (by app) opened to show every message in full, or folded back.
    Open(String, bool),
    /// A whole group dismissed (its ×, folded).
    DismissAll(Vec<u32>),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Notifications(m))
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::ShowAll(on) => {
            app.notif_all = on;
            Task::none()
        }
        Message::Hover(id) => {
            app.notif_hover = id;
            Task::none()
        }
        Message::Dismiss(id) => {
            if app.notif_hover == Some(id) {
                app.notif_hover = None;
            }
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
        Message::DismissAll(ids) => {
            app.notifications.retain(|n| !ids.contains(&n.id));
            app.toasts.retain(|t| !ids.contains(&t.id));
            if app.notif_hover.is_some_and(|h| ids.contains(&h)) {
                app.notif_hover = None;
            }
            Task::batch(
                ids.into_iter()
                    .map(|id| Task::future(notifications::closed(id, Reason::Dismissed)).discard()),
            )
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
    const D: f32 = 32.0;
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

/// A picture file's size, read from its header once per file.
fn image_size(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static SIZES: Mutex<Option<HashMap<std::path::PathBuf, (u32, u32)>>> = Mutex::new(None);
    let mut guard = SIZES.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(sz) = map.get(path) {
        return Some(*sz);
    }
    let sz = ::image::image_dimensions(path).ok()?;
    if map.len() > 64 {
        map.clear();
    }
    map.insert(path.to_path_buf(), sz);
    Some(sz)
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

// --- One card system (design call 2026-11-15, after ii's and Caelestia's groups) ---------------
//
// Every notification is built from the same three pieces, so nothing drifts apart:
// - `head`: the app's icon and name, the count, the time, and one fixed slot at the end for
//   the close button (shown on hover) and the open/fold arrow. The time never moves.
// - folded: the newest message's title and a line of it, a small square picture on the
//   right; a group shows the next title faded under it. No big pictures, no buttons.
// - open (`message`): every message in full: title, time and close on one line, the text,
//   its picture across the text's width (capped), its buttons last.
// A banner is one message, open, with the small picture: readable at once, never a wall.

/// The card's inner width: the control centre's list and a banner are both ~368 px wide.
const INNER: f32 = 368.0 - 24.0;
/// The app's icon and the gap after it: the text column starts here.
const LEAD: f32 = 28.0 + 12.0;
const TEXT: f32 = INNER - LEAD;
/// The small square picture beside a folded message or a banner.
const THUMB: f32 = 40.0;
/// An open message's picture: across the text, never taller than this.
const PICTURE_MAX: f32 = 120.0;
/// The close button's and the arrow's slot (kept when empty, so nothing moves).
const SLOT: f32 = 20.0;

/// The small square picture, if the notification has one.
fn thumb<'a>(n: &Notification) -> Option<Element<'a, AppMessage>> {
    match n.image.as_ref()? {
        // Decoded first: an app's unreadable file must never reach the renderer.
        Image::Path(p) => {
            let handle = ui::rounded_file(std::path::Path::new(p), THUMB, THUMB, 8.0)?;
            Some(image(handle).width(THUMB).height(THUMB).into())
        }
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), THUMB, THUMB, 8.0)),
    }
}

/// The picture across the text's width at its own shape, capped at `PICTURE_MAX`.
fn picture_wide<'a>(n: &Notification) -> Option<Element<'a, AppMessage>> {
    match n.image.as_ref()? {
        Image::Path(p) => {
            let path = std::path::Path::new(p);
            let (iw, ih) = image_size(path)?;
            let h = (TEXT * ih as f32 / iw.max(1) as f32)
                .clamp(48.0, PICTURE_MAX)
                .round();
            let handle = ui::rounded_file(path, TEXT, h, 10.0)?;
            Some(image(handle).width(TEXT).height(h).into())
        }
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), TEXT, PICTURE_MAX, 10.0)),
    }
}

/// The app's icon at the card's size (`app_icon` is drawn at 32; scaled in its slot).
fn lead<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
    container(app_icon(s, n)).center(28).into()
}

/// The quiet close button (fixed 20 px slot: present or not, nothing moves).
fn close<'a>(s: Scheme, on: Option<AppMessage>) -> Element<'a, AppMessage> {
    match on {
        Some(m) => control(container(icon(Icon::X, 11.0, s.ink_dim)).center(20))
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Tonal)
            .on_press(m)
            .into(),
        None => Space::new().width(SLOT).height(SLOT).into(),
    }
}

fn text_line<'a>(t: String, size: f32, color: Color, bold: bool) -> Element<'a, AppMessage> {
    iced::widget::text(t)
        .size(size)
        .color(color)
        .font(if bold {
            iced::Font {
                weight: theme::SEMIBOLD,
                ..theme::FONT
            }
        } else {
            theme::FONT
        })
        .line_height(iced::widget::text::LineHeight::Relative(1.3))
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .width(Length::Fill)
        .into()
}

fn flat(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The app's name for the header ("kestrel" for its own).
fn app_name(n: &Notification) -> &str {
    if n.app.is_empty() {
        "Notification"
    } else {
        &n.app
    }
}

/// The header: icon, app, count, time, then the close/arrow slot.
fn head<'a>(
    s: Scheme,
    n: &Notification,
    count: usize,
    close_on: Option<AppMessage>,
    arrow: Option<(bool, AppMessage)>,
    with_time: bool,
) -> Element<'a, AppMessage> {
    let mut r = row![
        lead(s, n),
        label_w(
            ui::fit(app_name(n), text::CAPTION, TEXT - 120.0),
            text::CAPTION,
            s.ink_dim,
            theme::MEDIUM
        ),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);
    if count > 1 {
        r = r.push(
            container(label(count.to_string(), text::CAPTION, s.ink_dim))
                .padding([tok::NONE, tok::S])
                .style(move |_| container::Style {
                    background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                }),
        );
    }
    r = r.push(Space::new().width(Length::Fill));
    if with_time {
        r = r.push(label(ago(n.time), text::CAPTION, s.ink_faint));
    }
    r = r.push(close(s, close_on));
    // The arrow's slot is kept even with nothing to open, so every card's time lines up.
    if arrow.is_none() {
        r = r.push(Space::new().width(SLOT).height(SLOT));
    }
    if let Some((open, m)) = arrow {
        r = r.push(
            control(
                container(icon(
                    if open {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    },
                    14.0,
                    s.ink_dim,
                ))
                .center(20),
            )
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(m),
        );
    }
    r.into()
}

/// The action buttons (the app's own, never "default"), at most three.
fn actions<'a>(s: Scheme, n: &Notification) -> Option<Element<'a, AppMessage>> {
    let list: Vec<_> = n
        .actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
        .take(3)
        .collect();
    (!list.is_empty()).then(|| {
        row(list.into_iter().map(|(k, l)| {
            control(label_w(
                ui::ellipsize(l, 20),
                text::SMALL,
                s.ink,
                theme::MEDIUM,
            ))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::Action(n.id, k.clone())))
            .into()
        }))
        .spacing(tok::S)
        .into()
    })
}

/// What a click on a message does: the app's default action, or for kestrel's own entries
/// opening the file.
fn default_action(n: &Notification) -> Option<AppMessage> {
    n.actions
        .iter()
        .find(|(k, _)| k == "default" || k.starts_with("kestrel-open:"))
        .map(|(k, _)| msg(Message::Action(n.id, k.clone())))
}

/// Whether a message has more than its folded summary shows.
fn has_more(n: &Notification) -> bool {
    n.image.is_some()
        || n.actions
            .iter()
            .any(|(k, l)| k != "default" && !l.is_empty())
        || flat(&n.body).chars().count() > 46
}

/// One message open in full, as a row of the text column: title with its time and close,
/// the text, the picture across the text (`wide`) or small on the right, the buttons last.
fn message<'a>(
    s: Scheme,
    n: &Notification,
    hovered: bool,
    with_time: bool,
    wide: bool,
) -> Element<'a, AppMessage> {
    let mut title = row![].spacing(tok::S).align_y(Alignment::Center);
    let summary = n.summary.trim();
    title = title.push(text_line(
        lines_of(
            if summary.is_empty() {
                app_name(n)
            } else {
                summary
            },
            text::BODY,
            TEXT - 70.0,
            2,
        ),
        text::BODY,
        s.ink,
        true,
    ));
    if with_time {
        title = title.push(label(ago(n.time), text::CAPTION, s.ink_faint));
        title = title.push(close(s, hovered.then(|| msg(Message::Dismiss(n.id)))));
    }
    let mut c = column![title].spacing(tok::XS);
    let body = flat(&n.body);
    let pic_side = (!wide).then(|| thumb(n)).flatten();
    let text_w = TEXT - pic_side.as_ref().map_or(0.0, |_| THUMB + tok::L);
    if !body.is_empty() || pic_side.is_some() {
        let mut r = row![
            container(text_line(
                lines_of(&body, text::SMALL, text_w, if wide { 6 } else { 3 }),
                text::SMALL,
                s.ink_dim,
                false
            ))
            .width(text_w)
        ]
        .spacing(tok::L)
        .align_y(Alignment::Start);
        if let Some(p) = pic_side {
            r = r.push(p);
        }
        c = c.push(r);
    }
    if wide && let Some(p) = picture_wide(n) {
        c = c.push(p);
    }
    if let Some(a) = actions(s, n) {
        c = c.push(container(a).padding(iced::Padding {
            top: tok::XS,
            ..iced::Padding::ZERO
        }));
    }
    c.width(TEXT).into()
}

/// A banner: one message, open, with the small picture (readable at once, never a wall).
pub fn card<'a>(
    s: Scheme,
    n: &Notification,
    boxed: bool,
    hovered: bool,
) -> Element<'a, AppMessage> {
    let body = column![
        head(
            s,
            n,
            1,
            hovered.then(|| msg(Message::Dismiss(n.id))),
            None,
            true
        ),
        row![
            Space::new().width(LEAD - tok::L),
            message(s, n, hovered, false, false)
        ]
        .spacing(tok::L),
    ]
    .spacing(tok::XS);
    control(body)
        .identity(u64::from(n.id))
        .padding(tok::L)
        .width(Length::Fill)
        .radius(size::CARD_RADIUS)
        .kind(
            s,
            if boxed {
                crate::ui::control::Kind::Tonal
            } else {
                crate::ui::control::Kind::Ghost
            },
        )
        .no_press_scale()
        .on_enter(msg(Message::Hover(Some(n.id))))
        .on_exit(msg(Message::Hover(None)))
        .on_press_maybe(default_action(n))
        .into()
}

/// A group in the list (one app's messages in a row; a single message is a group of one).
/// Folded: the newest message's title and a line, the next title faded; a click opens a group
/// (a single message does what the app asks). Open: every message in full.
fn group<'a>(
    s: Scheme,
    g: &[&Notification],
    open: bool,
    hover: Option<u32>,
) -> Element<'a, AppMessage> {
    let n = g[0];
    let key = n.app.clone();
    let expandable = g.len() > 1 || has_more(n);
    let arrow = expandable.then(|| (open, msg(Message::Open(key.clone(), !open))));
    let ids: Vec<u32> = g.iter().map(|m| m.id).collect();
    let card_hovered = hover == Some(n.id);
    let dismiss = if g.len() == 1 {
        msg(Message::Dismiss(n.id))
    } else {
        msg(Message::DismissAll(ids))
    };
    // Folded, the card is one target with one close (the group's); open, each message is.
    let style_card = |content: Element<'a, AppMessage>, on: Option<AppMessage>| {
        let c = control(content)
            .identity(u64::from(n.id))
            .padding(tok::L)
            .width(Length::Fill)
            .radius(size::CARD_RADIUS)
            .kind(s, crate::ui::control::Kind::Tonal)
            .no_press_scale();
        let c = if open {
            c
        } else {
            c.on_enter(msg(Message::Hover(Some(n.id))))
                .on_exit(msg(Message::Hover(None)))
        };
        c.on_press_maybe(on).into()
    };
    if !open {
        // Folded: the summary.
        let summary = n.summary.trim();
        let title = if summary.is_empty() {
            app_name(n)
        } else {
            summary
        };
        let line = flat(&n.body);
        let pic = thumb(n);
        let text_w = TEXT - pic.as_ref().map_or(0.0, |_| THUMB + tok::L);
        let mut words = column![label_w(
            ui::fit(title, text::BODY, text_w),
            text::BODY,
            s.ink,
            theme::SEMIBOLD
        )]
        .spacing(tok::XXS)
        .width(text_w);
        if !line.is_empty() {
            words = words.push(label(
                ui::fit(&line, text::SMALL, text_w),
                text::SMALL,
                s.ink_dim,
            ));
        }
        if let Some(next) = g.get(1) {
            let t = next.summary.trim();
            words = words.push(label(
                ui::fit(
                    if t.is_empty() { app_name(next) } else { t },
                    text::SMALL,
                    text_w,
                ),
                text::SMALL,
                s.ink_faint,
            ));
        }
        let mut r = row![Space::new().width(LEAD - tok::L), words]
            .spacing(tok::L)
            .align_y(Alignment::Center);
        if let Some(p) = pic {
            r = r.push(p);
        }
        let content = column![
            head(
                s,
                n,
                g.len(),
                card_hovered.then(|| dismiss.clone()),
                arrow,
                true
            ),
            r
        ]
        .spacing(tok::XS)
        .into();
        // A group opens on a click; a single message does what its app asks (or opens).
        let on = if g.len() > 1 {
            Some(msg(Message::Open(key, true)))
        } else {
            default_action(n).or_else(|| expandable.then(|| msg(Message::Open(key, true))))
        };
        return style_card(content, on);
    }
    // Open: every message in full, each its own target (its click, its close).
    // Open: the header is only who it is from and the arrow; each message has its time and
    // its close (two closes side by side, and the newest time twice, read as clutter).
    let mut c = column![head(s, n, g.len(), None, arrow, false)].spacing(tok::S);
    for m in g {
        let hovered = hover == Some(m.id);
        let item = control(
            row![
                Space::new().width(LEAD - tok::L),
                message(s, m, hovered, true, true)
            ]
            .spacing(tok::L),
        )
        .identity(u64::from(m.id))
        .padding([tok::XS, tok::NONE])
        .width(Length::Fill)
        .radius(size::ROW_RADIUS)
        .kind(s, crate::ui::control::Kind::Bare)
        .no_press_scale()
        .on_enter(msg(Message::Hover(Some(m.id))))
        .on_exit(msg(Message::Hover(None)))
        .on_press_maybe(default_action(m));
        c = c.push(item);
    }
    style_card(c.into(), None)
}

/// Notifications inside the control centre: the newest first, one card per app's run of
/// messages, folded until opened; cleared one by one, by group or all at once. Nothing at all
/// when there are none (no "empty" message).
pub fn section(app: &App, s: Scheme) -> Element<'_, AppMessage> {
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
    let newest: Vec<&Notification> = app.notifications.iter().rev().collect();
    let mut groups: Vec<Vec<&Notification>> = Vec::new();
    for n in newest {
        match groups.last_mut() {
            Some(g) if g[0].app == n.app => g.push(n),
            _ => groups.push(vec![n]),
        }
    }
    let hover = app.notif_hover;
    // Folded cards are small: the newest five, the rest behind "Show N more".
    const FIRST: usize = 5;
    let more = groups.len().saturating_sub(FIRST);
    if !app.notif_all {
        groups.truncate(FIRST);
    }
    let list = column(
        groups
            .iter()
            .map(|g| group(s, g, app.notif_open.contains(&g[0].app), hover)),
    )
    .spacing(tok::S);
    let toggle = (more > 0).then(|| {
        control(label(
            if app.notif_all {
                "Show fewer".to_string()
            } else {
                format!("Show {more} more")
            },
            text::SMALL,
            s.ink_dim,
        ))
        .pill(s, crate::ui::control::Pill::Quiet)
        .on_press(msg(Message::ShowAll(!app.notif_all)))
    });
    let mut c = column![header, list].spacing(tok::M);
    if let Some(t) = toggle {
        c = c.push(container(t).center_x(Length::Fill));
    }
    c.into()
}
