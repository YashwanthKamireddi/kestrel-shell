//! Now playing: artwork, title, artist, progress, transport. Chips to switch only when two
//! different things are playing.

use super::action::Action;
use super::cx::Cx;
use crate::services::mpris::{self, Player};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{Space, column, container, image, row};
use iced::{Alignment, Color, Element, Length, Task};

#[derive(Default)]
pub struct State {
    /// Player chosen with the chips (bus name).
    pub selected: Option<String>,
    /// Last read position (µs) and when it was read.
    pub position: Option<(i64, std::time::Instant)>,
    /// Play or pause pressed (the player's bus, playing wanted, when): shown at once, until
    /// the player says so itself or for at most `WANT`. The player's own state is never
    /// written ahead of it (it is the truth; this is only what was asked).
    want: Option<(String, bool, std::time::Instant)>,
}

const WANT: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether `p` shows as playing: what was just asked, until the player agrees.
pub fn playing(st: &State, p: &Player) -> bool {
    match &st.want {
        Some((bus, want, at)) if *bus == p.bus && at.elapsed() < WANT => *want,
        _ => p.playing,
    }
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
    /// Redraw the moving progress between position reads.
    Tick,
}

/// The player shown: the one picked in the panel, else the main one.
pub fn current<'a>(st: &State, media: &'a mpris::State) -> Option<&'a Player> {
    let sel = st.selected.as_ref();
    sel.and_then(|b| media.players.iter().find(|p| &p.bus == b))
        .or_else(|| media.main())
}

pub fn update(st: &mut State, m: Message, cx: &Cx) -> Action<Message> {
    let run = |c: mpris::Command| Action::Run(Task::future(mpris::command(c)).discard());
    match m {
        Message::Select(b) => {
            st.selected = Some(b);
            st.position = None;
            return Action::Run(Task::done(Message::Poll));
        }
        Message::PlayPause(b) => {
            if let Some(p) = cx.media.players.iter().find(|p| p.bus == b) {
                st.want = Some((b.clone(), !playing(st, p), std::time::Instant::now()));
            }
            return run(mpris::Command::PlayPause(b));
        }
        Message::Next(b) => {
            st.position = None;
            return run(mpris::Command::Next(b));
        }
        Message::Previous(b) => {
            st.position = None;
            return run(mpris::Command::Previous(b));
        }
        Message::Seek(b, frac) => {
            if let Some(p) = cx.media.players.iter().find(|p| p.bus == b) {
                let pos = (frac.clamp(0.0, 1.0) as f64 * p.length as f64) as i64;
                st.position = Some((pos, std::time::Instant::now()));
                return run(mpris::Command::Seek(b, pos));
            }
        }
        Message::Position(p) => {
            st.position = p.map(|p| (p, std::time::Instant::now()));
        }
        Message::Tick => {}
        Message::Poll => {
            if let Some(p) = current(st, cx.media) {
                return Action::Run(Task::perform(
                    mpris::position(p.bus.clone()),
                    Message::Position,
                ));
            }
        }
    }
    Action::None
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
    crate::ui::snap::snap(art_plain(s, p, size, radius)).into()
}

/// `art` without snapping to whole pixels, for use inside something snapped as a whole.
pub fn art_plain<'a, M: Clone + 'static>(
    s: Scheme,
    p: &Player,
    size: f32,
    radius: f32,
) -> Element<'a, M> {
    match &p.art {
        // Only a picture that decoded is drawn: a file the renderer cannot read would abort it.
        Some(path)
            if std::path::Path::new(path).is_file() && art_at(path, size, radius).is_some() =>
        {
            let h = art_at(path, size, radius)
                .unwrap_or_else(|| image::Handle::from_rgba(1, 1, vec![0; 4]));
            iced::widget::image(h)
                .width(size)
                .height(size)
                .opacity(s.opacity)
                .into()
        }
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
/// The artwork at one display size: trimmed, square, corners rounded. Kept per size.
fn art_at(path: &str, size: f32, radius: f32) -> Option<image::Handle> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    type Key = (String, u32, u32);
    static SIZED: LazyLock<Mutex<HashMap<Key, Option<image::Handle>>>> =
        LazyLock::new(Default::default);
    let key = (path.to_string(), size as u32, radius as u32);
    if let Some(hit) = SIZED.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let made = art_pixels(path).map(|img| {
        let r = crate::ui::rounded_cover(&img, size, size, radius);
        image::Handle::from_rgba(r.width(), r.height(), r.into_raw())
    });
    let mut cache = SIZED.lock().ok()?;
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert(key, made.clone());
    made
}

/// The artwork's pixels with flat bars cut away (cached by `art_data`).
fn art_pixels(path: &str) -> Option<::image::RgbaImage> {
    art_data(path).map(|(img, _)| img)
}

/// Whether the player shows real artwork (a file that decodes and is not a browser's icon).
pub fn has_art(p: &Player) -> bool {
    p.art.as_deref().is_some_and(|a| art_data(a).is_some())
}

/// Something a browser plays with no picture of its own (a muted hover preview, a page with no
/// artwork): not worth a music bubble.
pub fn is_bare_browser(p: &Player) -> bool {
    let b = p.bus.to_lowercase();
    [
        "chromium",
        "chrome",
        "brave",
        "firefox",
        "edge",
        "vivaldi",
        "kestrel_browser",
    ]
    .iter()
    .any(|n| b.contains(n))
        && !has_art(p)
}

/// The artwork's average colour, for tinting what surrounds it.
pub fn art_colour(p: &Player) -> Option<Color> {
    art_data(p.art.as_deref()?).map(|(_, c)| c)
}

/// Whether `img` is a browser's own icon rather than artwork: compared, small and blurred, with
/// the installed icons of the browsers that do this.
fn is_browser_logo(img: &::image::RgbaImage) -> bool {
    use std::sync::LazyLock;
    const N: u32 = 24;
    let shrink = |i: &::image::RgbaImage| {
        ::image::imageops::resize(i, N, N, ::image::imageops::FilterType::Triangle)
    };
    static LOGOS: LazyLock<Vec<::image::RgbaImage>> = LazyLock::new(|| {
        [
            "chromium",
            "google-chrome",
            "brave-browser",
            "microsoft-edge",
        ]
        .iter()
        .filter_map(|name| {
            let p = format!("/usr/share/icons/hicolor/256x256/apps/{name}.png");
            ::image::open(p).ok().map(|i| {
                ::image::imageops::resize(
                    &i.into_rgba8(),
                    N,
                    N,
                    ::image::imageops::FilterType::Triangle,
                )
            })
        })
        .collect()
    });
    // A logo is square-ish; artwork of other shapes is never one.
    let (w, h) = img.dimensions();
    if w.abs_diff(h) > w.max(h) / 8 {
        return false;
    }
    let small = shrink(img);
    LOGOS.iter().any(|logo| {
        // Mean colour difference over the opaque parts of the logo.
        let (mut diff, mut n) = (0u64, 0u64);
        for (a, b) in small.pixels().zip(logo.pixels()) {
            if b[3] < 128 {
                continue;
            }
            n += 1;
            diff += (0..3).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>();
        }
        n > 0 && diff / (n * 3) < 28
    })
}

fn art_data(path: &str) -> Option<(::image::RgbaImage, Color)> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    type Entry = Option<(::image::RgbaImage, Color)>;
    static CACHE: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(Default::default);
    if let Some(hit) = CACHE.lock().ok()?.get(path) {
        return hit.clone();
    }
    let made = (|| {
        // By content, not by name (a file can be named anything).
        let img = ::image::ImageReader::open(path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?
            .thumbnail(512, 512)
            .into_rgba8();
        if img.width() == 0 || img.height() == 0 {
            return None;
        }
        // A browser with nothing to show (a muted hover preview, a page without artwork) hands
        // over its own app icon as the "cover": that is not artwork, so none is shown.
        if is_browser_logo(&img) {
            return None;
        }
        let (x, y, w, h) = content_box(&img);
        let cut = ::image::imageops::crop_imm(&img, x, y, w, h).to_image();
        let n = u64::from((cut.width() * cut.height()).max(1));
        let sum = cut.pixels().fold([0u64; 3], |a, p| {
            [
                a[0] + u64::from(p[0]),
                a[1] + u64::from(p[1]),
                a[2] + u64::from(p[2]),
            ]
        });
        let avg = Color::from_rgb8((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8);
        Some((cut, avg))
    })();
    let mut cache = CACHE.lock().ok()?;
    // Each entry is up to 512 × 512 pixels (~1 MB): the current track and the last two.
    if cache.len() > 3 {
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

/// Panel width: room for a large cover beside the text.
pub const WIDTH: f32 = crate::theme::tok::PANEL_M;
/// The panel's width with nothing playing.
pub const EMPTY_WIDTH: f32 = 240.0;

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let Some(p) = current(st, cx.media) else {
        // Says why it is empty in the panel itself (a pop-up saying it read as noise).
        return container(crate::ui::empty_reason(
            s,
            "Nothing is playing".into(),
            96.0,
        ))
        .padding(tok::L)
        .into();
    };
    let bus = p.bus.clone();

    let mut top = column![].spacing(tok::L);
    if cx.media.players.len() > 1 {
        // Named by app, unless two come from the same app: then by what they play.
        let same_app = |q: &Player| {
            cx.media
                .players
                .iter()
                .filter(|o| o.identity == q.identity)
                .count()
                > 1
        };
        let chips = row(cx.media.players.iter().map(|q| {
            let on = q.bus == p.bus;
            control(label(
                ui::ellipsize(
                    if q.identity.is_empty() || same_app(q) {
                        &q.title
                    } else {
                        &q.identity
                    },
                    22,
                ),
                text::SMALL,
                if on { s.on_accent } else { s.ink_dim },
            ))
            .padding([tok::XS, tok::L])
            .radius(tok::FULL)
            .kind(
                s,
                if on {
                    crate::ui::control::Kind::Primary
                } else {
                    crate::ui::control::Kind::Tonal
                },
            )
            .on_press(Message::Select(q.bus.clone()))
            .into()
        }))
        .spacing(tok::S);
        top = top.push(chips);
    }

    // Right of the artwork: where it plays, what it is, how far along, and the controls.
    const ART: f32 = 136.0;
    let text_w = WIDTH - 2.0 * 16.0 - ART - 16.0;
    let mut meta = column![].spacing(tok::XS);
    // Where it plays (no visualiser here: the bar's pill already shows one).
    let mut source = row![].align_y(Alignment::Center);
    if !p.identity.is_empty() {
        source = source.push(
            container(label(p.identity.clone(), text::CAPTION, s.ink_dim))
                .padding([tok::XXS, tok::M])
                .style(move |_| container::Style {
                    background: Some(s.ink.scale_alpha(tok::FILL).into()),
                    border: iced::Border::default().rounded(tok::R_CONTROL()),
                    ..container::Style::default()
                }),
        );
    }
    meta = meta.push(source);
    meta = meta.push(label_w(
        ui::two_lines(&p.title, text::TITLE, text_w),
        text::TITLE,
        s.ink,
        theme::SEMIBOLD,
    ));
    if !p.artist.is_empty() {
        meta = meta.push(label(
            ui::fit(&p.artist, text::SMALL, text_w),
            text::SMALL,
            s.ink_dim,
        ));
    }

    // Progress, when the player reports a length.
    if p.length > 0 && !cx.length_unreliable.contains(&p.bus) {
        let pos = st
            .position
            .map(|(pos, at)| {
                if playing(st, p) {
                    pos + at.elapsed().as_micros() as i64
                } else {
                    pos
                }
            })
            .unwrap_or(0)
            .min(p.length);
        let frac = pos as f32 / p.length as f32;
        let b2 = bus.clone();
        let bar = crate::ui::slider::slider(
            Space::new(),
            frac,
            6.0,
            (s.ink.scale_alpha(tok::SELECTED), s.accent),
            move |f| Message::Seek(b2.clone(), f),
        );
        meta = meta.push(Space::new().height(tok::XS));
        meta = meta.push(
            column![
                bar,
                row![
                    label(clock(pos), text::CAPTION, s.ink_faint),
                    Space::new().width(Length::Fill),
                    label(clock(p.length), text::CAPTION, s.ink_faint),
                ]
            ]
            .spacing(tok::XS),
        );
    }

    let side = |i: Icon, m: Option<Message>| {
        control(
            container(icon(
                i,
                tok::ICON_M,
                if m.is_some() { s.ink } else { s.ink_faint },
            ))
            .center(36),
        )
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press_maybe(m)
    };
    let play = control(
        container(icon(
            if playing(st, p) {
                Icon::Pause
            } else {
                Icon::Play
            },
            20.0,
            s.on_accent,
        ))
        .center(48),
    )
    .radius(tok::FULL)
    .kind(s, crate::ui::control::Kind::Primary)
    .on_press(Message::PlayPause(bus.clone()));
    let transport = row![
        side(
            Icon::SkipBack,
            p.can_prev.then(|| Message::Previous(bus.clone()))
        ),
        play,
        side(
            Icon::SkipForward,
            p.can_next.then(|| Message::Next(bus.clone()))
        ),
    ]
    .spacing(tok::L)
    .align_y(Alignment::Center);
    meta = meta.push(container(transport).center_x(Length::Fill));

    top = top.push(
        row![art(s, p, ART, 16.0), container(meta).width(Length::Fill)]
            .spacing(tok::XL)
            .align_y(Alignment::Center),
    );

    // The panel takes a deep tint of the artwork's own colour; frosted, it sits on the frost
    // like every other panel.
    // On the frosted look the artwork's colour is a soft wash over the real blur.
    let tint = art_colour(p).map(|c| {
        if crate::theme::glass() {
            s.fade(c.scale_alpha(0.32))
        } else {
            s.fade(mix(c, s.base, 0.72))
        }
    });
    // Both ends fade with the panel (the bottom at full strength stayed behind on close).
    let base = if crate::theme::glass() {
        Color::TRANSPARENT
    } else {
        s.fade(s.base)
    };
    container(top)
        .padding(crate::theme::tok::PANEL_PAD)
        .style(move |_| container::Style {
            background: tint.map(|deep| {
                iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2 * 3.0))
                    .add_stop(0.0, deep)
                    .add_stop(1.0, base)
                    .into()
            }),
            border: iced::Border::default().rounded(crate::theme::tok::R_PANEL()),
            ..container::Style::default()
        })
        .into()
}

/// `a` mixed toward `b` by `t` (0 = a, 1 = b).
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

/// The bar chip: a small cover and the title.
pub fn chip<'a, M: Clone + 'static>(s: Scheme, p: &Player, active: bool) -> Element<'a, M> {
    let fg = if active { s.accent } else { s.ink };
    row![
        art(s, p, 18.0, 5.0),
        label(ui::ellipsize(&p.title, 32), text::SMALL, fg),
    ]
    .spacing(tok::M)
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

#[cfg(test)]
mod logo_tests {
    #[test]
    fn the_browsers_icon_is_not_artwork() {
        let Ok(logo) = ::image::open("/usr/share/icons/hicolor/128x128/apps/chromium.png") else {
            return; // Not installed here.
        };
        assert!(super::is_browser_logo(&logo.into_rgba8()));
        let mut photo = ::image::RgbaImage::new(64, 64);
        for (x, y, p) in photo.enumerate_pixels_mut() {
            *p = ::image::Rgba([(x * 4) as u8, (y * 4) as u8, 90, 255]);
        }
        assert!(!super::is_browser_logo(&photo));
    }
}

/// Whether play or pause was asked for and its answer is awaited (tests).
#[cfg(test)]
pub(crate) fn asked(st: &State) -> bool {
    st.want.is_some()
}

/// Space plays or pauses the player in front; the arrows go to the previous or next track.
pub fn key(st: &mut State, k: super::Key, cx: &Cx) -> Action<Message> {
    use super::Key;
    let Some(bus) = cx.media.main().map(|p| p.bus.clone()) else {
        return Action::None;
    };
    let m = match k {
        Key::Space | Key::Enter => Message::PlayPause(bus),
        Key::Left => Message::Previous(bus),
        Key::Right => Message::Next(bus),
        _ => return Action::None,
    };
    update(st, m, cx)
}
