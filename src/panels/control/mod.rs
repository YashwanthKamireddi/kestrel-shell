//! Control centre: quick toggles, levels, notifications, and pages for Wi-Fi, Bluetooth,
//! sound output, night light, power mode, Cloudflare WARP and Tailscale.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::services::{audio, bluetooth, network, power, toggles};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::slider::slider;
use crate::ui::{
    self, Icon, card, empty, icon, icon_button, label, label_w, list_row, subpage_header, switch,
    tile,
};
use iced::widget::{Space, container, row, text_input};
use iced::{Alignment, Element, Length, Task};

// One file per page (its view and what only it uses); this file keeps the state, messages,
// update, the panel's frame and what every page shares. Public pieces are re-exported, so
// `control::device_icon` and the like keep their paths.
mod bt_page;
mod main;
mod night;
mod power_page;
mod quiet;
mod sound;
mod vpn;
mod wifi;
pub use bt_page::*;
use main::*;
use night::*;
use power_page::*;
pub use quiet::*;
pub use sound::*;
use vpn::*;
use wifi::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    Wifi,
    Bluetooth,
    Output,
    NightLight,
    Power,
    Warp,
    Tailscale,
    /// Do not disturb: for how long.
    Quiet,
    /// A Wi-Fi network's details (`State::network`).
    Network,
    /// Joining a network that does not announce its name.
    Hidden,
}

#[derive(Default)]
pub struct State {
    pub page: Page,
    /// The Wi-Fi list's order while its page is open (ssids): sorted fresh on opening, then
    /// kept, so a rescan never moves the row under the pointer; new networks join at the end.
    pub wifi_order: Vec<String>,
    /// The same for Bluetooth's nearby devices (paths): found ones join at the end.
    pub nearby_order: Vec<String>,
    /// The tile the keyboard is on (arrows move it, Enter toggles, Right opens its page).
    pub focus: Option<usize>,
    /// Network waiting for a password: (ssid, access point, typed text).
    pub password: Option<(String, String, String)>,
    /// The tailnet as last read (its page reads it on opening).
    tailnet: Option<crate::services::vpn::Tailnet>,
    /// What WARP says about itself, as last read.
    warp: Option<crate::services::vpn::Warp>,
    /// A Tailscale action that failed, and why.
    ts_error: Option<String>,
    /// Peers pinged: address → round trip ("23 ms", or "no reply").
    pings: std::collections::HashMap<String, String>,
    /// Night light warmth while the slider moves (kelvin).
    pub kelvin: Option<u32>,
    /// What is being connected or disconnected right now (a network's name, a device's
    /// path), whether it should end up connected, and since when: shown as in progress until
    /// it is, or for at most `PENDING`.
    pub pending: Option<(String, bool, std::time::Instant)>,
    /// The Sound page's app whose device list is open.
    sound_open: Option<String>,
    /// The device being paired (its path).
    pub pairing: Option<String>,
    /// A device whose last action failed, and why (shown under its name).
    pub bt_error: Option<(String, String)>,
    /// A network whose last join failed, and why (shown under its name).
    pub net_error: Option<(String, String)>,
    /// The device whose Forget was pressed once (asks before removing).
    pub forget: Option<String>,
    /// The Wi-Fi network whose details page is open.
    network: Option<String>,
    /// Its Forget was pressed once.
    forget_net: bool,
    /// A hidden network being typed in: name and password.
    pub hidden: (String, String),
}

/// Whether the control centre is waiting for typing (a Wi-Fi password).
pub fn typing(st: &State) -> bool {
    st.password.is_some() || st.page == Page::Hidden
}

/// The reason the last join of `ssid` failed, if it did.
pub fn st_net_error(app: &App, ssid: &str) -> Option<(String, String)> {
    app.panels
        .control
        .net_error
        .clone()
        .filter(|(k, _)| k == ssid)
}

/// Joins a network and reports how it ended (connected, or why not).
fn join(ssid: String, cmd: network::Command) -> Task<AppMessage> {
    Task::perform(network::try_command(cmd), move |r| {
        msg(Message::NetDone(ssid.clone(), r))
    })
}

/// How long an attempt shows as in progress at most.
const PENDING: std::time::Duration = std::time::Duration::from_secs(25);

/// Whether `key` (a network's name or a device's path) is being connected or disconnected.
pub fn in_progress(app: &App, key: &str) -> Option<bool> {
    let (k, want, since) = app.panels.control.pending.as_ref()?;
    if k != key || since.elapsed() > PENDING {
        return None;
    }
    let done = if let Some(d) = app.bt.devices.iter().find(|d| d.path == *k) {
        d.connected == *want
    } else {
        // A network: connected to it (or, when leaving, to nothing).
        let on = app.net.ssid.as_deref() == Some(k.as_str()) && !app.net.connecting;
        on == *want
    };
    (!done).then_some(*want)
}

#[derive(Clone, Debug)]
pub enum Message {
    Page(Page),
    Wifi(bool),
    Bluetooth(bool),
    /// Every radio off (Wi-Fi and Bluetooth), or back to how they were.
    Airplane,
    Dnd,
    /// Do not disturb for this many minutes (0: until turned off; u64::MAX: until morning).
    DndFor(u64),
    NightLight,
    Caffeine,
    Profile,
    GameMode,
    Warp(bool),
    Tailscale(bool),
    SinkVolume(f32),
    SinkMute,
    SourceVolume(f32),
    SourceMute,
    /// The microphone used by default.
    Source(String),
    /// One app's volume (all its streams).
    AppVolume(Vec<u32>, f32),
    AppMute(Vec<u32>),
    /// Open (or close) an app's list of devices (its key: name, "rec:" for recording).
    AppPick(Option<String>),
    /// An app to this output / a recording app to this microphone.
    AppTo(Vec<u32>, String),
    RecTo(Vec<u32>, String),
    /// A recording app's level and mute.
    RecVolume(Vec<u32>, f32),
    RecMute(Vec<u32>),
    /// The output's balance, -1 (left) to 1 (right).
    Balance(f32),
    /// A sound card's mode (card, profile).
    CardMode(u32, String),
    Brightness(f32),
    BrightnessDone,
    /// Dimming of a screen without a settable backlight, 0 to 1.
    Dim(f32),
    /// The external monitor's own brightness (DDC/CI), 0 to 1.
    DdcBrightness(f32),
    Join(network::Network),
    Password(String),
    SubmitPassword,
    Disconnect,
    Device(String, bool),
    /// Pair with a device found nearby.
    Pair(String),
    /// A Bluetooth action ended: the device's path and how.
    BtDone(String, Result<(), String>),
    /// Forget a paired device: the first press asks, the second removes it.
    Forget(String),
    Sink(String),
    /// Night light warmth, 0 (warmest) to 1 (coolest), while dragging; then done.
    Warmth(f32),
    WarmthDone,
    /// Night light from sunset to sunrise by itself (or not).
    Schedule(bool),
    SetProfile(power::Profile),
    Tailnet(Option<crate::services::vpn::Tailnet>),
    WarpInfo(Option<crate::services::vpn::Warp>),
    WarpMode(String),
    /// A `tailscale` command (set, down, logout…), then a fresh read.
    Tailscale2(Vec<String>),
    TailscaleDone(Result<(), String>),
    Ping(String),
    Pinged(String, Option<String>),
    Login,
    /// Copy a tailnet address.
    CopyIp(String),
    /// Open a Wi-Fi network's details.
    NetworkInfo(String),
    /// Forget the open network: the first press asks, the second removes it.
    ForgetNetwork,
    /// The open network joins by itself when in range (or not).
    AutoJoin(bool),
    HiddenName(String),
    HiddenPassword(String),
    JoinHidden,
    /// A join ended: the network and how.
    NetDone(String, Result<(), String>),
}

/// Night light's range, warmest to coolest (kelvin).
pub const WARMEST: f32 = 2500.0;
pub const COOLEST: f32 = 6000.0;

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Control(m))
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    let st = &mut app.panels.control;
    match m {
        Message::Page(p) => {
            st.page = p;
            st.password = None;
            match p {
                Page::Bluetooth => {
                    st.nearby_order.clear();
                }
                Page::Wifi => {
                    st.wifi_order.clear();
                    keep_order(
                        &mut st.wifi_order,
                        app.net.networks.iter().map(|n| n.ssid.as_str()),
                    );
                    return Task::future(network::command(network::Command::Scan)).discard();
                }
                Page::Hidden => {
                    return Task::Iced(iced::widget::focus(iced::widget::Id::new("hidden-name")));
                }
                Page::Tailscale => {
                    return Task::perform(crate::services::vpn::tailnet(), |t| {
                        msg(Message::Tailnet(t))
                    });
                }
                Page::Warp => {
                    return Task::perform(crate::services::vpn::warp(), |w| {
                        msg(Message::WarpInfo(w))
                    });
                }
                _ => {}
            }
        }
        Message::Warmth(v) => {
            let k = (WARMEST + v.clamp(0.0, 1.0) * (COOLEST - WARMEST)).round() as u32;
            let k = k / 100 * 100;
            st.kelvin = Some(k);
            return Task::future(toggles::set_temperature(k)).discard();
        }
        Message::Schedule(on) => {
            app.config.night_light_schedule = on;
            crate::config::set(&[], "night_light_schedule", toml_edit::value(on));
            app.night_auto = None;
            return app.night_schedule();
        }
        Message::WarmthDone => {
            if let Some(k) = st.kelvin.take() {
                app.config.night_light_temperature = k;
                crate::config::set(
                    &[],
                    "night_light_temperature",
                    toml_edit::value(i64::from(k)),
                );
            }
        }
        Message::SetProfile(p) => {
            app.power.profile = Some(p);
            return Task::future(power::set_profile(p)).discard();
        }
        Message::Tailnet(t) => st.tailnet = t,
        Message::WarpInfo(w) => st.warp = w,
        Message::WarpMode(m) => {
            if let Some(w) = &mut st.warp {
                w.mode = m.clone();
            }
            return Task::perform(
                async move {
                    crate::services::vpn::warp_mode(m).await;
                    crate::services::vpn::warp().await
                },
                |w| msg(Message::WarpInfo(w)),
            );
        }
        Message::Tailscale2(args) => {
            st.ts_error = None;
            return Task::perform(crate::services::vpn::tailscale(args), |r| {
                msg(Message::TailscaleDone(r))
            });
        }
        Message::TailscaleDone(r) => {
            st.ts_error = r.err();
            return Task::perform(crate::services::vpn::tailnet(), |t| {
                msg(Message::Tailnet(t))
            });
        }
        Message::Ping(ip) => {
            st.pings.insert(ip.clone(), "…".into());
            return Task::perform(
                async move {
                    let r = crate::services::vpn::tailscale_ping(ip.clone()).await;
                    (ip, r)
                },
                |(ip, r)| msg(Message::Pinged(ip, r)),
            );
        }
        Message::Pinged(ip, r) => {
            st.pings.insert(ip, r.unwrap_or_else(|| "no reply".into()));
        }
        Message::Login => {
            return Task::future(crate::services::vpn::tailscale_login()).discard();
        }
        Message::CopyIp(ip) => {
            return Task::future(crate::panels::clipboard::copy_text(ip)).discard();
        }
        Message::Wifi(on) => {
            app.net.wifi_enabled = on;
            return Task::future(network::command(network::Command::SetWifi(on))).discard();
        }
        Message::Airplane => {
            let before = crate::state::get(|s| s.toggles.airplane);
            let (wifi, bt) = match before {
                // Off: each radio back to how it was.
                Some(was) => {
                    crate::state::update(|s| s.toggles.airplane = None);
                    was
                }
                // On: remember them, then both off.
                None => {
                    let was = (app.net.wifi_enabled, app.bt.powered);
                    crate::state::update(|s| s.toggles.airplane = Some(was));
                    (false, false)
                }
            };
            app.net.wifi_enabled = wifi;
            app.bt.powered = bt;
            return Task::batch([
                Task::future(network::command(network::Command::SetWifi(wifi))).discard(),
                Task::future(bluetooth::command(bluetooth::Command::SetPowered(bt))).discard(),
            ]);
        }
        Message::Bluetooth(on) => {
            app.bt.powered = on;
            return Task::future(bluetooth::command(bluetooth::Command::SetPowered(on))).discard();
        }
        Message::Dnd => {
            app.dnd = !app.dnd;
            let on = app.dnd;
            crate::state::update(|s| {
                s.toggles.dnd = on;
                s.toggles.dnd_until = None;
            });
        }
        Message::DndFor(minutes) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let until = match minutes {
                0 => None,
                // Until 7:00 tomorrow (or today, before 7).
                u64::MAX => {
                    let local = chrono::Local::now();
                    let mut day = local.date_naive();
                    if local.time() >= chrono::NaiveTime::from_hms_opt(7, 0, 0).unwrap_or_default()
                    {
                        day = day.succ_opt().unwrap_or(day);
                    }
                    day.and_hms_opt(7, 0, 0)
                        .and_then(|t| t.and_local_timezone(chrono::Local).single())
                        .map(|t| t.timestamp().max(0) as u64)
                }
                m => Some(now + m * 60),
            };
            app.dnd = true;
            crate::state::update(|s| {
                s.toggles.dnd = true;
                s.toggles.dnd_until = until;
            });
            st.page = Page::Main;
        }
        Message::NightLight => {
            app.night_light = !app.night_light;
            let on = app.night_light;
            crate::state::update(|s| s.toggles.night_light = on);
            let k = app.config.night_light_temperature;
            return Task::future(toggles::set_night_light(on, k)).discard();
        }
        Message::Caffeine => {
            let on = !app.caffeine;
            app.caffeine = on;
            crate::state::update(|s| s.toggles.caffeine = on);
            return Task::future(toggles::set_caffeine(on)).discard();
        }
        Message::Profile => {
            if let Some(p) = app.power.profile {
                let next = p.next();
                app.power.profile = Some(next);
                return Task::future(power::set_profile(next)).discard();
            }
        }
        Message::GameMode => {
            app.game_mode = !app.game_mode;
            crate::app::remember_game_mode(app.game_mode);
            // No shell motion at all while a game runs.
            crate::motion::set_still(app.game_mode);
            // Game mode drops the screen frame too, so the game gets every pixel.
            let hypr = Task::future(crate::services::hyprland::game_mode(app.game_mode)).discard();
            return Task::batch([hypr, app.apply_frame()]);
        }
        Message::Warp(on) => {
            app.vpn.warp = Some(on);
            st.warp = None;
            // Read back once it has had a moment, so the page shows what WARP says.
            return Task::perform(
                async move {
                    crate::services::vpn::set_warp(on).await;
                    crate::services::vpn::warp().await
                },
                |w| msg(Message::WarpInfo(w)),
            );
        }
        Message::Tailscale(on) => {
            app.vpn.tailscale = Some(on);
            st.tailnet = None;
            return Task::perform(
                async move {
                    crate::services::vpn::set_tailscale(on).await;
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    crate::services::vpn::tailnet().await
                },
                |t| msg(Message::Tailnet(t)),
            );
        }
        Message::SinkVolume(v) => {
            if let Some(d) = app
                .audio
                .sinks
                .iter_mut()
                .find(|d| d.name == app.audio.default_sink)
            {
                d.volume = v;
                d.muted = false;
                audio::command(audio::Command::SinkVolume(d.name.clone(), d.channels, v));
                if v > 0.0 {
                    audio::command(audio::Command::SinkMute(d.name.clone(), false));
                }
            }
        }
        Message::SinkMute => {
            if let Some(d) = app
                .audio
                .sinks
                .iter_mut()
                .find(|d| d.name == app.audio.default_sink)
            {
                d.muted = !d.muted;
                audio::command(audio::Command::SinkMute(d.name.clone(), d.muted));
            }
        }
        Message::SourceVolume(v) => {
            if let Some(d) = app
                .audio
                .sources
                .iter_mut()
                .find(|d| d.name == app.audio.default_source)
            {
                d.volume = v;
                audio::command(audio::Command::SourceVolume(d.name.clone(), d.channels, v));
            }
        }
        Message::SourceMute => {
            if let Some(d) = app
                .audio
                .sources
                .iter_mut()
                .find(|d| d.name == app.audio.default_source)
            {
                d.muted = !d.muted;
                audio::command(audio::Command::SourceMute(d.name.clone(), d.muted));
            }
        }
        Message::Brightness(v) => {
            app.brightness = Some(v);
            return Task::future(power::set_brightness(v)).discard();
        }
        Message::BrightnessDone => {}
        Message::DdcBrightness(v) => {
            if let Some(bus) = app.ddc_target() {
                app.ddc_level.insert(bus, v);
                return Task::future(crate::services::ddc::set(bus, v)).discard();
            }
        }
        Message::Dim(v) => {
            app.dim = v.max(toggles::MIN_GAMMA as f32 / 100.0);
            return Task::future(toggles::set_dim(v)).discard();
        }
        Message::Join(n) => {
            if n.active {
                return Task::none();
            }
            st.pending = Some((n.ssid.clone(), true, std::time::Instant::now()));
            st.net_error = None;
            if n.known || !n.secure {
                let ssid = n.ssid.clone();
                let cmd = if n.known {
                    network::Command::Connect(n.ssid)
                } else {
                    network::Command::ConnectNew {
                        ssid: n.ssid,
                        ap: n.ap,
                        password: None,
                    }
                };
                return join(ssid, cmd);
            }
            st.password = Some((n.ssid, n.ap, String::new()));
            return Task::Iced(iced::widget::focus(PASSWORD_ID.clone()));
        }
        Message::Password(p) => {
            if let Some(pw) = &mut st.password {
                pw.2 = p;
            }
        }
        Message::SubmitPassword => {
            if let Some((ssid, ap, pw)) = st.password.take()
                && !pw.is_empty()
            {
                st.net_error = None;
                st.pending = Some((ssid.clone(), true, std::time::Instant::now()));
                return join(
                    ssid.clone(),
                    network::Command::ConnectNew {
                        ssid,
                        ap,
                        password: Some(pw),
                    },
                );
            }
        }
        Message::NetworkInfo(ssid) => {
            st.network = Some(ssid);
            st.forget_net = false;
            st.password = None;
            st.page = Page::Network;
        }
        Message::ForgetNetwork => {
            if !st.forget_net {
                st.forget_net = true;
                return Task::none();
            }
            st.forget_net = false;
            if let Some(ssid) = st.network.take() {
                st.page = Page::Wifi;
                return Task::future(network::command(network::Command::Forget(ssid))).discard();
            }
        }
        Message::AutoJoin(on) => {
            if let Some(ssid) = st.network.clone() {
                if let Some(k) = app.net.saved.iter_mut().find(|k| k.ssid == ssid) {
                    k.auto = on;
                }
                return Task::future(network::command(network::Command::AutoJoin(ssid, on)))
                    .discard();
            }
        }
        Message::HiddenName(t) => st.hidden.0 = t,
        Message::HiddenPassword(t) => st.hidden.1 = t,
        Message::JoinHidden => {
            let (ssid, pw) = std::mem::take(&mut st.hidden);
            if !ssid.trim().is_empty() {
                st.pending = Some((ssid.clone(), true, std::time::Instant::now()));
                st.net_error = None;
                st.page = Page::Wifi;
                return join(
                    ssid.clone(),
                    network::Command::ConnectHidden {
                        ssid,
                        password: (!pw.is_empty()).then_some(pw),
                    },
                );
            }
        }
        Message::NetDone(ssid, r) => {
            // The attempt is over either way: no more "Connecting…".
            if st.pending.as_ref().is_some_and(|(k, _, _)| *k == ssid) {
                st.pending = None;
            }
            st.net_error = match r {
                // Cancelling the password prompt is not an error to show.
                Err(e) if e != "Cancelled" => Some((ssid, e)),
                _ => None,
            };
        }
        Message::Disconnect => {
            if let Some(ssid) = app.net.ssid.clone() {
                st.pending = Some((ssid, false, std::time::Instant::now()));
            }
            return Task::future(network::command(network::Command::Disconnect)).discard();
        }
        Message::Device(path, connected) => {
            st.pending = Some((path.clone(), !connected, std::time::Instant::now()));
            st.bt_error = None;
            st.forget = None;
            let cmd = if connected {
                bluetooth::Command::Disconnect(path.clone())
            } else {
                bluetooth::Command::Connect(path.clone())
            };
            return Task::perform(bluetooth::command(cmd), move |r| {
                msg(Message::BtDone(path.clone(), r))
            });
        }
        Message::Pair(path) => {
            if st.pairing.is_some() {
                return Task::none();
            }
            st.pairing = Some(path.clone());
            st.bt_error = None;
            return Task::perform(
                bluetooth::command(bluetooth::Command::Pair(path.clone())),
                move |r| msg(Message::BtDone(path.clone(), r)),
            );
        }
        Message::BtDone(path, r) => {
            if st.pairing.as_deref() == Some(path.as_str()) {
                st.pairing = None;
            }
            match r {
                Ok(()) => {
                    if st.bt_error.as_ref().is_some_and(|(p, _)| *p == path) {
                        st.bt_error = None;
                    }
                }
                Err(why) => {
                    if st.pending.as_ref().is_some_and(|(p, _, _)| *p == path) {
                        st.pending = None;
                    }
                    st.bt_error = Some((path, why));
                }
            }
        }
        Message::Forget(path) => {
            if st.forget.as_deref() != Some(path.as_str()) {
                st.forget = Some(path);
                return Task::none();
            }
            st.forget = None;
            return Task::perform(
                bluetooth::command(bluetooth::Command::Forget(path.clone())),
                move |r| msg(Message::BtDone(path.clone(), r)),
            );
        }
        Message::Source(name) => {
            app.audio.default_source = name.clone();
            audio::command(audio::Command::DefaultSource(name));
        }
        // Applied here at once (the slider never waits for the sound server) and sent on.
        Message::AppVolume(ids, v) => {
            if let Some(a) = app.audio.apps.iter_mut().find(|a| a.streams == ids) {
                a.volume = v;
                a.muted = false;
                audio::command(audio::Command::AppVolume(ids.clone(), a.channels, v));
                audio::command(audio::Command::AppMute(ids, false));
            }
        }
        Message::AppMute(ids) => {
            if let Some(a) = app.audio.apps.iter_mut().find(|a| a.streams == ids) {
                a.muted = !a.muted;
                audio::command(audio::Command::AppMute(ids, a.muted));
            }
        }
        Message::AppPick(key) => st.sound_open = key,
        Message::AppTo(ids, sink) => {
            if let Some(a) = app.audio.apps.iter_mut().find(|a| a.streams == ids) {
                a.sink = sink.clone();
            }
            st.sound_open = None;
            audio::command(audio::Command::AppMove(ids, sink));
        }
        Message::RecTo(ids, source) => {
            if let Some(a) = app.audio.recorders.iter_mut().find(|a| a.streams == ids) {
                a.sink = source.clone();
            }
            st.sound_open = None;
            audio::command(audio::Command::RecorderMove(ids, source));
        }
        Message::RecVolume(ids, v) => {
            if let Some(a) = app.audio.recorders.iter_mut().find(|a| a.streams == ids) {
                a.volume = v;
                a.muted = false;
                audio::command(audio::Command::RecorderVolume(ids.clone(), a.channels, v));
                audio::command(audio::Command::RecorderMute(ids, false));
            }
        }
        Message::RecMute(ids) => {
            if let Some(a) = app.audio.recorders.iter_mut().find(|a| a.streams == ids) {
                a.muted = !a.muted;
                audio::command(audio::Command::RecorderMute(ids, a.muted));
            }
        }
        Message::Balance(b) => {
            let name = app.audio.default_sink.clone();
            if let Some(d) = app.audio.sinks.iter_mut().find(|d| d.name == name) {
                d.balance = b;
            }
            audio::command(audio::Command::SinkBalance(name, b));
        }
        Message::CardMode(card, profile) => {
            if let Some(k) = app.audio.cards.iter_mut().find(|k| k.index == card) {
                k.active = profile.clone();
            }
            app.mode_switch = Some((card, profile.clone(), std::time::Instant::now()));
            audio::command(audio::Command::CardProfile(card, profile));
        }
        Message::Sink(name) => {
            // Choosing an output ends any mode switch's hold: the user's choice is what shows.
            app.mode_switch = None;
            app.audio.default_sink = name.clone();
            audio::command(audio::Command::DefaultSink(name));
        }
    }
    Task::none()
}

static PASSWORD_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("wifi-password"));

pub fn wifi_icon(n: &network::State) -> Icon {
    match n.link {
        network::Link::Wired => Icon::Cable,
        network::Link::Wifi => strength_icon(n.strength),
        network::Link::None if !n.wifi_enabled => Icon::WifiOff,
        network::Link::None => Icon::WifiZero,
    }
}

/// The signal as bars: what the radio actually hears, not a generic wave.
pub fn strength_icon(s: u8) -> Icon {
    match s {
        0..=15 => Icon::WifiZero,
        16..=35 => Icon::Wifi1,
        36..=55 => Icon::Wifi2,
        56..=75 => Icon::Wifi3,
        _ => Icon::Wifi,
    }
}

/// The speaker showing the volume, whatever the output (the AirPods have their own circle,
/// so headphones are not drawn again here).
pub fn level_icon(d: Option<&audio::Device>) -> Icon {
    match d {
        None => Icon::VolumeX,
        Some(d) if d.muted || d.volume <= 0.001 => Icon::VolumeX,
        Some(d) if d.volume < 0.34 => Icon::Volume,
        Some(d) if d.volume < 0.67 => Icon::Volume1,
        Some(_) => Icon::Volume2,
    }
}

pub fn volume_icon(d: Option<&audio::Device>) -> Icon {
    match d {
        None => Icon::VolumeX,
        Some(d) if d.muted || d.volume <= 0.001 => Icon::VolumeX,
        Some(d) if d.headphones => Icon::Headphones,
        Some(d) if d.volume < 0.34 => Icon::Volume,
        Some(d) if d.volume < 0.67 => Icon::Volume1,
        Some(_) => Icon::Volume2,
    }
}

/// The battery as it is: Material's horizontal battery filled to its charge, the bolt while
/// charging, the alert when nearly empty.
pub fn battery_icon(p: &power::State) -> Icon {
    if (p.charging || p.held) && p.percent < 99.5 {
        return Icon::BatteryFrameBolt;
    }
    match p.percent {
        x if x < 8.0 => Icon::BatteryFrameAlert,
        x if x < 22.0 => Icon::BatteryFrame1,
        x if x < 36.0 => Icon::BatteryFrame2,
        x if x < 50.0 => Icon::BatteryFrame3,
        x if x < 64.0 => Icon::BatteryFrame4,
        x if x < 78.0 => Icon::BatteryFrame5,
        x if x < 92.0 => Icon::BatteryFrame6,
        _ => Icon::BatteryFrameFull,
    }
}

pub fn duration(secs: i64) -> String {
    let m = secs / 60;
    if m >= 60 {
        format!("{} h {} min", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let body = match app.panels.control.page {
        Page::Main => main_page(app, s),
        Page::Wifi => wifi_page(app, s),
        Page::Bluetooth => bt_page(app, s),
        Page::Output => output_page(app, s),
        Page::NightLight => night_page(app, s),
        Page::Quiet => quiet_page(app, s),
        Page::Power => power_page(app, s),
        Page::Warp => warp_page(app, s),
        Page::Tailscale => tailscale_page(app, s),
        Page::Network => network_page(app, s),
        Page::Hidden => hidden_page(app, s),
    };
    // Never past the screen: at most the height under the bar (a few tall notifications, a
    // screenshot's picture, ran the panel off the bottom edge); taller pages scroll.
    let room = app
        .panel
        .as_ref()
        .and_then(|p| app.output(p.output))
        .map_or(f32::INFINITY, |o| {
            o.size.height - crate::theme::size::BAR - 2.0 * tok::XL
        });
    // The main page keeps its tiles still and scrolls only its notifications (`main_page`);
    // other pages scroll whole when taller than the screen.
    if app.panels.control.page == Page::Main {
        return container(body).padding(tok::L).into();
    }
    container(crate::ui::smooth(
        iced::widget::scrollable(container(body).padding(tok::L))
            .height(Length::Shrink)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(0)
                    .scroller_width(0),
            )),
    ))
    .max_height(room)
    .into()
}

/// Keys on the main page: arrows move between tiles (two per row), Space flips the tile,
/// Enter opens its page (or flips it when it has none). On a page, Left goes back.
pub fn key(app: &mut App, k: super::Key) -> Task<AppMessage> {
    use super::Key;
    if app.panels.control.page != Page::Main {
        if k == Key::Left {
            return update(app, Message::Page(Page::Main));
        }
        return Task::none();
    }
    let list = tiles(app);
    let n = list.len();
    if n == 0 {
        return Task::none();
    }
    let cur = app.panels.control.focus;
    let at = cur.unwrap_or(0);
    let to = match k {
        // Enter goes in (a tile's page), Space flips it.
        Key::Enter if cur.is_some() => {
            let t = &list[at];
            return Task::done(t.details.clone().unwrap_or_else(|| t.press.clone()));
        }
        Key::Space if cur.is_some() => return Task::done(list[at].press.clone()),
        _ if cur.is_none() => 0,
        Key::Right | Key::Tab => (at + 1).min(n - 1),
        Key::Left | Key::BackTab => at.saturating_sub(1),
        Key::Down => (at + 2).min(n - 1),
        Key::Up => at.saturating_sub(2),
        Key::Home => 0,
        Key::End => n - 1,
        _ => return Task::none(),
    };
    app.panels.control.focus = Some(to);
    Task::none()
}

#[cfg(test)]
mod tests {
    use super::short_names;

    #[test]
    fn strips_the_shared_controller_name() {
        let n = short_names(
            [
                "Alder Lake PCH-P High Definition Audio Controller Speaker",
                "Alder Lake PCH-P High Definition Audio Controller HDMI / DisplayPort 1 Output",
                "AirPods",
            ]
            .into_iter(),
        );
        assert_eq!(n, vec!["Speaker", "HDMI / DisplayPort 1 Output", "AirPods"]);
    }
}

#[cfg(test)]
mod mode_tests {
    #[test]
    fn modes_name_their_codec() {
        assert_eq!(
            super::mode_name("High Fidelity Playback (A2DP Sink, codec AAC)"),
            "High quality · AAC"
        );
        assert_eq!(
            super::mode_name("Headset Head Unit (HSP/HFP, codec LC3-24kHz)"),
            "Calls · LC3"
        );
        assert_eq!(
            super::mode_name("High Fidelity Playback (A2DP Sink)"),
            "High quality"
        );
    }
}

/// Keeps `order` (a list's rows as shown, by key) for the ones still there and adds new ones
/// at the end, in the service's order: a list that is open never moves a row under the pointer.
pub fn keep_order<'a>(order: &mut Vec<String>, now: impl Iterator<Item = &'a str> + Clone) {
    order.retain(|k| now.clone().any(|n| n == k));
    for n in now {
        if !order.iter().any(|k| k == n) {
            order.push(n.to_string());
        }
    }
}

/// `items` in `order`'s order (ones not in it last, as they came).
pub fn in_order<'a, T>(items: &'a [T], order: &[String], key: impl Fn(&T) -> &str) -> Vec<&'a T> {
    let mut v: Vec<&T> = items.iter().collect();
    v.sort_by_key(|i| order.iter().position(|k| k == key(i)).unwrap_or(usize::MAX));
    v
}
