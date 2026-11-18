//! kestrel: a native desktop shell for Hyprland.

mod app;
mod apptheme;
mod config;
mod disk;
mod guard;
mod history;
mod hypr_settings;
mod ipc;
mod lock;
mod look;
mod lua;
#[cfg(feature = "memtrace")]
mod memtrace;
mod modules;
mod motion;
mod panels;
mod prompt;
mod screens;
mod subs;
// The system-integration crate, under its short name.
pub(crate) use kestrel_services as services;
mod shell;
mod state;
mod theme;
mod ui;
mod watchdog;

use iced::{KeyboardInteractivity, Layer, LayerShellSettings};

fn main() -> anyhow::Result<()> {
    // TLS (weather, updates) through ring, installed once before any request.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Started by the browser for the "kestrel media" extension (it passes its origin).
    if args
        .first()
        .is_some_and(|a| a.starts_with("chrome-extension://"))
    {
        services::browser_host::run();
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "prompt") {
        if std::env::var_os("RUST_LOG").is_none() {
            // Safety: single-threaded at this point.
            unsafe { std::env::set_var("RUST_LOG", "kestrel=info,warn") };
        }
        env_logger();
        return prompt::run();
    }
    // The lock screen: its own process (src/lock), started by hypridle's lock_cmd.
    if args.first().is_some_and(|a| a == "lock") {
        if std::env::var_os("RUST_LOG").is_none() {
            // Safety: single-threaded at this point.
            unsafe { std::env::set_var("RUST_LOG", "kestrel=info,warn") };
        }
        env_logger();
        return lock::run();
    }
    if !args.is_empty() {
        if args[0] == "--help" || args[0] == "-h" {
            println!("kestrel                 run the shell");
            println!(
                "kestrel toggle <panel>  launcher | control | media | calendar | notifications | power"
            );
            println!("kestrel close           close the open panel");
            println!("kestrel lock            lock the session (the lock screen, docs/LOCK.md)");
            println!(
                "kestrel health          what each service is doing (running, restarting, failed)"
            );
            println!("kestrel focus bar       move along the bar's buttons with the keys");
            return Ok(());
        }
        if !ipc::send(&args.join(" ")) {
            eprintln!("kestrel is not running");
            std::process::exit(1);
        }
        return Ok(());
    }
    if std::env::var_os("RUST_LOG").is_none() {
        // Safety: single-threaded at this point.
        unsafe { std::env::set_var("RUST_LOG", "kestrel=info,warn") };
    }
    env_logger();
    iced::application(app::boot, app::update, app::view)
        .subscription(app::subscription)
        .theme(app::theme)
        .redraw_key(app::redraw_key)
        .clipboard(Clipboard)
        .default_font(theme::FONT)
        .font(ui::icon::BYTES)
        // Rubik's fixed weights: the installed Rubik is a variable font, and the text renderer
        // draws its default (Light) instance for every weight, so Medium came out thin.
        .font(theme::RUBIK_MEDIUM)
        .font(theme::RUBIK_SEMIBOLD)
        .font(theme::RUBIK_BOLD)
        .antialiasing(true)
        // The first surface is a 1 px placeholder; real surfaces are made per monitor.
        .layer_shell(LayerShellSettings {
            layer: Layer::Background,
            size: Some((1, 1)),
            anchor: iced::Anchor::TOP | iced::Anchor::LEFT,
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: "kestrel-root".into(),
            ..LayerShellSettings::default()
        })
        .run()?;
    Ok(())
}

/// Text fields copy and paste through kestrel's one clipboard connection
/// (services::clipboard), not a second one of iced's own (smithay-clipboard: another Wayland
/// connection and thread, woken by every key and pointer event). Only the regular clipboard:
/// iced's text fields never use the primary selection.
struct Clipboard;

impl iced::core::clipboard::Clipboard for Clipboard {
    fn read(&self, kind: iced::core::clipboard::Kind) -> Option<String> {
        match kind {
            iced::core::clipboard::Kind::Standard => services::clipboard::paste_text(),
            iced::core::clipboard::Kind::Primary => None,
        }
    }

    fn write(&mut self, kind: iced::core::clipboard::Kind, contents: String) {
        if kind == iced::core::clipboard::Kind::Standard {
            drop(services::clipboard::copy(
                services::clipboard::TEXT_TYPES,
                contents.into_bytes(),
            ));
        }
    }
}

/// Minimal stderr logger honouring RUST_LOG's level for kestrel.
fn env_logger() {
    struct L(log::LevelFilter);
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= self.0
        }
        fn log(&self, r: &log::Record) {
            // kestrel's own messages at the chosen level; everyone else's warnings and errors.
            // Also which renderer was chosen (GPU or CPU, API, alpha mode): the first thing to
            // know when something draws wrong.
            let ours = self.enabled(r.metadata())
                && (r.target().starts_with("kestrel") || r.target().starts_with("iced_wgpu"));
            // The SVG parser warns about attributes it skips (`marker-start="none"` in icon
            // files) while drawing them fine: only its errors are worth a line.
            let noise = r.target().starts_with("usvg") && r.level() > log::Level::Error;
            if !noise && (ours || r.level() <= log::Level::Warn) {
                eprintln!("{} {}: {}", r.level(), r.target(), r.args());
            }
        }
        fn flush(&self) {}
    }
    let level = if std::env::var("RUST_LOG").is_ok_and(|v| v.contains("debug")) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let _ = log::set_boxed_logger(Box::new(L(level)));
    log::set_max_level(level);
}
