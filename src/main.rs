//! kestrel: a native desktop shell for Hyprland.

mod app;
mod apptheme;
mod config;
mod guard;
mod history;
mod hypr_settings;
mod ipc;
mod look;
mod lua;
#[cfg(feature = "memtrace")]
mod memtrace;
mod modules;
mod motion;
mod panels;
mod prompt;
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
    if !args.is_empty() {
        if args[0] == "--help" || args[0] == "-h" {
            println!("kestrel                 run the shell");
            println!(
                "kestrel toggle <panel>  launcher | control | media | calendar | notifications | power"
            );
            println!("kestrel close           close the open panel");
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

/// Minimal stderr logger honouring RUST_LOG's level for kestrel.
fn env_logger() {
    struct L(log::LevelFilter);
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= self.0
        }
        fn log(&self, r: &log::Record) {
            // kestrel's own messages at the chosen level; everyone else's warnings and errors.
            let ours = self.enabled(r.metadata()) && r.target().starts_with("kestrel");
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
