//! Bar modules. Every item in the bar is a module: a file in this folder with one `view`, and
//! one entry in `MODULES` (its id as in config.toml, its name and icon in Settings, where it
//! goes by default, its options). `[bar] left/center/right` lists which modules show and in
//! what order; a module not listed is not drawn and the services only it needs never start.
//! The rules every module follows are in docs/BAR.md.

pub mod airpods;
pub mod app_name;
pub mod battery;
pub mod bluetooth;
pub mod clock;
pub mod island;
mod lua;
pub mod media;
pub mod network;
pub mod notifications;
pub mod power;
pub mod privacy;
pub mod system;
pub mod tray;
pub mod updates;
pub mod volume;
pub mod weather;
pub mod workspaces;

pub use tray::tray_icon_at;

use crate::app::{App, Message, Output};
use crate::config::BarConfig;
use crate::panels::PanelKind;
use crate::theme::{Scheme, size};
use crate::ui::{self, Icon, icon};
use iced::Element;
use iced::widget::container;

/// Where a module goes when it is added to the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Home {
    Left,
    Center,
    Right,
}

/// What a module draws: its bar items for output `o` (none when it has nothing to show now).
pub type View =
    for<'a> fn(&'a App, &'a Output, Scheme, Option<PanelKind>) -> Vec<Element<'a, Message>>;

/// A module: its id (as in config.toml), its name and icon in Settings, where it goes by
/// default, its own options, and its view. Settings and the bar are both built from this list.
pub struct Info {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: Icon,
    pub home: Home,
    pub options: &'static [Opt],
    pub view: View,
}

/// One option of a module, as it is kept in config.toml.
pub struct Opt {
    /// Its table (`["bar"]`, `["clock"]`, `[]` for the top level) and key.
    pub table: &'static [&'static str],
    pub key: &'static str,
    pub label: &'static str,
    pub kind: OptKind,
}

pub enum OptKind {
    /// On or off, with its default.
    Switch(bool),
    /// One of some values: (value written, label shown); the first is the default.
    Choice(&'static [(&'static str, &'static str)]),
}

impl Opt {
    /// Its value in the parsed config file, else its default.
    pub fn value(&self, doc: &toml::Table) -> toml::Value {
        let mut t = doc;
        for name in self.table {
            match t.get(*name).and_then(toml::Value::as_table) {
                Some(next) => t = next,
                None => return self.default(),
            }
        }
        t.get(self.key).cloned().unwrap_or_else(|| self.default())
    }

    fn default(&self) -> toml::Value {
        match self.kind {
            OptKind::Switch(on) => toml::Value::Boolean(on),
            OptKind::Choice(c) => toml::Value::String(c[0].0.to_string()),
        }
    }
}

const BATTERY_PERCENT: Opt = Opt {
    table: &["bar"],
    key: "battery_percent",
    label: "Percentage",
    kind: OptKind::Switch(true),
};
const CLOCK_FORMAT: Opt = Opt {
    table: &["clock"],
    key: "format",
    label: "Clock",
    kind: OptKind::Choice(&[("24h", "24-hour"), ("12h", "12-hour")]),
};
const CLOCK_DATE: Opt = Opt {
    table: &["clock"],
    key: "date",
    label: "Date",
    kind: OptKind::Switch(true),
};
const CLOCK_SECONDS: Opt = Opt {
    table: &["clock"],
    key: "seconds",
    label: "Seconds",
    kind: OptKind::Switch(false),
};
const VISUALISER: Opt = Opt {
    table: &["bar"],
    key: "visualiser",
    label: "Music visualiser",
    kind: OptKind::Switch(true),
};
const UNITS: Opt = Opt {
    table: &[],
    key: "weather_units",
    label: "Temperature",
    kind: OptKind::Choice(&[("celsius", "°C"), ("fahrenheit", "°F")]),
};

/// Every built-in module, in the order Settings lists them.
#[rustfmt::skip]
pub const MODULES: &[Info] = &[
    Info { id: "workspaces", name: "Workspaces", icon: Icon::LayoutGrid, home: Home::Left, options: &[], view: workspaces::view },
    Info { id: "app", name: "Focused app", icon: Icon::AppWindow, home: Home::Left, options: &[], view: app_name::view },
    Info { id: "island", name: "Time and island", icon: Icon::CalendarClock, home: Home::Center, options: &[CLOCK_FORMAT, CLOCK_DATE, CLOCK_SECONDS, VISUALISER], view: island::view },
    Info { id: "clock", name: "Clock", icon: Icon::CalendarClock, home: Home::Center, options: &[CLOCK_FORMAT, CLOCK_DATE, CLOCK_SECONDS], view: clock::view },
    Info { id: "media", name: "Music", icon: Icon::Music, home: Home::Center, options: &[], view: media::view },
    Info { id: "weather", name: "Weather", icon: Icon::CloudSun, home: Home::Center, options: &[UNITS], view: weather::view },
    Info { id: "airpods", name: "AirPods", icon: Icon::Headphones, home: Home::Center, options: &[], view: airpods::view },
    Info { id: "tray", name: "Tray icons", icon: Icon::Package, home: Home::Right, options: &[], view: tray::view },
    Info { id: "system", name: "CPU and memory", icon: Icon::Cpu, home: Home::Right, options: &[], view: system::view },
    Info { id: "updates", name: "Updates", icon: Icon::Cube, home: Home::Right, options: &[], view: updates::view },
    Info { id: "privacy", name: "Microphone and screen in use", icon: Icon::Mic, home: Home::Right, options: &[], view: privacy::view },
    Info { id: "network", name: "Network", icon: Icon::Wifi, home: Home::Right, options: &[], view: network::view },
    Info { id: "bluetooth", name: "Bluetooth", icon: Icon::Bluetooth, home: Home::Right, options: &[], view: bluetooth::view },
    Info { id: "volume", name: "Sound", icon: Icon::Volume2, home: Home::Right, options: &[], view: volume::view },
    Info { id: "battery", name: "Battery", icon: Icon::Battery, home: Home::Right, options: &[BATTERY_PERCENT], view: battery::view },
    Info { id: "notifications", name: "Notifications", icon: Icon::Bell, home: Home::Right, options: &[], view: notifications::view },
    Info { id: "power", name: "Power", icon: Icon::Power, home: Home::Right, options: &[], view: power::view },
];

/// The old combined status button, read as the separate buttons it held.
pub const STATUS: [&str; 4] = ["network", "bluetooth", "volume", "battery"];

/// The module `id`, if built in.
pub fn info(id: &str) -> Option<&'static Info> {
    MODULES.iter().find(|m| m.id == id)
}

/// Whether `id` is a built-in module.
pub fn known(id: &str) -> bool {
    info(id).is_some()
}

/// Which optional services the configured layout needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub media: bool,
    pub tray: bool,
    pub network: bool,
    pub system: bool,
    pub updates: bool,
    pub weather: bool,
    pub airpods: bool,
}

pub fn needs(bar: &BarConfig) -> Needs {
    let on = |id: &str| bar.contains(id);
    // The island shows what is live (music, games) and opens the dashboard (weather, system).
    let island = on("island");
    Needs {
        media: on("media") || island,
        tray: on("tray"),
        // Wi-Fi and Bluetooth (and the VPNs in the control centre they open).
        network: on("network") || on("bluetooth"),
        system: on("system") || island,
        updates: on("updates"),
        weather: on("weather") || island,
        airpods: on("airpods"),
    }
}

/// The bar items for module `id` (empty when it has nothing to show right now).
pub fn items<'a>(
    id: &str,
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    // A module that panics is switched off; the rest of the bar keeps drawing.
    crate::guard::run(
        &format!("module {id}"),
        || match info(id) {
            Some(m) => (m.view)(app, o, s, open),
            // Anything else is a Lua widget (shown once it has produced something).
            None => app
                .chips
                .get_key_value(id)
                .map(|(name, c)| vec![lua::chip(name, c, s)])
                .unwrap_or_default(),
        },
        Vec::new,
    )
}

// --- The bar's design language: every module is drawn from these. ---------------------------

/// A bar button holding `content` (an icon and a label, a few icons): the bar's one height,
/// padding, shape and hover / pressed / open states.
pub fn chip<'a>(
    s: Scheme,
    content: impl Into<Element<'a, Message>>,
    active: bool,
) -> ui::control::Control<'a, Message> {
    ui::control(content)
        .padding([0, 10])
        .height(size::CONTROL)
        .radius(size::CONTROL / 2.0)
        .bar(s, active)
}

/// A round bar button holding one icon.
pub fn icon_chip<'a>(
    s: Scheme,
    i: Icon,
    color: iced::Color,
    active: bool,
) -> ui::control::Control<'a, Message> {
    ui::control(container(icon(i, size::ICON, color)).center(size::CONTROL))
        .radius(size::CONTROL / 2.0)
        .bar(s, active)
}

/// Whether the control centre is open on `page` (its button shows as open).
pub fn control_on(app: &App, open: Option<PanelKind>, page: crate::panels::control::Page) -> bool {
    open == Some(PanelKind::Control) && app.panels.control.page == page
}
