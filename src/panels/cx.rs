//! What a panel may see of the shell: a read-only context (`Cx`), handed to every panel with
//! its own state. A panel changes nothing but its own state; anything else it wants (close,
//! open another panel, change a setting the shell holds) it asks for with a message, which
//! the shell carries out in one place (docs/ARCHITECTURE.md, "Panels are components").
//!
//! `split` hands out a panel's state and the context together: the borrow checker sees two
//! separate parts of the shell, so a panel cannot reach the shell or another panel's state.

use crate::app::App;
use crate::config::Config;
use crate::services::{airpods, audio, bluetooth, hyprland, mpris, network, tray, web};
use crate::theme::Scheme;

/// A network or device being connected or disconnected right now (a network's name, a
/// device's path), whether it should end up connected, and since when: shown as in progress
/// until the system says it is so, or for at most `Connecting::LIMIT`. The shell's, not a
/// panel's: the control centre, Settings and the AirPods panel all show it.
#[derive(Clone, Debug)]
pub struct Connecting {
    key: String,
    want: bool,
    since: std::time::Instant,
}

impl Connecting {
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(25);

    pub fn new(key: impl Into<String>, want: bool) -> Self {
        Self {
            key: key.into(),
            want,
            since: std::time::Instant::now(),
        }
    }

    /// Whether this is about `key`.
    pub fn is(&self, key: &str) -> bool {
        self.key == key
    }

    /// Whether `key` is still on its way (and to connected or not), by the system's state.
    pub fn in_progress(
        &self,
        key: &str,
        bt: &bluetooth::State,
        net: &network::State,
    ) -> Option<bool> {
        if self.key != key || self.since.elapsed() > Self::LIMIT {
            return None;
        }
        let done = if let Some(d) = bt.devices.iter().find(|d| d.path == self.key) {
            d.connected == self.want
        } else {
            // A network: connected to it (or, when leaving, to nothing).
            let on = net.ssid.as_deref() == Some(self.key.as_str()) && !net.connecting;
            on == self.want
        };
        (!done).then_some(self.want)
    }
}

/// The shell, read-only, as panels see it.
pub struct Cx<'a> {
    pub scheme: Scheme,
    pub config: &'a Config,
    pub now: &'a chrono::DateTime<chrono::Local>,
    pub hypr: &'a hyprland::State,
    pub audio: &'a audio::State,
    pub bt: &'a bluetooth::State,
    pub net: &'a network::State,
    pub connecting: Option<&'a Connecting>,
    /// The last device that could not be connected or paired, and why.
    pub device_error: Option<&'a (String, String)>,
    pub media: &'a mpris::State,
    /// Players whose track length is not to be trusted (no progress bar for them).
    pub length_unreliable: &'a std::collections::HashSet<String>,
    pub weather: Option<&'a web::Weather>,
    pub tray: &'a tray::State,
    pub pods: Option<&'a airpods::Pods>,
    /// The apps using a camera now.
    pub camera_apps: &'a [String],
    /// What used the camera, microphone or screen lately: (device, app, when).
    pub privacy_recent: &'a [(crate::ui::Icon, String, std::time::Instant)],
}

impl Cx<'_> {
    /// Whether `key` (a network's name or a device's path) is being connected (Some(true)) or
    /// disconnected (Some(false)).
    pub fn in_progress(&self, key: &str) -> Option<bool> {
        self.connecting?.in_progress(key, self.bt, self.net)
    }

    /// A temperature in the units chosen in Settings ("21°").
    pub fn temp(&self, celsius: f32) -> String {
        self.config.weather_units.show(celsius)
    }
}

/// The panels' states (writable) and the context (read-only), from the same shell (for the
/// panels' updates as they move onto it).
#[allow(dead_code)]
pub fn split(app: &mut App) -> (&mut super::State, Cx<'_>) {
    let App {
        panels,
        scheme,
        config,
        now,
        hypr,
        audio,
        bt,
        net,
        connecting,
        device_error,
        media,
        length_unreliable,
        weather,
        tray,
        pods,
        camera_apps,
        privacy_recent,
        ..
    } = app;
    let cx = Cx {
        scheme: *scheme,
        config,
        now,
        hypr,
        audio,
        bt,
        net,
        connecting: connecting.as_ref(),
        device_error: device_error.as_ref(),
        media,
        length_unreliable,
        weather: weather.as_ref(),
        tray,
        pods: pods.as_ref(),
        camera_apps,
        privacy_recent,
    };
    (panels, cx)
}

/// The context alone, for drawing.
pub fn of(app: &App) -> (&super::State, Cx<'_>) {
    let cx = Cx {
        scheme: app.scheme,
        config: &app.config,
        now: &app.now,
        hypr: &app.hypr,
        audio: &app.audio,
        bt: &app.bt,
        net: &app.net,
        connecting: app.connecting.as_ref(),
        device_error: app.device_error.as_ref(),
        media: &app.media,
        length_unreliable: &app.length_unreliable,
        weather: app.weather.as_ref(),
        tray: &app.tray,
        pods: app.pods.as_ref(),
        camera_apps: &app.camera_apps,
        privacy_recent: &app.privacy_recent,
    };
    (&app.panels, cx)
}
