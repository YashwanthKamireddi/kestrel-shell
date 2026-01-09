//! kestrel: a native desktop shell for Hyprland.

mod app;
mod apptheme;
mod config;
mod ipc;
mod motion;
mod panels;
mod services;
mod shell;
mod theme;
mod ui;

use iced::{KeyboardInteractivity, Layer, LayerShellSettings};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if args[0] == "--help" || args[0] == "-h" {
            println!("kestrel                 run the shell");
            println!(
                "kestrel toggle <panel>  launcher | control | media | calendar | notifications | power"
            );
            println!("kestrel close           close the open panel");
            return Ok(());
        }
        if args[0] == "statusline" {
            claude_statusline();
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
            if self.enabled(r.metadata()) && r.target().starts_with("kestrel") {
                eprintln!("{} {}: {}", r.level(), r.target(), r.args());
            } else if r.level() <= log::Level::Warn {
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

/// `kestrel statusline`: Claude Code's status line command. It prints the status text and
/// keeps the plan usage Claude Code passes in (`rate_limits`) where the bar can show it.
fn claude_statusline() {
    use std::io::Read;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    if let Some(limits) = v.get("rate_limits") {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("kestrel");
        let _ = std::fs::create_dir_all(&dir);
        let record = serde_json::json!({ "rate_limits": limits });
        let tmp = dir.join(".claude-usage.json.tmp");
        if std::fs::write(&tmp, record.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("claude-usage.json"));
        }
    }
    let cwd = v
        .pointer("/workspace/current_dir")
        .and_then(|d| d.as_str())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    print!(
        "[{}@{} {}]",
        user,
        host.trim().split('.').next().unwrap_or(""),
        cwd.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    );
}
