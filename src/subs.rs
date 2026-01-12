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
pub fn nba() -> Subscription<Vec<web::Game>> {
    Subscription::run(web::nba_stream)
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
