//! Wallpaper picker: large thumbnails of the wallpaper folder, the current one ringed in white
//! with a check; a click sets the wallpaper (and with it the colours) and saves the choice. The
//! folder's name opens it in the file manager.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::config;
use crate::theme::{Scheme, size, text};
use crate::ui::{self, Icon, control, empty, label_w};
use iced::widget::{column, container, image, row, scrollable};
use iced::{Alignment, Color, Element, Length, Task};
use std::path::{Path, PathBuf};

const COLS: usize = 4;
const THUMB_W: f32 = 188.0;
const THUMB_H: f32 = 106.0;
pub const WIDTH: f32 = 4.0 * (THUMB_W + 6.0) + 3.0 * 10.0 + 28.0;

#[derive(Default)]
pub struct State {
    /// (original, thumbnail handle)
    items: Option<Vec<(PathBuf, image::Handle)>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Loaded(Vec<(PathBuf, PathBuf)>),
    Set(PathBuf),
    OpenFolder,
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Wallpaper(m))
}

pub fn folder(app: &App) -> PathBuf {
    if let Some(d) = &app.config.wallpaper_dir {
        return config::expand(d);
    }
    // Default: the folder of the current wallpaper, else ~/Pictures/Wallpapers.
    app.config
        .wallpaper
        .as_deref()
        .map(config::expand)
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| config::expand("~/Pictures/Wallpapers"))
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_default()
        .join("kestrel")
        .join("thumbs")
}

/// Thumbnails are cached by file name and modification time.
fn thumbnail(src: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(src).ok()?;
    let stamp = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let name = format!(
        "{}-{stamp}.jpg",
        src.file_name()?.to_string_lossy().replace(['/', ' '], "_")
    );
    let out = cache_dir().join(name);
    if out.is_file() {
        return Some(out);
    }
    let img = ::image::open(src)
        .ok()?
        .resize_to_fill(
            (THUMB_W * 2.0) as u32,
            (THUMB_H * 2.0) as u32,
            ::image::imageops::FilterType::Triangle,
        )
        .into_rgb8();
    std::fs::create_dir_all(cache_dir()).ok()?;
    img.save(&out).ok()?;
    Some(out)
}

async fn load(dir: PathBuf) -> Vec<(PathBuf, PathBuf)> {
    tokio::task::spawn_blocking(move || {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().is_some_and(|e| {
                            matches!(
                                e.to_string_lossy().to_lowercase().as_str(),
                                "jpg" | "jpeg" | "png" | "webp"
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let out = files
            .into_iter()
            .filter_map(|f| thumbnail(&f).map(|t| (f, t)))
            .collect();
        unsafe {
            libc::malloc_trim(0);
        }
        out
    })
    .await
    .unwrap_or_default()
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    if app.panels.wallpaper.items.is_some() {
        return Task::none();
    }
    let dir = folder(app);
    Task::perform(load(dir), |v| msg(Message::Loaded(v)))
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Loaded(v) => {
            app.panels.wallpaper.items = Some(
                v.into_iter()
                    .filter_map(|(p, t)| {
                        // Rounded in the pixels, so the selection ring meets round corners.
                        // An unreadable picture is left out (never handed to the renderer).
                        let h = ::image::ImageReader::open(&t)
                            .ok()?
                            .with_guessed_format()
                            .ok()?
                            .decode()
                            .ok()
                            .map(|img| {
                                let r = ui::rounded_cover(
                                    &img.into_rgba8(),
                                    THUMB_W,
                                    THUMB_H,
                                    size::ROW_RADIUS - 3.0,
                                );
                                image::Handle::from_rgba(r.width(), r.height(), r.into_raw())
                            })?;
                        Some((p, h))
                    })
                    .collect(),
            );
            Task::none()
        }
        Message::OpenFolder => {
            let _ = std::process::Command::new("xdg-open")
                .arg(folder(app))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            Task::done(AppMessage::Close)
        }
        Message::Set(p) => {
            let s = p.to_string_lossy().into_owned();
            config::set_wallpaper(&s);
            app.config.wallpaper = Some(s);
            Task::done(AppMessage::ReloadWallpaper)
        }
    }
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let name = folder(app)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let header = row![
        label_w("Wallpaper", text::HEADING, s.ink, crate::theme::SEMIBOLD).width(Length::Fill),
        control(
            row![
                ui::icon(Icon::Folder, 14.0, s.ink_dim),
                ui::label(name, text::SMALL, s.ink)
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        )
        .padding([5, 12])
        .radius(14.0)
        .colors(
            s.ink.scale_alpha(0.08 * s.opacity),
            s.ink.scale_alpha(0.14 * s.opacity),
            s.ink.scale_alpha(0.2 * s.opacity),
        )
        .on_press(msg(Message::OpenFolder)),
    ]
    .align_y(Alignment::Center)
    .height(size::CONTROL + 4.0);
    let current = app.config.wallpaper.as_deref().map(config::expand);
    let body: Element<'_, AppMessage> = match &app.panels.wallpaper.items {
        None => empty(s, Icon::Image, "Loading…"),
        Some(v) if v.is_empty() => empty(s, Icon::Image, "No pictures in this folder"),
        Some(v) => {
            let mut grid = column![].spacing(10);
            for chunk in v.chunks(COLS) {
                let mut r = row![].spacing(10);
                for (path, thumb) in chunk {
                    let on = current.as_deref() == Some(path.as_path());
                    let picture = iced::widget::image(thumb.clone())
                        .width(THUMB_W)
                        .height(THUMB_H)
                        .opacity(s.opacity);
                    // The current one: a white ring (the padding) and a check in its corner.
                    let tile: Element<'_, AppMessage> = if on {
                        iced::widget::stack![
                            picture,
                            container(
                                container(ui::icon(Icon::Check, 13.0, s.base))
                                    .center(20)
                                    .style(move |_| container::Style {
                                        background: Some(s.ink.into()),
                                        border: iced::Border::default().rounded(10),
                                        ..container::Style::default()
                                    })
                            )
                            .width(THUMB_W)
                            .height(THUMB_H)
                            .padding(7)
                            .align_x(Alignment::End)
                            .align_y(Alignment::Start),
                        ]
                        .into()
                    } else {
                        picture.into()
                    };
                    r = r.push(
                        control(tile)
                            .padding(3)
                            .radius(size::ROW_RADIUS)
                            .colors(
                                if on { s.ink } else { Color::TRANSPARENT },
                                if on {
                                    s.ink
                                } else {
                                    s.ink.scale_alpha(0.22 * s.opacity)
                                },
                                if on {
                                    s.ink
                                } else {
                                    s.ink.scale_alpha(0.3 * s.opacity)
                                },
                            )
                            .on_press(msg(Message::Set(path.clone()))),
                    );
                }
                grid = grid.push(r);
            }
            scrollable(grid)
                .height(Length::Fixed(3.0 * (THUMB_H + 6.0) + 2.0 * 10.0 + 4.0))
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().width(0).scroller_width(0),
                ))
                .into()
        }
    };
    container(column![header, body].spacing(12))
        .padding(iced::Padding {
            top: 10.0,
            right: 14.0,
            bottom: 14.0,
            left: 14.0,
        })
        .into()
}
