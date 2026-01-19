//! Service streams as iced subscriptions. Each subscription exists only while the app asks for
//! it, so a module that is switched off never starts its service.

use crate::services::{
    audio, bluetooth, clock, hyprland, mpris, network, notifications, power, system, tray, updates,
    vpn, web,
};
use iced::Subscription;

pub fn hyprland() -> Subscription<hyprland::State> {
    Subscription::run(hyprland::stream)
}
pub fn clock() -> Subscription<()> {
    Subscription::run(clock::stream)
}
pub fn audio() -> Subscription<audio::State> {
    Subscription::run(audio::stream)
}
pub fn network() -> Subscription<network::State> {
    Subscription::run(network::stream)
}
pub fn bluetooth() -> Subscription<bluetooth::State> {
    Subscription::run(bluetooth::stream)
}
pub fn power() -> Subscription<power::State> {
    Subscription::run(power::stream)
}
pub fn mpris() -> Subscription<mpris::State> {
    Subscription::run(mpris::stream)
}
pub fn copied() -> Subscription<crate::services::clipboard::Copied> {
    Subscription::run(crate::services::clipboard::stream)
}
pub fn tray() -> Subscription<tray::State> {
    Subscription::run(tray::stream)
}
pub fn notifications() -> Subscription<notifications::Event> {
    Subscription::run(notifications::stream)
}
pub fn vpn() -> Subscription<vpn::State> {
    Subscription::run(vpn::stream)
}
pub fn system() -> Subscription<system::State> {
    Subscription::run(system::stream)
}
pub fn updates() -> Subscription<updates::State> {
    Subscription::run(updates::stream)
}

/// Weather for a city, or exact coordinates with the city as the label.
pub fn weather(
    city: Option<&str>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Subscription<web::Weather> {
    let coords = lat
        .zip(lon)
        .map(|(a, b)| ((a * 1e6) as i64, (b * 1e6) as i64));
    match city {
        Some(c) if !c.trim().is_empty() || coords.is_some() => Subscription::run_with(
            web::Place {
                name: c.to_string(),
                coords,
            },
            web::weather_stream,
        ),
        _ => Subscription::none(),
    }
}

/// Which file in `~/.config/kestrel/` changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigFile {
    Toml,
    Lua,
}

/// Changes to config.toml and init.lua, so edits apply without a restart. The directory is
/// watched (not the files) because editors save by replacing the file.
pub fn config_files() -> Subscription<ConfigFile> {
    Subscription::run(config_stream)
}

fn config_stream() -> impl futures::Stream<Item = ConfigFile> {
    use futures::{SinkExt, StreamExt};
    crate::services::stream::channel(4, async |mut out| {
        let dir = crate::config::path()
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let _ = std::fs::create_dir_all(&dir);
        let Ok(ino) = inotify::Inotify::init() else {
            return;
        };
        let mask = inotify::WatchMask::CLOSE_WRITE
            | inotify::WatchMask::MOVED_TO
            | inotify::WatchMask::CREATE
            | inotify::WatchMask::DELETE;
        if let Err(e) = ino.watches().add(&dir, mask) {
            log::warn!("config: watching {}: {e}", dir.display());
            return;
        }
        let Ok(mut events) = ino.into_event_stream([0u8; 4096]) else {
            return;
        };
        while let Some(Ok(e)) = events.next().await {
            let file = match e.name.as_deref().and_then(|n| n.to_str()) {
                Some("config.toml") => ConfigFile::Toml,
                Some("init.lua") => ConfigFile::Lua,
                _ => continue,
            };
            // An editor's save is several events; let it finish, then report once.
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            if out.send(file).await.is_err() {
                return;
            }
        }
    })
}

pub fn lua() -> Subscription<crate::lua::Event> {
    Subscription::run(crate::lua::stream)
}
