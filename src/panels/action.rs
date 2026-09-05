//! What a panel asks the shell to do: the answer of a panel's `update`, as iced's guide has a
//! screen return an `Action` to its parent (docs.rs/iced, "Pocket Guide"). A panel changes only
//! its own state; everything beyond it is one of these, carried out by the shell in `perform`,
//! the one place it happens. The list is the whole contract: a panel cannot do anything else.

use crate::app::Message as AppMessage;
use crate::services::compositor;
use iced::Task;

pub enum Action<M> {
    None,
    /// Work for the panel itself (its answers come back as its own messages).
    Run(Task<M>),
    /// Close the open panel.
    Close,
    /// Ask the compositor (focus a window, start a program, open a file).
    Dispatch(compositor::Action),
    /// Connect (true) or disconnect a Bluetooth device, shown in progress until it is so.
    Device {
        path: String,
        connect: bool,
    },
    /// Show a panel on an output, hanging from `at` (a panel that waited for what it shows).
    Open {
        kind: super::PanelKind,
        output: iced::OutputId,
        at: iced::Rectangle,
    },
    /// Open Settings on a page.
    Settings(super::settings::Page),
    /// Open Settings › Windows on the rule for an app (its window class).
    WindowRule(String),
    /// Read Hyprland's state again now (windows moved since the last event).
    ReadHyprland,
    /// Put text on the clipboard and paste it into the window that had focus (an emoji).
    Type(String),
    /// Put a clipboard history entry (by its key) back on the clipboard; with `paste`, also
    /// paste it into the window that had focus once the compositor has it.
    PasteClip {
        key: String,
        paste: bool,
    },
    /// Make this picture the wallpaper (saved in config.toml, shown at once).
    SetWallpaper(std::path::PathBuf),
    /// The folder the wallpaper picker shows (saved in config.toml), and show it again.
    WallpaperFolder(std::path::PathBuf),
    /// Ask the file picker (kestrel's own question); the choice comes back to the panel as
    /// `reply(path)`.
    AskFile {
        request: crate::services::filechooser::Request,
        reply: fn(std::path::PathBuf) -> M,
    },
    /// Say something in the island's OSD (a countdown's number).
    Notice(crate::ui::Icon, String),
    /// Take the OSD's message away.
    HideNotice,
    /// Deliver the answer to a question the shell itself asked (built by the asker: the panel
    /// only hands it back, e.g. the file kestrel's Settings asked the file picker for).
    Reply(Box<AppMessage>),
    /// Several, in order.
    Many(Vec<Action<M>>),
}

impl<M> Action<M> {
    pub fn and(self, next: Action<M>) -> Action<M> {
        match self {
            Action::None => next,
            Action::Many(mut v) => {
                v.push(next);
                Action::Many(v)
            }
            a => Action::Many(vec![a, next]),
        }
    }
}

/// Carries out a panel's action. `wrap` turns the panel's own messages into the shell's.
pub fn perform<M: Send + 'static>(
    app: &mut crate::app::App,
    action: Action<M>,
    wrap: fn(M) -> AppMessage,
) -> Task<AppMessage> {
    match action {
        Action::None => Task::none(),
        Action::Run(t) => t.map(wrap),
        Action::Close => Task::done(AppMessage::Close),
        Action::Dispatch(a) => Task::done(AppMessage::Dispatch(a)),
        Action::Device { path, connect } => {
            use crate::services::bluetooth;
            app.connecting = Some(super::cx::Connecting::new(path.clone(), connect));
            app.device_error = None;
            let cmd = if connect {
                bluetooth::Command::Connect(path.clone())
            } else {
                bluetooth::Command::Disconnect(path.clone())
            };
            Task::perform(bluetooth::command(cmd), move |r| {
                AppMessage::DeviceDone(path.clone(), r)
            })
        }
        Action::Open { kind, output, at } => app.open_panel(kind, output, at),
        Action::Settings(page) => Task::done(AppMessage::SettingsAt(page)),
        Action::WindowRule(class) => Task::done(AppMessage::RuleFor(class)),
        Action::ReadHyprland => Task::perform(crate::services::hyprland::snapshot(), |s| match s {
            Some(s) => AppMessage::Hypr(s),
            None => AppMessage::Batch(Vec::new()),
        }),
        Action::Type(text) => {
            let (address, class) = (
                app.hypr.window_address.clone(),
                app.hypr.window_class.clone(),
            );
            Task::future(super::clipboard::type_into(text, address, class)).discard()
        }
        Action::PasteClip { key, paste } => {
            let (address, class) = (
                app.hypr.window_address.clone(),
                app.hypr.window_class.clone(),
            );
            Task::future(async move {
                super::clipboard::clip_copy(key).await;
                if paste && !class.is_empty() {
                    super::clipboard::paste_into(&address, &class).await;
                }
            })
            .discard()
        }
        Action::Reply(m) => Task::done(*m),
        Action::SetWallpaper(p) => {
            let s = p.to_string_lossy().into_owned();
            crate::config::set_wallpaper(&s);
            app.config.wallpaper = Some(s);
            Task::done(AppMessage::ReloadWallpaper)
        }
        Action::WallpaperFolder(p) => {
            let h = std::env::var("HOME").unwrap_or_default();
            let shown = match p.strip_prefix(&h) {
                Ok(rest) => format!("~/{}", rest.display()),
                Err(_) => p.display().to_string(),
            };
            crate::config::set(&[], "wallpaper_dir", toml_edit::value(shown.clone()));
            app.config.wallpaper_dir = Some(shown);
            Task::done(AppMessage::Ipc("toggle wallpaper".into()))
        }
        Action::AskFile { request, reply } => {
            app.panels.files.then = Some(Box::new(move |p| wrap(reply(p))));
            Task::done(AppMessage::Close).chain(Task::done(AppMessage::FileRequest(request)))
        }
        Action::Notice(icon, title) => {
            app.show_event(icon, title, String::new());
            Task::none()
        }
        Action::HideNotice => {
            app.hide_osd();
            Task::none()
        }
        Action::Many(v) => Task::batch(v.into_iter().map(|a| perform(app, a, wrap))),
    }
}
