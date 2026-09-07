//! Changes to the system the shell holds: radios, Do not disturb, night light, sound levels,
//! brightness, joining networks, connecting devices. One list (`Change`) and one place that
//! carries it out (`App::change`), whoever asks: the control centre, Settings, the bar's
//! buttons, the island or an IPC command. Each change is shown at once (the shell's state is
//! set as asked, never waiting on the service) and sent on; the service's next report is the
//! truth again.

use super::*;

#[derive(Clone, Debug)]
pub enum Change {
    Wifi(bool),
    Bluetooth(bool),
    /// Every radio off (Wi-Fi and Bluetooth), or back to how they were.
    Airplane,
    /// Do not disturb on or off.
    Dnd,
    /// Do not disturb for this many minutes (0: until turned off; u64::MAX: until morning).
    DndFor(u64),
    NightLight,
    /// Night light from sunset to sunrise by itself (or not).
    NightSchedule(bool),
    /// Night light's warmth (kelvin), kept in config.toml.
    NightTemperature(u32),
    Caffeine,
    /// The next power mode (the tile), or this one (its page).
    NextProfile,
    Profile(power::Profile),
    GameMode,
    Warp(bool),
    Tailscale(bool),
    SinkVolume(f32),
    SinkMute,
    SourceVolume(f32),
    SourceMute,
    /// The output and microphone used by default.
    Sink(String),
    Source(String),
    /// One app's volume and mute (all its streams), and the output it plays to.
    AppVolume(Vec<u32>, f32),
    AppMute(Vec<u32>),
    AppTo(Vec<u32>, String),
    /// A recording app's level, mute and microphone.
    RecVolume(Vec<u32>, f32),
    RecMute(Vec<u32>),
    RecTo(Vec<u32>, String),
    /// The output's balance, -1 (left) to 1 (right).
    Balance(f32),
    /// A sound card's mode (card, profile).
    CardMode(u32, String),
    Brightness(f32),
    /// The external monitor's own brightness (DDC/CI), 0 to 1.
    DdcBrightness(f32),
    /// Dimming of a screen without a settable backlight, 0 to 1.
    Dim(f32),
    /// Change settings in config.toml: written, then applied at once (the file watcher's
    /// reload afterwards finds nothing new), the same way a hand edit is applied.
    Settings(Vec<config::Edit>),
    /// Join a network (its name, and how), shown in progress until it is so or fails.
    Join(String, network::Command),
    Disconnect,
    ForgetNetwork(String),
    /// A saved network joins by itself when in range (or not).
    AutoJoin(String, bool),
    /// Connect (true) or disconnect a Bluetooth device, shown in progress until it is so.
    Device {
        path: String,
        connect: bool,
    },
    /// Pair with a device found nearby (one at a time).
    Pair(String),
    ForgetDevice(String),
}

impl App {
    /// Carries out a change. The task ends when the service has done it (a panel that asked
    /// with `Action::ChangeThen` hears back then).
    pub(crate) fn change(&mut self, c: Change) -> Task<Message> {
        match c {
            Change::Wifi(on) => {
                self.net.wifi_enabled = on;
                Task::future(network::command(network::Command::SetWifi(on))).discard()
            }
            Change::Bluetooth(on) => {
                self.bt.powered = on;
                Task::future(bluetooth::command(bluetooth::Command::SetPowered(on))).discard()
            }
            Change::Airplane => {
                let before = crate::state::get(|s| s.toggles.airplane);
                let (wifi, bt) = match before {
                    // Off: each radio back to how it was.
                    Some(was) => {
                        crate::state::update(|s| s.toggles.airplane = None);
                        was
                    }
                    // On: remember them, then both off.
                    None => {
                        let was = (self.net.wifi_enabled, self.bt.powered);
                        crate::state::update(|s| s.toggles.airplane = Some(was));
                        (false, false)
                    }
                };
                self.net.wifi_enabled = wifi;
                self.bt.powered = bt;
                Task::batch([
                    Task::future(network::command(network::Command::SetWifi(wifi))).discard(),
                    Task::future(bluetooth::command(bluetooth::Command::SetPowered(bt))).discard(),
                ])
            }
            Change::Dnd => {
                self.dnd = !self.dnd;
                let on = self.dnd;
                crate::state::update(|s| {
                    s.toggles.dnd = on;
                    s.toggles.dnd_until = None;
                });
                Task::none()
            }
            Change::DndFor(minutes) => {
                let until = dnd_until(minutes);
                self.dnd = true;
                crate::state::update(|s| {
                    s.toggles.dnd = true;
                    s.toggles.dnd_until = until;
                });
                Task::none()
            }
            Change::NightLight => {
                self.night_light = !self.night_light;
                let on = self.night_light;
                crate::state::update(|s| s.toggles.night_light = on);
                let k = self.config.night_light_temperature;
                Task::future(toggles::set_night_light(on, k)).discard()
            }
            Change::NightSchedule(on) => self.change(Change::Settings(vec![config::Edit::new(
                &[],
                "night_light_schedule",
                toml_edit::value(on),
            )])),
            Change::NightTemperature(k) => self.change(Change::Settings(vec![config::Edit::new(
                &[],
                "night_light_temperature",
                toml_edit::value(i64::from(k)),
            )])),
            Change::Caffeine => {
                let on = !self.caffeine;
                self.caffeine = on;
                crate::state::update(|s| s.toggles.caffeine = on);
                Task::future(toggles::set_caffeine(on)).discard()
            }
            Change::NextProfile => match self.power.profile {
                Some(p) => self.change(Change::Profile(p.next())),
                None => Task::none(),
            },
            Change::Profile(p) => {
                self.power.profile = Some(p);
                Task::future(power::set_profile(p)).discard()
            }
            Change::GameMode => {
                self.game_mode = !self.game_mode;
                remember_game_mode(self.game_mode);
                // No shell motion at all while a game runs.
                crate::motion::set_still(self.game_mode);
                // Game mode drops the screen frame too, so the game gets every pixel.
                let hypr = Task::future(hyprland::game_mode(self.game_mode)).discard();
                Task::batch([hypr, self.apply_frame()])
            }
            Change::Warp(on) => {
                self.vpn.warp = Some(on);
                Task::future(vpn::set_warp(on)).discard()
            }
            Change::Tailscale(on) => {
                self.vpn.tailscale = Some(on);
                Task::future(async move {
                    vpn::set_tailscale(on).await;
                    // Timing guess (docs/ROADMAP-v2.md): the tailnet reads as it was for a
                    // moment after `tailscale up`.
                    tokio::time::sleep(Duration::from_millis(800)).await;
                })
                .discard()
            }
            Change::SinkVolume(v) => {
                let default = self.audio.default_sink.clone();
                if let Some(d) = self.audio.sinks.iter_mut().find(|d| d.name == default) {
                    d.volume = v;
                    d.muted = false;
                    audio::command(audio::Command::SinkVolume(d.name.clone(), d.channels, v));
                    if v > 0.0 {
                        audio::command(audio::Command::SinkMute(d.name.clone(), false));
                    }
                }
                Task::none()
            }
            Change::SinkMute => {
                let default = self.audio.default_sink.clone();
                if let Some(d) = self.audio.sinks.iter_mut().find(|d| d.name == default) {
                    d.muted = !d.muted;
                    audio::command(audio::Command::SinkMute(d.name.clone(), d.muted));
                }
                Task::none()
            }
            Change::SourceVolume(v) => {
                let default = self.audio.default_source.clone();
                if let Some(d) = self.audio.sources.iter_mut().find(|d| d.name == default) {
                    d.volume = v;
                    audio::command(audio::Command::SourceVolume(d.name.clone(), d.channels, v));
                }
                Task::none()
            }
            Change::SourceMute => {
                let default = self.audio.default_source.clone();
                if let Some(d) = self.audio.sources.iter_mut().find(|d| d.name == default) {
                    d.muted = !d.muted;
                    audio::command(audio::Command::SourceMute(d.name.clone(), d.muted));
                }
                Task::none()
            }
            Change::Sink(name) => {
                // Choosing an output ends any mode switch's hold: the user's choice is what shows.
                self.mode_switch = None;
                self.audio.default_sink = name.clone();
                audio::command(audio::Command::DefaultSink(name));
                Task::none()
            }
            Change::Source(name) => {
                self.audio.default_source = name.clone();
                audio::command(audio::Command::DefaultSource(name));
                Task::none()
            }
            Change::AppVolume(ids, v) => {
                if let Some(a) = self.audio.apps.iter_mut().find(|a| a.streams == ids) {
                    a.volume = v;
                    a.muted = false;
                    audio::command(audio::Command::AppVolume(ids.clone(), a.channels, v));
                    audio::command(audio::Command::AppMute(ids, false));
                }
                Task::none()
            }
            Change::AppMute(ids) => {
                if let Some(a) = self.audio.apps.iter_mut().find(|a| a.streams == ids) {
                    a.muted = !a.muted;
                    audio::command(audio::Command::AppMute(ids, a.muted));
                }
                Task::none()
            }
            Change::AppTo(ids, sink) => {
                if let Some(a) = self.audio.apps.iter_mut().find(|a| a.streams == ids) {
                    a.sink = sink.clone();
                }
                audio::command(audio::Command::AppMove(ids, sink));
                Task::none()
            }
            Change::RecVolume(ids, v) => {
                if let Some(a) = self.audio.recorders.iter_mut().find(|a| a.streams == ids) {
                    a.volume = v;
                    a.muted = false;
                    audio::command(audio::Command::RecorderVolume(ids.clone(), a.channels, v));
                    audio::command(audio::Command::RecorderMute(ids, false));
                }
                Task::none()
            }
            Change::RecMute(ids) => {
                if let Some(a) = self.audio.recorders.iter_mut().find(|a| a.streams == ids) {
                    a.muted = !a.muted;
                    audio::command(audio::Command::RecorderMute(ids, a.muted));
                }
                Task::none()
            }
            Change::RecTo(ids, source) => {
                if let Some(a) = self.audio.recorders.iter_mut().find(|a| a.streams == ids) {
                    a.sink = source.clone();
                }
                audio::command(audio::Command::RecorderMove(ids, source));
                Task::none()
            }
            Change::Balance(b) => {
                let name = self.audio.default_sink.clone();
                if let Some(d) = self.audio.sinks.iter_mut().find(|d| d.name == name) {
                    d.balance = b;
                }
                audio::command(audio::Command::SinkBalance(name, b));
                Task::none()
            }
            Change::CardMode(card, profile) => {
                if let Some(k) = self.audio.cards.iter_mut().find(|k| k.index == card) {
                    k.active = profile.clone();
                }
                self.mode_switch = Some((card, profile.clone(), Instant::now()));
                audio::command(audio::Command::CardProfile(card, profile));
                Task::none()
            }
            Change::Brightness(v) => {
                self.brightness = Some(v);
                Task::future(power::set_brightness(v)).discard()
            }
            Change::DdcBrightness(v) => match self.ddc_target() {
                Some(bus) => {
                    self.ddc_level.insert(bus, v);
                    Task::future(services::ddc::set(bus, v)).discard()
                }
                None => Task::none(),
            },
            Change::Dim(v) => {
                self.dim = v.max(toggles::MIN_GAMMA as f32 / 100.0);
                Task::future(toggles::set_dim(v)).discard()
            }
            Change::Settings(edits) => {
                for e in edits {
                    e.write();
                }
                match config::read() {
                    Ok(c) => {
                        self.config_error = None;
                        apply_config(self, c)
                    }
                    // A hand-made mistake elsewhere in the file: the watcher says where.
                    Err(_) => Task::none(),
                }
            }
            Change::Join(ssid, cmd) => {
                self.connecting = Some(panels::cx::Connecting::new(ssid.clone(), true));
                self.net_error = None;
                Task::perform(network::try_command(cmd), move |r| {
                    Message::NetDone(ssid.clone(), r)
                })
            }
            Change::Disconnect => {
                if let Some(ssid) = self.net.ssid.clone() {
                    self.connecting = Some(panels::cx::Connecting::new(ssid, false));
                }
                Task::future(network::command(network::Command::Disconnect)).discard()
            }
            Change::ForgetNetwork(ssid) => {
                Task::future(network::command(network::Command::Forget(ssid))).discard()
            }
            Change::AutoJoin(ssid, on) => {
                if let Some(k) = self.net.saved.iter_mut().find(|k| k.ssid == ssid) {
                    k.auto = on;
                }
                Task::future(network::command(network::Command::AutoJoin(ssid, on))).discard()
            }
            Change::Device { path, connect } => {
                self.connecting = Some(panels::cx::Connecting::new(path.clone(), connect));
                self.device_error = None;
                let cmd = if connect {
                    bluetooth::Command::Connect(path.clone())
                } else {
                    bluetooth::Command::Disconnect(path.clone())
                };
                Task::perform(bluetooth::command(cmd), move |r| {
                    Message::DeviceDone(path.clone(), r)
                })
            }
            Change::Pair(path) => {
                if self.pairing.is_some() {
                    return Task::none();
                }
                self.pairing = Some(path.clone());
                self.device_error = None;
                Task::perform(
                    bluetooth::command(bluetooth::Command::Pair(path.clone())),
                    move |r| Message::DeviceDone(path.clone(), r),
                )
            }
            Change::ForgetDevice(path) => Task::perform(
                bluetooth::command(bluetooth::Command::Forget(path.clone())),
                move |r| Message::DeviceDone(path.clone(), r),
            ),
        }
    }

    /// A join ended: no more "Connecting…", and why it failed (shown under its name).
    pub(crate) fn net_done(&mut self, ssid: String, r: Result<(), String>) -> Task<Message> {
        if self.connecting.as_ref().is_some_and(|c| c.is(&ssid)) {
            self.connecting = None;
        }
        self.net_error = match r {
            // Cancelling the password prompt is not an error to show.
            Err(e) if e != "Cancelled" => Some((ssid, e)),
            _ => None,
        };
        Task::none()
    }

    /// A Bluetooth action ended (connect, disconnect, pair, forget): the device and how.
    pub(crate) fn device_done(&mut self, path: String, r: Result<(), String>) -> Task<Message> {
        if self.pairing.as_deref() == Some(path.as_str()) {
            self.pairing = None;
        }
        match r {
            Ok(()) => {
                if self.device_error.as_ref().is_some_and(|(p, _)| *p == path) {
                    self.device_error = None;
                }
            }
            Err(why) => {
                if self.connecting.as_ref().is_some_and(|c| c.is(&path)) {
                    self.connecting = None;
                }
                self.device_error = Some((path, why));
            }
        }
        Task::none()
    }
}

impl Change {
    /// Joining `n`: at once when it is saved or open; None when it needs a password first.
    pub fn join(n: &network::Network) -> Option<Change> {
        if n.active || !(n.known || !n.secure) {
            return None;
        }
        let cmd = if n.known {
            network::Command::Connect(n.ssid.clone())
        } else {
            network::Command::ConnectNew {
                ssid: n.ssid.clone(),
                ap: n.ap.clone(),
                password: None,
            }
        };
        Some(Change::Join(n.ssid.clone(), cmd))
    }
}

/// When Do not disturb for `minutes` ends (seconds since 1970): 0 is never, u64::MAX the next
/// 7:00.
fn dnd_until(minutes: u64) -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match minutes {
        0 => None,
        // Until 7:00 tomorrow (or today, before 7).
        u64::MAX => {
            let local = chrono::Local::now();
            let mut day = local.date_naive();
            if local.time() >= chrono::NaiveTime::from_hms_opt(7, 0, 0).unwrap_or_default() {
                day = day.succ_opt().unwrap_or(day);
            }
            day.and_hms_opt(7, 0, 0)
                .and_then(|t| t.and_local_timezone(chrono::Local).single())
                .map(|t| t.timestamp().max(0) as u64)
        }
        m => Some(now + m * 60),
    }
}
