//! Wallpaper picker: large thumbnails of the wallpaper folder, the current one ringed in white
//! with a check; a click sets the wallpaper (and with it the colours) and saves the choice. The
//! folder's name chooses another folder (kestrel's file picker, folders only; saved as
//! `wallpaper_dir`); a folder with no pictures says so.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::config;
use crate::theme::tok;
use crate::theme::{Scheme, size, text};
use crate::ui::{self, Icon, control, empty, label_w};
use iced::widget::{column, container, image, row, scrollable};
use iced::{Alignment, Element, Length, Task};
use std::path::{Path, PathBuf};

const COLS: usize = 4;
const THUMB_W: f32 = 188.0;
const THUMB_H: f32 = 106.0;
pub const WIDTH: f32 = 4.0 * (THUMB_W + 6.0) + 3.0 * 10.0 + 28.0;

#[derive(Default)]
pub struct State {
    /// (original, thumbnail handle), kept between opens so the picker shows them at once.
    items: Option<Vec<(PathBuf, image::Handle)>>,
    /// The one tile marked: under the pointer, or where the arrow keys moved (one ring, never
    /// two).
    sel: Option<usize>,
    /// The folder as last read: each file with its modification time. A re-read that finds the
    /// same files changes nothing (it used to redecode and replace every picture on each open).
    stamps: Vec<(PathBuf, u64)>,
    /// How far the grid is scrolled (to bring a key-marked row into view only when needed).
    offset: f32,
}

static GRID_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("wallpaper-grid"));

/// The grid's visible height (three rows).
const VIEW_H: f32 = 3.0 * (THUMB_H + 6.0) + 2.0 * 10.0 + 4.0;
/// One row's step.
const ROW_STEP: f32 = THUMB_H + 6.0 + 10.0;

#[derive(Clone, Debug)]
pub enum Message {
    /// The folder as it is now (files and times), to compare with what is shown.
    Listed(Vec<(PathBuf, u64)>),
    /// Thumbnails of the files that were new or changed, made and rounded off the drawing
    /// thread, with the folder's full order.
    Loaded(Vec<(PathBuf, u64)>, Vec<(PathBuf, image::Handle)>),
    /// The pointer came onto a tile.
    Hover(usize),
    Scrolled(f32),
    /// A random one other than the current.
    Shuffle,
    Set(PathBuf),
    /// Choose another folder (the file picker).
    ChooseFolder,
    /// The folder chosen.
    Folder(PathBuf),
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

fn stamp(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// The folder's pictures and their modification times, sorted (cheap: no image is read).
async fn list(dir: PathBuf) -> Vec<(PathBuf, u64)> {
    tokio::task::spawn_blocking(move || {
        let mut files: Vec<(PathBuf, u64)> = std::fs::read_dir(&dir)
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
                    .map(|p| {
                        let t = stamp(&p);
                        (p, t)
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files
    })
    .await
    .unwrap_or_default()
}

/// Thumbnails for `files`, decoded and rounded off the drawing thread (done on it, the picker
/// stuttered as it opened).
async fn thumbs(files: Vec<PathBuf>) -> Vec<(PathBuf, image::Handle)> {
    tokio::task::spawn_blocking(move || {
        let out = files
            .into_iter()
            .filter_map(|f| {
                let t = thumbnail(&f)?;
                let img = ::image::ImageReader::open(&t)
                    .ok()?
                    .with_guessed_format()
                    .ok()?
                    .decode()
                    .ok()?;
                let r =
                    ui::rounded_cover(&img.into_rgba8(), THUMB_W, THUMB_H, size::ROW_RADIUS - 3.0);
                Some((
                    f,
                    image::Handle::from_rgba(r.width(), r.height(), r.into_raw()),
                ))
            })
            .collect();
        unsafe {
            libc::malloc_trim(0);
        }
        out
    })
    .await
    .unwrap_or_default()
}

/// Brings the marked tile's row into view (keys move past the three rows shown).
fn reveal(app: &App) -> Task<AppMessage> {
    let Some(i) = app.panels.wallpaper.sel else {
        return Task::none();
    };
    let row = (i / COLS) as f32;
    let top = row * ROW_STEP;
    // Only as far as needed: the row just inside the top or bottom edge.
    let y = (top - (VIEW_H - (THUMB_H + 6.0))).max(0.0);
    let off = app.panels.wallpaper.offset;
    let to = if top < off {
        top
    } else if top + THUMB_H + 6.0 > off + VIEW_H {
        y
    } else {
        return Task::none();
    };
    Task::Iced(iced::widget::scroll_to(
        GRID_ID.clone(),
        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: to },
    ))
}

/// Read the folder and make its thumbnails in the background when the shell starts, so the
/// first open shows them at once (it was empty for a second while 96 pictures decoded).
pub fn preload(app: &App) -> Task<AppMessage> {
    Task::perform(list(folder(app)), |v| msg(Message::Listed(v)))
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    // The kept thumbnails show at once; the folder is read again behind them.
    let current = app.config.wallpaper.as_deref().map(config::expand);
    let st = &mut app.panels.wallpaper;
    st.sel = st.items.as_ref().and_then(|v| {
        v.iter()
            .position(|(p, _)| Some(p.as_path()) == current.as_deref())
    });
    let dir = folder(app);
    Task::batch([
        Task::perform(list(dir), |v| msg(Message::Listed(v))),
        // Opens on the row of the current wallpaper (from the top: the grid's own scroll may
        // not be what was last heard).
        {
            app.panels.wallpaper.offset = f32::MAX;
            reveal(app)
        },
    ])
}

/// Arrows move over the tiles, Enter sets the one they are on.
pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    let n = app.panels.wallpaper.items.as_ref().map_or(0, Vec::len);
    if n == 0 {
        return Task::none();
    }
    let current = app.config.wallpaper.as_deref().map(config::expand);
    let st = &mut app.panels.wallpaper;
    // Keys start from the current wallpaper when nothing is marked.
    let at = st
        .sel
        .or_else(|| {
            st.items
                .as_ref()?
                .iter()
                .position(|(p, _)| Some(p.as_path()) == current.as_deref())
        })
        .unwrap_or(0) as i64;
    let to = match k {
        Key::Left => at - 1,
        Key::Right | Key::Tab => at + 1,
        Key::BackTab => at - 1,
        Key::Up => at - COLS as i64,
        Key::Down => at + COLS as i64,
        Key::Home => 0,
        Key::End => n as i64 - 1,
        Key::Enter | Key::Space => {
            let path = st
                .items
                .as_ref()
                .and_then(|v| v.get(at as usize))
                .map(|(p, _)| p.clone());
            return match path {
                Some(p) => update(app, Message::Set(p)),
                None => Task::none(),
            };
        }
        _ => return Task::none(),
    };
    st.sel = Some(to.clamp(0, n as i64 - 1) as usize);
    reveal(app)
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Listed(now) => {
            let st = &mut app.panels.wallpaper;
            if st.items.is_some() && now == st.stamps {
                return Task::none();
            }
            // Only new or changed files are decoded; the rest keep their pictures.
            let need: Vec<PathBuf> = now
                .iter()
                .filter(|f| !st.stamps.contains(f))
                .map(|(p, _)| p.clone())
                .collect();
            Task::perform(thumbs(need), move |made| {
                msg(Message::Loaded(now.clone(), made))
            })
        }
        Message::Loaded(now, made) => {
            let st = &mut app.panels.wallpaper;
            let mut have: std::collections::HashMap<PathBuf, image::Handle> = st
                .items
                .take()
                .unwrap_or_default()
                .into_iter()
                .chain(made)
                .collect();
            st.items = Some(
                now.iter()
                    .filter_map(|(p, _)| have.remove(p).map(|h| (p.clone(), h)))
                    .collect(),
            );
            st.stamps = now;
            Task::none()
        }
        Message::Scrolled(y) => {
            app.panels.wallpaper.offset = y;
            Task::none()
        }
        Message::Hover(i) => {
            app.panels.wallpaper.sel = Some(i);
            Task::none()
        }
        Message::Shuffle => {
            let current = app.config.wallpaper.as_deref().map(config::expand);
            let pick = app.panels.wallpaper.items.as_ref().and_then(|v| {
                let others: Vec<&PathBuf> = v
                    .iter()
                    .map(|(p, _)| p)
                    .filter(|p| Some(p.as_path()) != current.as_deref())
                    .collect();
                if others.is_empty() {
                    return None;
                }
                let n = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_nanos() as usize);
                Some(others[n % others.len()].clone())
            });
            match pick {
                Some(p) => update(app, Message::Set(p)),
                None => Task::none(),
            }
        }
        Message::ChooseFolder => {
            app.panels.files.then = Some(|p| msg(Message::Folder(p)));
            let req = crate::services::filechooser::Request {
                id: crate::services::filechooser::OWN,
                title: "Wallpaper folder".into(),
                directory: true,
                folder: Some(folder(app)),
                accept: "Choose".into(),
                ..Default::default()
            };
            Task::done(AppMessage::Close).chain(Task::done(AppMessage::FileRequest(req)))
        }
        Message::Folder(p) => {
            let h = std::env::var("HOME").unwrap_or_default();
            let shown = match p.strip_prefix(&h) {
                Ok(rest) => format!("~/{}", rest.display()),
                Err(_) => p.display().to_string(),
            };
            config::set(&[], "wallpaper_dir", toml_edit::value(shown.clone()));
            app.config.wallpaper_dir = Some(shown);
            app.panels.wallpaper.items = None;
            // Back to the pictures, now from there.
            Task::done(AppMessage::Ipc("toggle wallpaper".into()))
        }
        Message::Set(p) => {
            if let Some(v) = &app.panels.wallpaper.items {
                app.panels.wallpaper.sel = v.iter().position(|(q, _)| *q == p);
            }
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
            .spacing(tok::S)
            .align_y(Alignment::Center)
        )
        .pill(s, crate::ui::control::Pill::Plain)
        .on_press(msg(Message::ChooseFolder)),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center)
    .height(size::CONTROL + 4.0);
    // Shuffle only with something to shuffle to.
    let header = if app
        .panels
        .wallpaper
        .items
        .as_ref()
        .is_some_and(|v| v.len() > 1)
    {
        header.push(
            control(
                row![
                    ui::icon(Icon::Shuffle, 14.0, s.ink_dim),
                    ui::label("Shuffle", text::SMALL, s.ink)
                ]
                .spacing(tok::S)
                .align_y(Alignment::Center),
            )
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(msg(Message::Shuffle)),
        )
    } else {
        header
    };
    let current = app.config.wallpaper.as_deref().map(config::expand);
    let body: Element<'_, AppMessage> = match &app.panels.wallpaper.items {
        None => empty(s, Icon::Image),
        // A folder with no pictures (or none at all) says which, not just an icon.
        Some(v) if v.is_empty() => {
            let f = folder(app);
            let h = std::env::var("HOME").unwrap_or_default();
            let shown = match f.strip_prefix(&h) {
                Ok(rest) => format!("~/{}", rest.display()),
                Err(_) => f.display().to_string(),
            };
            ui::empty_reason(
                s,
                if f.is_dir() {
                    format!("No pictures in {shown}")
                } else {
                    format!("No folder {shown}")
                },
                3.0 * tok::XXXL * 2.0,
            )
        }
        Some(v) => {
            let mut grid = column![].spacing(tok::L);
            let sel = app.panels.wallpaper.sel;
            for (ci, chunk) in v.chunks(COLS).enumerate() {
                let mut r = row![].spacing(tok::L);
                for (ti, (path, thumb)) in chunk.iter().enumerate() {
                    let on = current.as_deref() == Some(path.as_path());
                    let i = ci * COLS + ti;
                    let marked = sel == Some(i);
                    let picture = iced::widget::image(thumb.clone())
                        .width(THUMB_W)
                        .height(THUMB_H)
                        .opacity(s.opacity);
                    let mut tile = iced::widget::stack![picture];
                    // The current one: a check in its corner.
                    if on {
                        tile = tile.push(
                            container(
                                container(ui::icon(Icon::Check, 13.0, s.base))
                                    .center(20)
                                    .style(move |_| container::Style {
                                        background: Some(s.ink.into()),
                                        border: iced::Border::default().rounded(tok::FULL),
                                        ..container::Style::default()
                                    }),
                            )
                            .width(THUMB_W)
                            .height(THUMB_H)
                            .padding(tok::S)
                            .align_x(Alignment::End)
                            .align_y(Alignment::Start),
                        );
                    }
                    // One ring: the tile under the pointer or the keys (the current one has
                    // its check). Drawn over the picture, at once.
                    if marked || on {
                        let ring = if marked { 0.9 } else { 0.5 };
                        tile = tile.push(
                            container(iced::widget::Space::new())
                                .width(THUMB_W)
                                .height(THUMB_H)
                                .style(move |_| container::Style {
                                    border: iced::Border {
                                        color: s.ink.scale_alpha(ring * s.opacity),
                                        width: 2.0,
                                        radius: (size::ROW_RADIUS - 3.0).into(),
                                    },
                                    ..container::Style::default()
                                }),
                        );
                    }
                    r = r.push(
                        control(tile)
                            .padding(tok::XS)
                            .radius(size::ROW_RADIUS)
                            .kind(s, crate::ui::control::Kind::Bare)
                            .no_press_scale()
                            .on_enter(msg(Message::Hover(i)))
                            .on_press(msg(Message::Set(path.clone()))),
                    );
                }
                grid = grid.push(r);
            }
            crate::ui::smooth(
                scrollable(grid)
                    .id(GRID_ID.clone())
                    .on_scroll(|v| msg(Message::Scrolled(v.absolute_offset().y)))
                    .height(Length::Fixed(VIEW_H))
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::new().width(0).scroller_width(0),
                    )),
            )
            .into()
        }
    };
    container(column![header, body].spacing(tok::L))
        .padding(iced::Padding {
            top: 10.0,
            right: 14.0,
            bottom: 14.0,
            left: 14.0,
        })
        .into()
}
