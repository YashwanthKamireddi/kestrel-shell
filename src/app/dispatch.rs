//! Every message to the shell, routed to the part that handles it.

use super::*;

mod agents;
mod config;
mod outputs;
mod panels;
mod services;

pub(crate) fn update_inner(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        m @ (Message::Audio { .. }
        | Message::Net { .. }
        | Message::Bt { .. }
        | Message::Pods { .. }
        | Message::Power { .. }
        | Message::Media { .. }
        | Message::Camera { .. }
        | Message::Ddc { .. }
        | Message::DdcLevel { .. }
        | Message::ArtSettled { .. }
        | Message::Tray { .. }
        | Message::Weather { .. }
        | Message::Updates { .. }
        | Message::System { .. }
        | Message::Locks { .. }
        | Message::Vpn { .. }
        | Message::Meters { .. }
        | Message::Copied { .. }
        | Message::Hypr { .. }
        | Message::Notification { .. }
        | Message::VolumeStep { .. }
        | Message::Level { .. }
        | Message::VisTick { .. }) => services::handle(app, m),
        m @ (Message::Polkit { .. }
        | Message::WifiAsk { .. }
        | Message::BtAsk { .. }
        | Message::KeyringAsk { .. }
        | Message::OpenWith { .. }
        | Message::FileRequest { .. }
        | Message::WindowClosed { .. }) => agents::handle(app, m),
        m @ (Message::Output { .. }
        | Message::Second
        | Message::Minute
        | Message::Frame { .. }
        | Message::WallpaperLoaded { .. }
        | Message::Trim
        | Message::Measured { .. }
        | Message::MeasuredToasts { .. }
        | Message::MeasuredOsd { .. }
        | Message::IslandMeasured { .. }
        | Message::Breathe
        | Message::Expire { .. }
        | Message::ReloadWallpaper) => outputs::handle(app, m),
        m @ (Message::Toggle { .. }
        | Message::ControlAt { .. }
        | Message::SettingsAt { .. }
        | Message::RuleFor { .. }
        | Message::Close
        | Message::Escape { .. }
        | Message::TrayExpand
        | Message::TrayMenu { .. }
        | Message::Panel { .. }
        | Message::Ipc { .. }
        | Message::Dispatch { .. }
        | Message::DeviceDone { .. }
        | Message::Change { .. }
        | Message::NetDone { .. }
        | Message::StopRecording
        | Message::Batch { .. }) => panels::handle(app, m),
        Message::Alive => {
            crate::watchdog::notify("WATCHDOG=1");
            Task::none()
        }
        m @ (Message::ConfigFile { .. } | Message::Lua { .. } | Message::LuaClick { .. }) => {
            config::handle(app, m)
        }
    }
}
