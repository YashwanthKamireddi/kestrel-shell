//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Element, Length, Task};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub enum Message {
    Dismiss(u32),
    Action(u32, String),
    ClearAll,
    /// A group (by app) opened to show every message in full, or folded back.
    Open(String, bool),
    /// A message's text put on the clipboard (its Copy button).
    CopyText(u32),
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
        Message::CopyText(id) => {
            if let Some(n) = app.notifications.iter().find(|n| n.id == id) {
                let text = if n.body.trim().is_empty() {
                    n.summary.clone()
                } else {
                    format!("{}\n{}", n.summary, n.body)
                };
                return Task::Iced(iced::clipboard::write(text));
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

// --- The card: ii's notification group, in kestrel's colours (design call 2026-11-15) ----------
//
// Copied from ii (`modules/common/widgets/NotificationGroup.qml`, `NotificationItem.qml`):
// - A round icon on the left: the app's, or a single notification's own picture.
// - The top line: a single notification's title (a group: the app's name), its time right
//   after it, and at the far right the button with the count that opens and folds it.
// - Folded: each message one line, "Title  text…" (the title, then the text cut), the second
//   faded; never taller than about 80 px.
// - Open: each message in full, then one row of equal buttons: Close, the app's own, Copy.
// - No close on hover, no big pictures.

/// The card's inner width: the control centre's list and a banner are both ~368 px wide.
const INNER: f32 = 368.0 - 24.0;
/// The round icon and the gap after it.
const ICON: f32 = 38.0;
const TEXT: f32 = INNER - ICON - tok::M - tok::XXS;

/// The round icon: a single notification's own picture, else the app's icon.
fn lead<'a>(s: Scheme, n: &Notification, single: bool) -> Element<'a, AppMessage> {
    if single {
        let pic = match n.image.as_ref() {
            Some(Image::Path(p)) => {
                ui::rounded_file(std::path::Path::new(p), ICON, ICON, ICON / 2.0)
                    .map(|h| Element::from(image(h).width(ICON).height(ICON)))
            }
            Some(Image::Rgba(p)) => Some(ui::cover(crate::ui::pixels(p), ICON, ICON, ICON / 2.0)),
            None => None,
        };
        if let Some(p) = pic {
            return p;
        }
    }
    container(app_icon(s, n))
        .center(ICON)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(tok::FILL * s.opacity).into()),
            border: iced::Border::default().rounded(ICON / 2.0),
            ..container::Style::default()
        })
        .into()
}

fn flat(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn title_of(n: &Notification) -> &str {
    let t = n.summary.trim();
    if t.is_empty() {
        if n.app.is_empty() {
            "Notification"
        } else {
            &n.app
        }
    } else {
        t
    }
}

/// The open/fold button: the count (a group) and the arrow.
fn expander<'a>(s: Scheme, count: usize, open: bool, key: &str) -> Element<'a, AppMessage> {
    let mut inner = row![].spacing(tok::XXS).align_y(Alignment::Center);
    if count > 1 {
        inner = inner.push(label(count.to_string(), text::CAPTION, s.ink_dim));
    }
    inner = inner.push(icon(
        if open {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        },
        12.0,
        s.ink_dim,
    ));
    control(inner)
        .padding([tok::XXS, tok::S])
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Tonal)
        .on_press(msg(Message::Open(key.to_string(), !open)))
        .into()
}

/// One folded line: "Title  text…" in one row (the title as it is, the text cut).
fn line<'a>(s: Scheme, n: &Notification, with_title: bool, faded: bool) -> Element<'a, AppMessage> {
    let body = flat(&n.body);
    let k = if faded { 0.5 } else { 1.0 };
    let mut r = row![].spacing(tok::S).align_y(Alignment::Center);
    let mut room = TEXT;
    if with_title {
        let t = ui::fit(title_of(n), text::SMALL, TEXT * 0.55);
        room -= t.chars().count() as f32 * text::SMALL * 0.56 + tok::S;
        r = r.push(label_w(t, text::SMALL, s.ink.scale_alpha(k), theme::MEDIUM));
    }
    if !body.is_empty() {
        r = r.push(label(
            ui::fit(&body, text::SMALL, room.max(40.0)),
            text::SMALL,
            s.ink_dim.scale_alpha(k),
        ));
    }
    r.into()
}

/// The open message's buttons: Close, the app's own (never "default"), Copy; equal widths.
fn buttons<'a>(s: Scheme, n: &Notification) -> Element<'a, AppMessage> {
    let pill = |content: Element<'a, AppMessage>, m: AppMessage| -> Element<'a, AppMessage> {
        control(container(content).center_x(Length::Fill))
            .padding([tok::S, tok::M])
            .width(Length::Fill)
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Tonal)
            .on_press(m)
            .into()
    };
    let mut r = row![pill(
        icon(Icon::X, 14.0, s.ink).into(),
        msg(Message::Dismiss(n.id))
    )]
    .spacing(tok::XS);
    for (k, l) in n
        .actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
        .take(3)
    {
        r = r.push(pill(
            label_w(ui::ellipsize(l, 16), text::SMALL, s.ink, theme::MEDIUM).into(),
            msg(Message::Action(n.id, k.clone())),
        ));
    }
    r = r.push(pill(
        icon(Icon::Clipboard, 14.0, s.ink).into(),
        msg(Message::CopyText(n.id)),
    ));
    r.into()
}

/// What a click on a message does: the app's default action, or for kestrel's own entries
/// opening the file.
fn default_action(n: &Notification) -> Option<AppMessage> {
    n.actions
        .iter()
        .find(|(k, _)| k == "default" || k.starts_with("kestrel-open:"))
        .map(|(k, _)| msg(Message::Action(n.id, k.clone())))
}

/// One app's run of messages (a single message is a group of one), folded or open.
fn group<'a>(s: Scheme, g: &[&Notification], open: bool, boxed: bool) -> Element<'a, AppMessage> {
    let n = g[0];
    let single = g.len() == 1;
    let key = n.app.clone();
    // The top line: a single notification's title, a group's app name; the time after it.
    let mut top = row![].spacing(tok::S).align_y(Alignment::Center);
    if single {
        top = top.push(label_w(
            ui::fit(title_of(n), text::BODY, TEXT - 110.0),
            text::BODY,
            s.ink,
            theme::SEMIBOLD,
        ));
    } else {
        top = top.push(label_w(
            ui::fit(
                if n.app.is_empty() {
                    "Notifications"
                } else {
                    &n.app
                },
                text::SMALL,
                TEXT - 110.0,
            ),
            text::SMALL,
            s.ink_dim,
            theme::MEDIUM,
        ));
    }
    top = top
        .push(label(ago(n.time), text::CAPTION, s.ink_faint))
        .push(Space::new().width(Length::Fill))
        .push(expander(s, g.len(), open, &key));
    let mut content = column![top].spacing(tok::XS);
    if !open {
        if single {
            if !flat(&n.body).is_empty() {
                content = content.push(line(s, n, false, false));
            }
        } else {
            content = content.push(line(s, g[0], true, false));
            if let Some(next) = g.get(1) {
                content = content.push(line(s, next, true, true));
            }
        }
    } else {
        for (i, m) in g.iter().enumerate() {
            let mut item = column![].spacing(tok::XS);
            if !single {
                item = item.push(
                    row![
                        label_w(
                            ui::fit(title_of(m), text::SMALL, TEXT - 60.0),
                            text::SMALL,
                            s.ink,
                            theme::SEMIBOLD
                        ),
                        Space::new().width(Length::Fill),
                        label(ago(m.time), text::CAPTION, s.ink_faint),
                    ]
                    .align_y(Alignment::Center),
                );
            }
            let body = flat(&m.body);
            if !body.is_empty() {
                item = item.push(
                    iced::widget::text(lines_of(&body, text::SMALL, TEXT, 8))
                        .size(text::SMALL)
                        .color(s.ink_dim)
                        .font(theme::FONT)
                        .line_height(iced::widget::text::LineHeight::Relative(1.3))
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                );
            }
            item = item.push(buttons(s, m));
            let item = control(item)
                .identity(u64::from(m.id))
                .width(Length::Fill)
                .kind(s, crate::ui::control::Kind::Bare)
                .no_press_scale()
                .on_press_maybe(default_action(m));
            if i > 0 {
                content = content.push(Space::new().height(tok::XS));
            }
            content = content.push(item);
        }
    }
    let body = row![lead(s, n, single), content.width(TEXT)]
        .spacing(tok::M + tok::XXS)
        .align_y(Alignment::Start);
    // Folded, a click opens a group (a single message does what its app asks, else opens).
    let on = if open {
        None
    } else if single {
        default_action(n).or_else(|| Some(msg(Message::Open(key.clone(), true))))
    } else {
        Some(msg(Message::Open(key, true)))
    };
    control(body)
        .identity(u64::from(n.id))
        .padding(tok::M + tok::XXS)
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
        .on_press_maybe(on)
        .into()
}

/// A banner (the same card as the list's, folded; its count button opens it in place).
pub fn card<'a>(s: Scheme, n: &Notification, boxed: bool, open: bool) -> Element<'a, AppMessage> {
    group(s, &[n], open, boxed)
}

/// Notifications inside the control centre: the newest first, one card per app's run of
/// messages, folded until opened; the list scrolls in `room` (the tiles above stay still).
/// Nothing at all when there are none (no "empty" message).
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
    let newest: Vec<&Notification> = app.notifications.iter().rev().collect();
    let mut groups: Vec<Vec<&Notification>> = Vec::new();
    for n in newest {
        match groups.last_mut() {
            Some(g) if g[0].app == n.app => g.push(n),
            _ => groups.push(vec![n]),
        }
    }
    let list = column(
        groups
            .iter()
            .map(|g| group(s, g, app.notif_open.contains(&g[0].app), true)),
    )
    .spacing(tok::S);
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
