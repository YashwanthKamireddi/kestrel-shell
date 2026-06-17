//! Notifications: the list in the control centre and the toasts. Both use the same card.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::icons;
use crate::services::notifications::{self, Image, Notification, Reason};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row, scrollable};
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

/// The picture's size on the card's right (a screenshot is wider, as screens are).
fn picture_size(n: &Notification) -> (f32, f32) {
    if n.app == "kestrel" {
        (72.0, 44.0)
    } else {
        (44.0, 44.0)
    }
}

/// A screenshot (or recording) of kestrel's own: shown under the title across the text's
/// width at its own shape (height kept between 70 and 150 px), on a faint backing so a dark
/// picture still shows its edges. A 72 px chip beside the title read as a clipped sliver.
fn preview<'a>(s: Scheme, n: &Notification, width: f32) -> Option<Element<'a, AppMessage>> {
    if n.app != "kestrel" {
        return None;
    }
    let Some(Image::Path(p)) = n.image.as_ref() else {
        return None;
    };
    let path = std::path::Path::new(p);
    let (iw, ih) = image_size(path)?;
    let h = (width * ih as f32 / iw.max(1) as f32)
        .clamp(70.0, 150.0)
        .round();
    let handle = ui::rounded_file(path, width, h, 10.0)?;
    Some(
        container(image(handle).width(width).height(h))
            .padding(tok::HAIR)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::FILL_STRONG * s.opacity).into()),
                border: iced::Border::default().rounded(11),
                ..container::Style::default()
            })
            .into(),
    )
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
    // A handful of screenshots at most are listed; the table stays small.
    if map.len() > 64 {
        map.clear();
    }
    map.insert(path.to_path_buf(), sz);
    Some(sz)
}

fn picture<'a>(n: &Notification) -> Option<Element<'a, AppMessage>> {
    // kestrel's own pictures are a preview under the title (`preview`), not a chip.
    if n.app == "kestrel" && matches!(n.image, Some(Image::Path(_))) {
        return None;
    }
    let (w, h) = picture_size(n);
    match n.image.as_ref()? {
        // Decoded first: an app's unreadable file must never reach the renderer.
        Image::Path(p) => {
            let handle = ui::rounded_file(std::path::Path::new(p), w, h, 8.0)?;
            Some(image(handle).width(w).height(h).into())
        }
        Image::Rgba(p) => Some(ui::cover(crate::ui::pixels(p), w, h, 8.0)),
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

/// One notification. In the list it is a card of its own; as a banner (`boxed` false) the
/// banner's box is its card, so there is no box inside a box.
pub fn card<'a>(
    s: Scheme,
    n: &Notification,
    boxed: bool,
    hovered: bool,
) -> Element<'a, AppMessage> {
    card_in(s, n, boxed, hovered, false, false, true)
}

/// `card`, optionally as a later message in an app's group (`continued`): no icon or app name
/// again, only its own time and close button, lined up under the first message's text.
fn card_in<'a>(
    s: Scheme,
    n: &Notification,
    boxed: bool,
    hovered: bool,
    continued: bool,
    grouped: bool,
    show_time: bool,
) -> Element<'a, AppMessage> {
    // Laid out like a phone's banner: the app's icon on the left; beside it the app and when on
    // one quiet line, the title, then the message wrapped over up to four lines.
    // kestrel's own entries (a screenshot, a recording) need no app line: the title says it.
    // One layout for every notification (screenshots included): the app's icon; beside it
    // the app and when on one quiet line with the close button, the title (up to two lines),
    // the message (up to three); a picture, if any, on the right at a fixed size; actions under.
    // Every piece lives in a text column of a known width, so nothing runs under the picture.
    let own = n.app == "kestrel";
    let pic = picture(n);
    // The card's inner width: the control centre's list or a banner, both ~368 px wide.
    const INNER: f32 = 368.0 - 24.0;
    let text_w = INNER - 32.0 - 12.0 - pic.as_ref().map_or(0.0, |_| picture_size(n).0 + 12.0);
    let app_name: Element<'a, AppMessage> = if continued {
        Space::new().into()
    } else {
        label_w(
            ui::fit(
                if own { "kestrel" } else { &n.app },
                text::CAPTION,
                text_w - 70.0,
            ),
            text::CAPTION,
            s.ink_dim,
            theme::MEDIUM,
        )
        .into()
    };
    let top = row![
        app_name,
        Space::new().width(Length::Fill),
        // When it came at rest, the close button in its place under the pointer (as macOS):
        // one quiet corner instead of a time and a button on every card.
        container(if hovered {
            Element::from(
                control(container(icon(Icon::X, 11.0, s.ink_dim)).center(20))
                    .radius(tok::FULL)
                    .kind(s, crate::ui::control::Kind::Tonal)
                    .on_press(msg(Message::Dismiss(n.id))),
            )
        } else if show_time {
            label(ago(n.time), text::CAPTION, s.ink_faint).into()
        } else {
            // A later message from the same moment: the group's time above says it.
            Space::new().into()
        })
        .align_x(Alignment::End)
        .center_y(20)
        .width(40),
    ]
    .spacing(tok::S)
    .align_y(Alignment::Center);
    let wrapped = |t: String, size: f32, color: Color, bold: bool| {
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
    };
    let mut words = column![].spacing(tok::XXS).width(text_w);
    if !n.summary.trim().is_empty() {
        words = words.push(wrapped(
            lines_of(n.summary.trim(), text::BODY, text_w, 2),
            text::BODY,
            s.ink,
            true,
        ));
    }
    let body_text = n.body.split_whitespace().collect::<Vec<_>>().join(" ");
    if !body_text.is_empty() {
        words = words.push(wrapped(
            lines_of(&body_text, text::SMALL, text_w, 3),
            text::SMALL,
            s.ink_dim,
            false,
        ));
    }
    let mut main = row![words].spacing(tok::L).align_y(Alignment::Start);
    if let Some(p) = pic {
        main = main.push(p);
    }
    let mut texts = column![top, main].spacing(tok::XS).width(Length::Fill);
    if let Some(p) = preview(s, n, INNER - 32.0 - 12.0 - 2.0) {
        texts = texts.push(container(p).padding(iced::Padding {
            top: tok::XS,
            ..iced::Padding::ZERO
        }));
    }
    let actions: Vec<_> = n
        .actions
        .iter()
        .filter(|(k, l)| k != "default" && !l.is_empty())
        .take(3)
        .collect();
    if !actions.is_empty() {
        texts = texts.push(
            container(
                row(actions.into_iter().map(|(k, l)| {
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
                .spacing(tok::S),
            )
            .padding(iced::Padding {
                top: tok::S,
                ..iced::Padding::ZERO
            }),
        );
    }
    let lead: Element<'a, AppMessage> = if continued {
        Space::new().width(32).into()
    } else {
        app_icon(s, n)
    };
    let body = row![lead, texts].spacing(tok::L).align_y(Alignment::Start);
    let col = column![body];
    // What a click on the card does: the app's default action, or for kestrel's own entries
    // (a recording, a picture) opening it.
    let default = n
        .actions
        .iter()
        .find(|(k, _)| k == "default" || k.starts_with("kestrel-open:"))
        .map(|(k, _)| k.clone());
    // In a group the messages sit close: less room under each, none above a later one (its
    // time row is the gap).
    let pad = if grouped {
        iced::Padding {
            top: if continued { tok::NONE } else { tok::L },
            bottom: tok::S,
            left: tok::L,
            right: tok::L,
        }
    } else {
        iced::Padding::from(tok::L)
    };
    control(col)
        .padding(pad)
        .width(Length::Fill)
        .radius(size::CARD_RADIUS)
        // A card that opens something lifts on hover (one with no default action does not
        // react: it has nothing to press).
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
        .on_press_maybe(default.map(|k| msg(Message::Action(n.id, k))))
        .into()
}

/// Notifications inside the control centre: the newest first, cleared one by one or all at
/// once. Nothing at all when there are none (no "empty" message).
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
    // Consecutive messages from one app share one card (as on phones and macOS): the app once,
    // its messages under it; a single message is its own card as before.
    let newest: Vec<&Notification> = app.notifications.iter().rev().collect();
    let mut groups: Vec<Vec<&Notification>> = Vec::new();
    for n in newest {
        match groups.last_mut() {
            Some(g) if g[0].app == n.app => g.push(n),
            _ => groups.push(vec![n]),
        }
    }
    let hover = app.notif_hover;
    // The newest few groups in full, the rest behind "Show N more" (a long list cut off at the
    // panel's edge looked unfinished).
    const FIRST: usize = 3;
    let more = groups.len().saturating_sub(FIRST);
    if !app.notif_all {
        groups.truncate(FIRST);
    }
    let list = column(groups.into_iter().map(|g| {
        if g.len() == 1 {
            return card(s, g[0], true, hover == Some(g[0].id));
        }
        // Each message's time only where it differs from the one above ("now", "now", "now"
        // on every line was noise).
        let inner = column(g.iter().enumerate().map(|(k, n)| {
            let show_time = k == 0 || ago(g[k - 1].time) != ago(n.time);
            card_in(s, n, false, hover == Some(n.id), k > 0, true, show_time)
        }));
        container(inner)
            .width(Length::Fill)
            .padding(iced::Padding {
                bottom: tok::XS,
                ..iced::Padding::ZERO
            })
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
                border: iced::Border::default().rounded(size::CARD_RADIUS),
                ..container::Style::default()
            })
            .into()
    }))
    .spacing(tok::M);
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
    let body: Element<'_, AppMessage> = if app.notif_all {
        let list =
            scrollable(list)
                .height(Length::Shrink)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().width(0).scroller_width(0),
                ));
        container(crate::ui::smooth(list)).max_height(520).into()
    } else {
        list.into()
    };
    let mut c = column![header, body].spacing(tok::M);
    if let Some(t) = toggle {
        c = c.push(container(t).center_x(Length::Fill));
    }
    c.into()
}
