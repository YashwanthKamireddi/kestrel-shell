//! Control centre: quick toggles, levels, notifications, and pages for Wi-Fi, Bluetooth,
//! sound output, night light, power mode, Cloudflare WARP and Tailscale.

use super::action::Action as Act;
use super::cx::{Cx, Light};
use crate::app::Change;
use crate::services::{audio, network, power, toggles};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::slider::slider;
use crate::ui::{
    self, Icon, card, empty, icon, icon_button, label, label_w, list_row, subpage_header, switch,
    tile,
};
use iced::widget::{Space, container, row, text_input};
use iced::{Alignment, Element, Length, Task};

type Action = Act<Message>;

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
    /// The Sound page's app whose device list is open.
    sound_open: Option<String>,
    /// The device whose Forget was pressed once (asks before removing).
    pub forget: Option<String>,
    /// The Wi-Fi network whose details page is open.
    network: Option<String>,
    /// Its Forget was pressed once.
    forget_net: bool,
    /// A hidden network being typed in: name and password.
    pub hidden: (String, String),
}

impl State {
    /// The Wi-Fi networks or nearby devices changed: the lists keep their rows where they
    /// are; found ones join at the end (called by the shell after each report).
    pub fn lists_changed(&mut self, net: &network::State, bt: &crate::services::bluetooth::State) {
        keep_order(
            &mut self.wifi_order,
            net.networks.iter().map(|n| n.ssid.as_str()),
        );
        keep_order(
            &mut self.nearby_order,
            bt.nearby.iter().map(|d| d.path.as_str()),
        );
    }
}

/// Whether the control centre is waiting for typing (a Wi-Fi password).
pub fn typing(st: &State) -> bool {
    st.password.is_some() || st.page == Page::Hidden
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
    /// Read WARP's or the tailnet's state again (after a change to it).
    ReadWarp,
    ReadTailnet,
    /// Start a tray app (a click on its icon), or open its menu (a right click).
    TrayActivate(crate::services::tray::Item),
    TrayMenu(crate::services::tray::Item),
    /// A header button: open a panel by name ("settings", "capture", "wallpaper").
    Toggle(&'static str),
    Lock,
}

/// Night light's range, warmest to coolest (kelvin).
pub const WARMEST: f32 = 2500.0;
pub const COOLEST: f32 = 6000.0;

fn read_warp() -> Action {
    Act::Run(Task::perform(
        crate::services::vpn::warp(),
        Message::WarpInfo,
    ))
}

fn read_tailnet() -> Action {
    Act::Run(Task::perform(
        crate::services::vpn::tailnet(),
        Message::Tailnet,
    ))
}

pub fn update(st: &mut State, m: Message, cx: &Cx<'_>) -> Action {
    let change = Act::Change;
    match m {
        Message::Page(p) => {
            st.page = p;
            st.password = None;
            match p {
                Page::Bluetooth => st.nearby_order.clear(),
                Page::Wifi => {
                    st.wifi_order.clear();
                    keep_order(
                        &mut st.wifi_order,
                        cx.net.networks.iter().map(|n| n.ssid.as_str()),
                    );
                    return Act::Run(
                        Task::future(network::command(network::Command::Scan)).discard(),
                    );
                }
                Page::Hidden => {
                    return Act::Run(Task::Iced(iced::widget::focus(iced::widget::Id::new(
                        "hidden-name",
                    ))));
                }
                Page::Tailscale => return read_tailnet(),
                Page::Warp => return read_warp(),
                _ => {}
            }
            Act::None
        }
        // Warmth follows the slider at once (night light's own tool); kept when let go.
        Message::Warmth(v) => {
            let k = (WARMEST + v.clamp(0.0, 1.0) * (COOLEST - WARMEST)).round() as u32;
            let k = k / 100 * 100;
            st.kelvin = Some(k);
            Act::Run(Task::future(toggles::set_temperature(k)).discard())
        }
        Message::WarmthDone => match st.kelvin.take() {
            Some(k) => change(Change::NightTemperature(k)),
            None => Act::None,
        },
        Message::Schedule(on) => change(Change::NightSchedule(on)),
        Message::SetProfile(p) => change(Change::Profile(p)),
        Message::Tailnet(t) => {
            st.tailnet = t;
            Act::None
        }
        Message::WarpInfo(w) => {
            st.warp = w;
            Act::None
        }
        Message::ReadWarp => read_warp(),
        Message::ReadTailnet => read_tailnet(),
        Message::WarpMode(m) => {
            if let Some(w) = &mut st.warp {
                w.mode = m.clone();
            }
            Act::Run(Task::perform(
                async move {
                    crate::services::vpn::warp_mode(m).await;
                    crate::services::vpn::warp().await
                },
                Message::WarpInfo,
            ))
        }
        Message::Tailscale2(args) => {
            st.ts_error = None;
            Act::Run(Task::perform(
                crate::services::vpn::tailscale(args),
                Message::TailscaleDone,
            ))
        }
        Message::TailscaleDone(r) => {
            st.ts_error = r.err();
            read_tailnet()
        }
        Message::Ping(ip) => {
            st.pings.insert(ip.clone(), "…".into());
            Act::Run(Task::perform(
                async move {
                    let r = crate::services::vpn::tailscale_ping(ip.clone()).await;
                    (ip, r)
                },
                |(ip, r)| Message::Pinged(ip, r),
            ))
        }
        Message::Pinged(ip, r) => {
            st.pings.insert(ip, r.unwrap_or_else(|| "no reply".into()));
            Act::None
        }
        Message::Login => Act::Run(Task::future(crate::services::vpn::tailscale_login()).discard()),
        Message::CopyIp(ip) => {
            Act::Run(Task::future(crate::panels::clipboard::copy_text(ip)).discard())
        }
        Message::Wifi(on) => change(Change::Wifi(on)),
        Message::Airplane => change(Change::Airplane),
        Message::Bluetooth(on) => change(Change::Bluetooth(on)),
        Message::Dnd => change(Change::Dnd),
        Message::DndFor(minutes) => {
            st.page = Page::Main;
            change(Change::DndFor(minutes))
        }
        Message::NightLight => change(Change::NightLight),
        Message::Caffeine => change(Change::Caffeine),
        Message::Profile => change(Change::NextProfile),
        Message::GameMode => change(Change::GameMode),
        // The page shows what WARP or Tailscale says once the change is done.
        Message::Warp(on) => {
            st.warp = None;
            Act::ChangeThen(Change::Warp(on), Message::ReadWarp)
        }
        Message::Tailscale(on) => {
            st.tailnet = None;
            Act::ChangeThen(Change::Tailscale(on), Message::ReadTailnet)
        }
        Message::SinkVolume(v) => change(Change::SinkVolume(v)),
        Message::SinkMute => change(Change::SinkMute),
        Message::SourceVolume(v) => change(Change::SourceVolume(v)),
        Message::SourceMute => change(Change::SourceMute),
        Message::Brightness(v) => change(Change::Brightness(v)),
        Message::BrightnessDone => Act::None,
        Message::DdcBrightness(v) => change(Change::DdcBrightness(v)),
        Message::Dim(v) => change(Change::Dim(v)),
        Message::Join(n) => {
            if n.active {
                return Act::None;
            }
            if let Some(c) = Change::join(&n) {
                return change(c);
            }
            // A new secured network: its password first (typed here).
            st.password = Some((n.ssid, n.ap, String::new()));
            Act::Run(Task::Iced(iced::widget::focus(PASSWORD_ID.clone())))
        }
        Message::Password(p) => {
            if let Some(pw) = &mut st.password {
                pw.2 = p;
            }
            Act::None
        }
        Message::SubmitPassword => match st.password.take() {
            Some((ssid, ap, pw)) if !pw.is_empty() => change(Change::Join(
                ssid.clone(),
                network::Command::ConnectNew {
                    ssid,
                    ap,
                    password: Some(pw),
                },
            )),
            _ => Act::None,
        },
        Message::NetworkInfo(ssid) => {
            st.network = Some(ssid);
            st.forget_net = false;
            st.password = None;
            st.page = Page::Network;
            Act::None
        }
        Message::ForgetNetwork => {
            if !st.forget_net {
                st.forget_net = true;
                return Act::None;
            }
            st.forget_net = false;
            match st.network.take() {
                Some(ssid) => {
                    st.page = Page::Wifi;
                    change(Change::ForgetNetwork(ssid))
                }
                None => Act::None,
            }
        }
        Message::AutoJoin(on) => match st.network.clone() {
            Some(ssid) => change(Change::AutoJoin(ssid, on)),
            None => Act::None,
        },
        Message::HiddenName(t) => {
            st.hidden.0 = t;
            Act::None
        }
        Message::HiddenPassword(t) => {
            st.hidden.1 = t;
            Act::None
        }
        Message::JoinHidden => {
            let (ssid, pw) = std::mem::take(&mut st.hidden);
            if ssid.trim().is_empty() {
                return Act::None;
            }
            st.page = Page::Wifi;
            change(Change::Join(
                ssid.clone(),
                network::Command::ConnectHidden {
                    ssid,
                    password: (!pw.is_empty()).then_some(pw),
                },
            ))
        }
        Message::Disconnect => change(Change::Disconnect),
        Message::Device(path, connected) => {
            st.forget = None;
            change(Change::Device {
                path,
                connect: !connected,
            })
        }
        Message::Pair(path) => change(Change::Pair(path)),
        Message::Forget(path) => {
            if st.forget.as_deref() != Some(path.as_str()) {
                st.forget = Some(path);
                return Act::None;
            }
            st.forget = None;
            change(Change::ForgetDevice(path))
        }
        Message::Source(name) => change(Change::Source(name)),
        Message::AppVolume(ids, v) => change(Change::AppVolume(ids, v)),
        Message::AppMute(ids) => change(Change::AppMute(ids)),
        Message::AppPick(key) => {
            st.sound_open = key;
            Act::None
        }
        Message::AppTo(ids, sink) => {
            st.sound_open = None;
            change(Change::AppTo(ids, sink))
        }
        Message::RecTo(ids, source) => {
            st.sound_open = None;
            change(Change::RecTo(ids, source))
        }
        Message::RecVolume(ids, v) => change(Change::RecVolume(ids, v)),
        Message::RecMute(ids) => change(Change::RecMute(ids)),
        Message::Balance(b) => change(Change::Balance(b)),
        Message::CardMode(card, profile) => change(Change::CardMode(card, profile)),
        Message::Sink(name) => change(Change::Sink(name)),
        Message::TrayActivate(item) => Act::Run(
            Task::future(crate::services::tray::activate(item.bus, item.path, false)).discard(),
        ),
        Message::TrayMenu(item) => Act::TrayMenu(item),
        Message::Toggle(name) => Act::Command(format!("toggle {name}")),
        Message::Lock => Act::Close.and(Act::Run(
            super::power::run(super::power::Action::Lock).discard(),
        )),
    }
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

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let body = match st.page {
        Page::Main => main_page(cx),
        Page::Wifi => wifi_page(st, cx),
        Page::Bluetooth => bt_page(st, cx),
        Page::Output => output_page(st, cx),
        Page::NightLight => night_page(st, cx),
        Page::Quiet => quiet_page(cx),
        Page::Power => power_page(cx),
        Page::Warp => warp_page(st, cx),
        Page::Tailscale => tailscale_page(st, cx),
        Page::Network => network_page(st, cx),
        Page::Hidden => hidden_page(st, cx),
    };
    // Never past the screen: at most the height under the bar (a few tall notifications, a
    // screenshot's picture, ran the panel off the bottom edge); taller pages scroll.
    let room = cx.screen.map_or(f32::INFINITY, |o| {
        o.height - crate::theme::tok::BAR - 2.0 * tok::XL
    });
    // The main page keeps its tiles still; other pages scroll whole when taller than the
    // screen.
    if st.page == Page::Main {
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

/// The control centre's own key: Left goes back from a page. Everything else is the shared
/// keyboard (Tab and arrows between tiles and rows, Enter and Space press): None.
pub fn key(st: &mut State, k: super::Key, cx: &Cx<'_>) -> Option<Action> {
    (st.page != Page::Main && k == super::Key::Left)
        .then(|| update(st, Message::Page(Page::Main), cx))
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
