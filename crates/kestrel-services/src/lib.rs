//! kestrel-services: everything kestrel reads from or tells the system, with no UI code.
//!
//! Each service talks to its source directly (sockets, D-Bus, sysfs) and exposes a `stream()` of
//! full state snapshots plus async commands. None spawns helper processes for data, except where
//! a module documents it (clipboard history, update checks, night light), and only on demand.

pub mod apps;
pub mod audio;
pub mod bluetooth;
pub mod clipboard;
pub mod clock;
pub mod dbus;
pub mod hyprland;
pub mod icons;
pub mod logind;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod power;
pub mod previews;
pub mod stream;
pub mod system;
pub mod toggles;
pub mod tray;
pub mod updates;
pub mod vpn;
pub mod web;
