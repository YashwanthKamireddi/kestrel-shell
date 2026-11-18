//! The lock screen (`kestrel lock`): its own small process, as the prompt process is
//! (docs/ARCHITECTURE.md, Stability), started by hypridle's `lock_cmd` (docs/LOCK.md).
//!
//! It locks the session with the Wayland session lock protocol (ext-session-lock-v1), the way
//! swaylock and hyprlock do: the compositor hides everything and gives input only to this
//! process's lock surfaces, one per output. If this process crashes or is killed, the session
//! stays locked (the compositor's guarantee, not ours). It unlocks only after PAM accepts the
//! password (`pam::Accepted`, which only a successful check can produce), and exits.
//!
//! Nothing here blocks the UI thread: PAM runs on a blocking thread (pam_unix waits about two
//! seconds after a wrong password), the wallpaper is read and blurred on another, and Esc gives
//! up on a check that never answers. No network: no request, no D-Bus call, nothing but the
//! compositor, PAM and two files (the colours in state.toml, the wallpaper).

mod pam;
mod secret;
mod state;

use crate::theme::{self, Scheme, text, tok};
use crate::ui::{icon::Icon, label, label_w};
use iced::widget::{Space, column, container, row, stack};
use iced::{
    Alignment, Anchor, ContentFit, Element, KeyboardInteractivity, Layer, LayerShellSettings,
    Length, LockEvent, OutputEvent, OutputId, Subscription, SurfaceId, Task, Theme,
};
use state::{Effect, Input, Lock, Session, Status};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// The picture behind is blurred at this width (then drawn stretched, which smooths it more):
/// blurring a small copy is quick, and a lock screen's background needs no detail.
const BLUR_WIDTH: u32 = 480;
/// Gaussian blur strength at `BLUR_WIDTH`.
const BLUR_SIGMA: f32 = 6.0;
/// How much of `base` lies over the picture, so text stays easy to read.
const SCRIM: f32 = 0.45;
/// At most this many dots for the typed characters (the field's width).
const MAX_DOTS: usize = 24;

/// Set when the compositor refused the lock (another lock screen holds the session).
static REFUSED: AtomicBool = AtomicBool::new(false);

pub struct App {
    lock: Lock,
    scheme: Scheme,
    /// The login name PAM checks, and what is shown (the full name when there is one).
    user: String,
    shown_name: String,
    surfaces: HashMap<OutputId, SurfaceId>,
    wallpaper: Option<iced::widget::image::Handle>,
    now: chrono::DateTime<chrono::Local>,
    h12: bool,
    /// devtools: check against the stand-in password instead of PAM.
    preview: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Session(LockEvent),
    Output(OutputEvent),
    Key(Input),
    Modifiers,
    Verdict(u64, pam::Verdict),
    Wallpaper(Option<iced::widget::image::Handle>),
    Tick,
}

#[cfg(feature = "devtools")]
fn preview_on() -> bool {
    std::env::var("KESTREL_PREVIEW").is_ok_and(|v| v == "lock")
}

#[cfg(not(feature = "devtools"))]
fn preview_on() -> bool {
    false
}

fn boot(user: String, shown_name: String) -> (App, Task<Message>) {
    let (scheme, _) = crate::prompt::look();
    let config = crate::config::load();
    let app = App {
        lock: Lock::default(),
        scheme,
        user,
        shown_name,
        surfaces: HashMap::new(),
        wallpaper: None,
        now: chrono::Local::now(),
        h12: config.clock.format == crate::config::ClockFormat::H12,
        preview: preview_on(),
    };
    // The lock first: nothing else is waited for before the screen is locked.
    let task = Task::batch([iced::lock_session(), load_wallpaper(config.wallpaper)]);
    (app, task)
}

/// The wallpaper, small and blurred, read off the UI thread (the shell's screen-sized copy
/// when it has one; this process never writes the shell's cache).
fn load_wallpaper(path: Option<String>) -> Task<Message> {
    let Some(path) = path else {
        return Task::none();
    };
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let path = crate::config::expand(&path);
                let img = match crate::app::wallpaper::wallpaper_cache_read(&path) {
                    Some((img, _)) => img,
                    None => image::open(&path).ok()?,
                };
                let h = (u64::from(BLUR_WIDTH) * u64::from(img.height())
                    / u64::from(img.width().max(1)))
                .max(1) as u32;
                let small = img.resize_exact(BLUR_WIDTH, h, image::imageops::FilterType::Triangle);
                let blurred = image::imageops::fast_blur(&small.into_rgba8(), BLUR_SIGMA);
                let (w, h) = blurred.dimensions();
                Some(iced::widget::image::Handle::from_rgba(
                    w,
                    h,
                    blurred.into_raw(),
                ))
            })
            .await
            .ok()
            .flatten()
        },
        Message::Wallpaper,
    )
}

/// Carries out what the lock state decided.
fn effect(app: &App, e: Effect) -> Task<Message> {
    match e {
        Effect::None => Task::none(),
        Effect::Check(n, password) => {
            let user = app.user.clone();
            let preview = app.preview;
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || check(&user, password, preview))
                        .await
                        .unwrap_or_else(|_| pam::Verdict::Failed("The check stopped".into()))
                },
                move |v| Message::Verdict(n, v),
            )
        }
        Effect::Unlock(proof) => unlock(proof),
    }
}

#[cfg(feature = "devtools")]
fn check(user: &str, password: secret::Secret, preview: bool) -> pam::Verdict {
    if preview {
        pam::preview(password)
    } else {
        pam::authenticate(user, password)
    }
}

#[cfg(not(feature = "devtools"))]
fn check(user: &str, password: secret::Secret, _preview: bool) -> pam::Verdict {
    pam::authenticate(user, password)
}

/// The only way to unlock: it takes PAM's proof. Unlocks, then ends the process (the loop
/// flushes the unlock before it stops).
fn unlock(_proof: pam::Accepted) -> Task<Message> {
    log::info!("unlocked");
    Task::batch([iced::unlock_session(), iced_runtime::exit().into()])
}

pub fn update(app: &mut App, m: Message) -> Task<Message> {
    match m {
        Message::Session(LockEvent::Locked) => {
            log::info!("session locked");
            let e = app.lock.locked();
            effect(app, e)
        }
        Message::Session(LockEvent::Finished) => {
            if app.lock.session() == Session::Asked {
                log::error!("the compositor refused the lock (another lock screen holds it)");
                REFUSED.store(true, Ordering::Relaxed);
            } else {
                log::warn!("the compositor ended the lock");
            }
            iced_runtime::exit().into()
        }
        Message::Output(OutputEvent::Added(info)) => {
            if app.surfaces.contains_key(&info.id) {
                return Task::none();
            }
            let (id, task) = iced::new_lock_surface(info.id);
            app.surfaces.insert(info.id, id);
            task
        }
        Message::Output(OutputEvent::Removed(output)) => match app.surfaces.remove(&output) {
            Some(id) => iced::destroy_layer_surface(id),
            None => Task::none(),
        },
        Message::Output(_) => Task::none(),
        Message::Key(input) => {
            app.lock.caps_lock = iced::caps_lock();
            let e = app.lock.key(input);
            effect(app, e)
        }
        Message::Modifiers => {
            app.lock.caps_lock = iced::caps_lock();
            Task::none()
        }
        Message::Verdict(n, v) => {
            if matches!(v, pam::Verdict::Failed(_)) {
                log::warn!("password check failed: {v:?}");
            }
            let e = app.lock.verdict(n, v);
            effect(app, e)
        }
        Message::Wallpaper(w) => {
            app.wallpaper = w;
            Task::none()
        }
        Message::Tick => {
            app.now = chrono::Local::now();
            Task::none()
        }
    }
}

/// What a lock surface shows, as a number: a surface is redrawn only when it changes (the
/// clock's tick every second redraws once a minute).
pub fn redraw_key(app: &App, _id: SurfaceId) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    app.now.format("%Y%m%d%H%M").to_string().hash(&mut h);
    app.lock.input.chars().min(MAX_DOTS + 1).hash(&mut h);
    app.lock.status.hash(&mut h);
    app.lock.caps_lock.hash(&mut h);
    app.wallpaper.is_some().hash(&mut h);
    let k = h.finish();
    // Never the "not ready" key: that would hold the surface back.
    Some(if k == iced::NOT_READY { 0 } else { k })
}

fn time_text(app: &App) -> String {
    if app.h12 {
        app.now.format("%-I:%M").to_string()
    } else {
        app.now.format("%H:%M").to_string()
    }
}

/// The status line under the field, in its colour. Empty keeps its height (nothing moves).
fn status_line(app: &App) -> (String, iced::Color) {
    let s = app.scheme;
    match &app.lock.status {
        Status::None => (String::new(), s.ink_dim),
        Status::Checking => ("Checking…".into(), s.ink_dim),
        Status::Wrong(None) => ("Wrong password".into(), s.bad),
        Status::Wrong(Some(why)) => (format!("Wrong password. {why}"), s.bad),
        Status::Failed(why) => (format!("Could not check the password: {why}"), s.bad),
    }
}

fn field(app: &App) -> Element<'_, Message> {
    let s = app.scheme;
    let n = app.lock.input.chars();
    let checking = app.lock.is_checking();
    let content: Element<'_, Message> = if n == 0 && !checking {
        label("Password", text::BODY, s.ink_faint).into()
    } else {
        let dot = if checking { s.ink_faint } else { s.ink };
        let dots = (0..n.min(MAX_DOTS)).map(|_| {
            container(Space::new())
                .width(tok::M)
                .height(tok::M)
                .style(move |_| container::Style {
                    background: Some(dot.into()),
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                })
                .into()
        });
        row(dots).spacing(tok::S).align_y(Alignment::Center).into()
    };
    let wrong = matches!(app.lock.status, Status::Wrong(_) | Status::Failed(_));
    let edge = if wrong { s.bad } else { s.line };
    container(
        row![
            crate::ui::icon(Icon::Lock, tok::ICON, s.ink_dim),
            container(content)
                .width(Length::Fill)
                .align_x(Alignment::Center)
        ]
        .spacing(tok::M)
        .align_y(Alignment::Center),
    )
    .width(tok::PANEL_S)
    .height(tok::H_ROW)
    .padding([tok::NONE, tok::L])
    .align_y(Alignment::Center)
    .style(move |_| container::Style {
        background: Some(s.raised.into()),
        border: iced::Border::default()
            .rounded(tok::FULL)
            .width(tok::HAIR)
            .color(edge),
        ..container::Style::default()
    })
    .into()
}

pub fn view(app: &App, id: SurfaceId) -> Element<'_, Message> {
    if !app.surfaces.values().any(|s| *s == id) {
        return Space::new().into();
    }
    let s = app.scheme;
    let background: Element<'_, Message> = match &app.wallpaper {
        Some(h) => iced::widget::image(h.clone())
            .content_fit(ContentFit::Cover)
            .filter_method(iced::widget::image::FilterMethod::Linear)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        None => Space::new().width(Length::Fill).height(Length::Fill).into(),
    };
    let base = s.base;
    let scrim_alpha = if app.wallpaper.is_some() { SCRIM } else { 1.0 };
    let scrim = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(base.scale_alpha(scrim_alpha).into()),
            ..container::Style::default()
        });
    let (status, status_color) = status_line(app);
    let caps = if app.lock.caps_lock {
        "Caps Lock is on"
    } else {
        ""
    };
    let content = column![
        label_w(time_text(app), text::HERO, s.ink, theme::SEMIBOLD),
        label(
            app.now.format("%A %-d %B").to_string(),
            text::HEADING,
            s.ink_dim
        ),
        Space::new().height(tok::XXXL),
        label(app.shown_name.as_str(), text::TITLE, s.ink_dim),
        field(app),
        container(label(status, text::SMALL, status_color)).height(tok::H_SMALL),
        container(label(caps, text::SMALL, s.warn)).height(tok::H_SMALL),
    ]
    .spacing(tok::M)
    .align_x(Alignment::Center);
    stack![
        background,
        scrim,
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    ]
    .into()
}

/// A key as the lock screen understands it (`None`: not for it).
fn key_input(e: iced::event::Event) -> Option<Message> {
    use iced::keyboard::{Event, Key, key::Named};
    match e {
        iced::event::Event::Keyboard(Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        }) => {
            let input = match key {
                Key::Named(Named::Enter) => Input::Submit,
                Key::Named(Named::Escape) => Input::Clear,
                Key::Named(Named::Backspace) => Input::Backspace,
                // Ctrl+U clears, as in swaylock and a terminal.
                Key::Character(c) if modifiers.control() && c.as_str() == "u" => Input::Clear,
                _ if modifiers.control() || modifiers.alt() || modifiers.logo() => return None,
                _ => Input::Text(state::Chars::new(text.as_deref()?)?),
            };
            Some(Message::Key(input))
        }
        iced::event::Event::Keyboard(Event::ModifiersChanged(_)) => Some(Message::Modifiers),
        _ => None,
    }
}

pub fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::session_lock_events().map(Message::Session),
        iced::output_events().map(Message::Output),
        iced::event::listen_with(|e, _status, _id| key_input(e)),
        iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::Tick),
    ])
}

pub fn theme(_app: &App, _id: SurfaceId) -> Theme {
    Theme::Dark
}

/// Runs the lock screen until the password is accepted. An error when it could not lock.
pub fn run() -> anyhow::Result<()> {
    // One lock screen per display: a second `kestrel lock` (hypridle's lock_cmd again, the
    // power menu) ends at once. Held until this process ends, however it ends.
    let Some(_instance) = single_instance() else {
        log::info!("a lock screen is already running");
        return Ok(());
    };
    // Who to check, before anything is locked: without it the screen could never unlock.
    let Some((user, full)) = pam::current_user() else {
        anyhow::bail!("kestrel lock: cannot tell which user this is; not locking");
    };
    let shown = full.unwrap_or_else(|| user.clone());
    iced::application(move || boot(user, shown), update, view)
        .subscription(subscription)
        .theme(theme)
        .redraw_key(redraw_key)
        .default_font(crate::theme::FONT)
        .font(crate::ui::icon::BYTES)
        .font(crate::theme::RUBIK_MEDIUM)
        .font(crate::theme::RUBIK_SEMIBOLD)
        .font(crate::theme::RUBIK_BOLD)
        .antialiasing(true)
        // A 1 px placeholder, as the prompt process has; the lock surfaces are made per output.
        .layer_shell(LayerShellSettings {
            layer: Layer::Background,
            size: Some((1, 1)),
            anchor: Anchor::TOP | Anchor::LEFT,
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: "kestrel-lock-root".into(),
            ..LayerShellSettings::default()
        })
        .run()?;
    if REFUSED.load(Ordering::Relaxed) {
        anyhow::bail!("kestrel lock: the compositor refused the lock (is the session locked?)");
    }
    Ok(())
}

/// An exclusive `flock` on `$XDG_RUNTIME_DIR/kestrel-lock-<display>.lock`; `None` when another
/// lock screen holds it. The kernel lets it go when the process ends (a crash too), so a
/// stale file never blocks the next lock. Without a runtime directory there is no guard (the
/// compositor still refuses a second lock while one is held).
fn single_instance() -> Option<Option<std::fs::File>> {
    use std::os::fd::AsRawFd;
    let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return Some(None);
    };
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let name = format!("kestrel-lock-{}.lock", display.replace('/', "_"));
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::path::Path::new(&dir).join(name))
    else {
        return Some(None);
    };
    // Safety: a valid descriptor we own.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Some(Some(file));
    }
    // Only "held by another" means another lock screen; any other failure must not stop the
    // screen from locking.
    let held = std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK);
    if held { None } else { Some(None) }
}
