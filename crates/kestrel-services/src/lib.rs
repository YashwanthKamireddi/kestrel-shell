//! kestrel-services: everything kestrel reads from or tells the system, with no UI code.
//!
//! Each service talks to its source directly (sockets, D-Bus, sysfs) and exposes a `stream()` of
//! full state snapshots plus async commands. None spawns helper processes for data, except where
//! a module documents it (clipboard history, update checks, night light), and only on demand.

pub mod about;
pub mod airpods;
pub mod apps;
pub mod audio;
pub mod autostart;
pub mod bluetooth;
pub mod browser_host;
pub mod camera;
pub mod clipboard;
pub mod clock;
pub mod compositor;
pub mod dbus;
pub mod ddc;
pub mod defaults;
pub mod disks;
pub mod displays;
pub mod filechooser;
pub mod gsettings;
pub mod hyprland;
pub mod icons;
pub mod idle;
pub mod level;
pub mod logind;
pub mod meters;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod open;
pub mod pipewire;
pub mod polkit;
pub mod power;
pub mod previews;
pub mod region;
pub mod sounds;
pub mod stream;
pub mod system;
pub mod timedate;
pub mod toggles;
pub mod tray;
pub mod updates;
pub mod vpn;
pub mod web;
pub mod xdg;
