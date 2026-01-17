//! Now playing: artwork, title, artist, progress, transport. Chips to switch only when two
//! different things are playing.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::mpris::{self, Player};
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, empty, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Color, Element, Length, Task};

#[derive(Default)]
pub struct State {
    /// Player chosen with the chips (bus name).
    pub selected: Option<String>,
    /// Last read position (µs) and when it was read.
    pub position: Option<(i64, std::time::Instant)>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Select(String),
    PlayPause(String),
    Next(String),
    Previous(String),
    Seek(String, f32),
    Position(Option<i64>),
    Poll,
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Media(m))
}

pub fn current(app: &App) -> Option<&Player> {
    let sel = app.panels.media.selected.as_ref();
    sel.and_then(|b| app.media.players.iter().find(|p| &p.bus == b))
        .or_else(|| app.media.main())
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Select(b) => {
            app.panels.media.selected = Some(b);
            app.panels.media.position = None;
            return Task::done(msg(Message::Poll));
        }
        Message::PlayPause(b) => {
            if let Some(p) = app.media.players.iter_mut().find(|p| p.bus == b) {
                p.playing = !p.playing;
            }
            return Task::future(mpris::command(mpris::Command::PlayPause(b))).discard();
        }
        Message::Next(b) => {
            app.panels.media.position = None;
            return Task::future(mpris::command(mpris::Command::Next(b))).discard();
        }
        Message::Previous(b) => {
            app.panels.media.position = None;
            return Task::future(mpris::command(mpris::Command::Previous(b))).discard();
        }
        Message::Seek(b, frac) => {
            if let Some(p) = app.media.players.iter().find(|p| p.bus == b) {
                let pos = (frac.clamp(0.0, 1.0) as f64 * p.length as f64) as i64;
                app.panels.media.position = Some((pos, std::time::Instant::now()));
                return Task::future(mpris::command(mpris::Command::Seek(b, pos))).discard();
            }
        }
        Message::Position(p) => {
            app.panels.media.position = p.map(|p| (p, std::time::Instant::now()));
        }
        Message::Poll => {
            if let Some(p) = current(app) {
                return Task::perform(mpris::position(p.bus.clone()), |p| {
                    msg(Message::Position(p))
                });
            }
        }
    }
    Task::none()
}

fn clock(us: i64) -> String {
    let s = (us / 1_000_000).max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn art<'a, M: Clone + 'static>(
    s: Scheme,
    p: &Player,
    size: f32,
    radius: f32,
) -> Element<'a, M> {
    match &p.art {
        Some(path) if std::path::Path::new(path).is_file() => match trimmed_art(path) {
            Some(h) => crate::ui::cover(h, size, size, radius),
            None => crate::ui::cover(image::Handle::from_path(path), size, size, radius),
        },
        _ => container(icon(Icon::Music, size * 0.4, s.ink_faint))
            .center(size)
            .style(move |_| container::Style {
                background: Some(s.raised.into()),
                border: iced::Border::default().rounded(radius),
                ..container::Style::default()
            })
            .into(),
    }
}

/// Artwork with flat bars cut away: video thumbnails (Shorts, portrait clips) often come as a
/// 16:9 frame with filled sides, which a square crop shows as a picture cut off at the edges.
/// Decoded once per file, downscaled, and kept.
fn trimmed_art(path: &str) -> Option<image::Handle> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    static CACHE: LazyLock<Mutex<HashMap<String, Option<image::Handle>>>> =
        LazyLock::new(Default::default);
    if let Some(hit) = CACHE.lock().ok()?.get(path) {
        return hit.clone();
    }
    let made = (|| {
        let img = ::image::open(path).ok()?.thumbnail(512, 512).into_rgba8();
        let (x, y, w, h) = content_box(&img);
        let cut = ::image::imageops::crop_imm(&img, x, y, w, h).to_image();
        Some(image::Handle::from_rgba(
            cut.width(),
            cut.height(),
            cut.into_raw(),
        ))
    })();
    let mut cache = CACHE.lock().ok()?;
    if cache.len() > 32 {
        cache.clear();
    }
    cache.insert(path.to_string(), made.clone());
    made
}

/// The part of an image inside uniform bars on its edges (at most 40 % cut from each side).
fn content_box(img: &::image::RgbaImage) -> (u32, u32, u32, u32) {
    let (w, h) = img.dimensions();
    let flat = |pixels: &mut dyn Iterator<Item = &::image::Rgba<u8>>| {
        let v: Vec<[i32; 3]> = pixels
            .map(|p| [i32::from(p[0]), i32::from(p[1]), i32::from(p[2])])
            .collect();
        let n = v.len().max(1) as i32;
        let mean = v
            .iter()
            .fold([0; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        let mean = mean.map(|c| c / n);
        // Nearly every pixel within a small distance of the line's mean colour.
        let off = v
            .iter()
            .filter(|p| (0..3).map(|i| (p[i] - mean[i]).abs()).sum::<i32>() > 36)
            .count();
        (off as f32) < v.len() as f32 * 0.02
    };
    let col = |x: u32| flat(&mut (0..h).map(|y| img.get_pixel(x, y)));
    let row = |y: u32| flat(&mut (0..w).map(|x| img.get_pixel(x, y)));
    let (mut l, mut r, mut t, mut b) = (0, w, 0, h);
    while l < w * 2 / 5 && col(l) {
        l += 1;
    }
    while r > w * 3 / 5 && col(r - 1) {
        r -= 1;
    }
    while t < h * 2 / 5 && row(t) {
        t += 1;
    }
    while b > h * 3 / 5 && row(b - 1) {
        b -= 1;
    }
    (l, t, (r - l).max(1), (b - t).max(1))
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let Some(p) = current(app) else {
        return container(empty(s, Icon::Music, "Nothing is playing"))
            .padding(12)
            .into();
    };
    let bus = p.bus.clone();

    let mut top = column![].spacing(12);
    if app.media.players.len() > 1 {
        let chips = row(app.media.players.iter().map(|q| {
            let on = q.bus == p.bus;
            control(label(
                ui::ellipsize(
                    if q.identity.is_empty() {
                        &q.title
                    } else {
                        &q.identity
                    },
                    18,
                ),
                text::SMALL,
                if on { s.on_accent } else { s.ink_dim },
            ))
            .padding([4, 10])
            .radius(12.0)
            .colors(
                if on { s.accent } else { s.raised },
                if on { s.accent } else { s.raised_hi },
                s.raised_press,
            )
            .on_press(msg(Message::Select(q.bus.clone())))
            .into()
        }))
        .spacing(6);
        top = top.push(chips);
    }

    let mut meta = column![label_w(
        ui::ellipsize(&p.title, 38),
        text::TITLE,
        s.ink,
        theme::SEMIBOLD
    )]
    .spacing(3);
    if !p.artist.is_empty() {
        meta = meta.push(label(ui::ellipsize(&p.artist, 44), text::SMALL, s.ink_dim));
    }
    if !p.identity.is_empty() {
        meta = meta.push(label(p.identity.clone(), text::CAPTION, s.ink_faint));
    }
    top = top.push(
        row![
            art(s, p, 72.0, 12.0),
            container(meta).center_y(72).width(Length::Fill)
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    );

    // Progress, when the player reports a length.
    if p.length > 0 {
        let pos = app
            .panels
            .media
            .position
            .map(|(pos, at)| {
                if p.playing {
                    pos + at.elapsed().as_micros() as i64
                } else {
                    pos
                }
            })
            .unwrap_or(0)
            .min(p.length);
        let frac = pos as f32 / p.length as f32;
        let b2 = bus.clone();
        let bar =
            crate::ui::slider::slider(Space::new(), frac, 6.0, (s.raised_hi, s.accent), move |f| {
                msg(Message::Seek(b2.clone(), f))
            });
        top = top.push(
            column![
                bar,
                row![
                    label(clock(pos), text::CAPTION, s.ink_faint),
                    Space::new().width(Length::Fill),
                    label(clock(p.length), text::CAPTION, s.ink_faint),
                ]
            ]
            .spacing(6),
        );
    }

    let side = |i: Icon, m: Option<AppMessage>| {
        control(container(icon(i, 18.0, if m.is_some() { s.ink } else { s.ink_faint })).center(40))
            .radius(20.0)
            .colors(Color::TRANSPARENT, s.raised_hi, s.raised_press)
            .on_press_maybe(m)
    };
    let play = control(
        container(icon(
            if p.playing { Icon::Pause } else { Icon::Play },
            20.0,
            s.on_accent,
        ))
        .center(48),
    )
    .radius(24.0)
    .colors(s.accent, s.accent, s.accent)
    .on_press(msg(Message::PlayPause(bus.clone())));
    let transport = row![
        side(
            Icon::SkipBack,
            p.can_prev.then(|| msg(Message::Previous(bus.clone())))
        ),
        play,
        side(
            Icon::SkipForward,
            p.can_next.then(|| msg(Message::Next(bus.clone())))
        ),
    ]
    .spacing(16)
    .align_y(Alignment::Center);
    top = top.push(container(transport).center_x(Length::Fill));

    container(top).padding(16).into()
}

/// The bar chip: a small cover and the title.
pub fn chip<'a>(s: Scheme, p: &Player, active: bool) -> Element<'a, AppMessage> {
    let fg = if active { s.accent } else { s.ink };
    row![
        art(s, p, 18.0, 5.0),
        label(ui::ellipsize(&p.title, 32), text::SMALL, fg),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::content_box;

    #[test]
    fn side_bars_are_cut() {
        // A 160x90 frame: brown bars 50 px wide each side, a varied picture between.
        let img = ::image::RgbaImage::from_fn(160, 90, |x, y| {
            if !(50..110).contains(&x) {
                ::image::Rgba([90, 60, 40, 255])
            } else {
                ::image::Rgba([(x * 3) as u8, (y * 2) as u8, 128, 255])
            }
        });
        let (x, y, w, h) = content_box(&img);
        assert_eq!((x, w), (50, 60));
        assert_eq!((y, h), (0, 90));
    }
}
