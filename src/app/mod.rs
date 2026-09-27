//! The application: per-monitor surfaces, shared state, and the message loop.

use crate::config::{self, Config};
use crate::motion::{Clock, Spring, range};
use crate::panels::{self, PanelKind};
use crate::services::{
    self, audio, bluetooth, hyprland, mpris, network, notifications, power, toggles, tray, vpn,
};
use crate::shell;
use crate::theme::{Scheme, Seed, tok};
use crate::ui::Icon;
use crate::ui::shape::Edge;
use iced::task::{
    InputRegionRect, destroy_layer_surface, new_layer_surface, set_exclusive_zone,
    set_input_region, set_keyboard_interactivity, set_margin, set_size,
};
use iced::{
    Anchor, Element, KeyboardInteractivity, Layer, LayerShellSettings, OutputEvent, OutputId,
    Rectangle, Size, Subscription, SurfaceId, Task, Theme,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod alerts;
mod change;
mod commands;
mod dispatch;
mod frames;
mod listen;
mod panel_flow;
mod reload;
mod surfaces;
mod system;
mod wallpaper;
#[allow(unused_imports)]
use alerts::*;
pub use change::Change;
#[allow(unused_imports)]
pub(crate) use commands::*;
#[allow(unused_imports)]
use dispatch::*;
#[allow(unused_imports)]
use frames::*;
#[allow(unused_imports)]
pub(crate) use listen::*;
#[allow(unused_imports)]
use panel_flow::*;
#[allow(unused_imports)]
use reload::*;
#[allow(unused_imports)]
use surfaces::*;
#[allow(unused_imports)]
use system::*;
#[allow(unused_imports)]
use wallpaper::*;

/// A panel surface's size and margins (top, right, bottom, left).
pub type PanelGeom = ((u32, u32), (i32, i32, i32, i32));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Wallpaper,
    Frame(Edge),
    Reserve,
    Bar,
    /// Panels, toasts and pop-ups (full output; input only where they are).
    Overlay,
    /// An open panel on its own surface, exactly its size (floating style): Hyprland animates
    /// it in and out with its blur, as macOS and Windows animate their layers.
    Panel,
}

pub struct Output {
    pub id: OutputId,
    info: iced::OutputInfo,
    pub name: String,
    pub size: Size,
    pub overlay: SurfaceId,
    surfaces: Vec<SurfaceId>,
    /// Space reservations: top, left, right, bottom. Their exclusive zones follow the frame
    /// in effect, so game mode and the pill style change them in place.
    reserves: Vec<SurfaceId>,
    /// Size, input region and keyboard mode last applied to the bar surface.
    plan: Option<Plan>,
    /// Workspace pill position and width.
    /// The current-workspace highlight's left and right edges (separate springs: the edge
    /// facing the move leads, the other follows, so it stretches and settles like liquid).
    pub pill_x: Spring,
    pub pill_r: Spring,
    /// The island's shown width, springing towards its content's natural width.
    pub island_w: Spring,
}

/// A panel hanging from the bar of one output.
pub struct Panel {
    pub kind: PanelKind,
    pub output: OutputId,
    pub open: bool,
    pub x: Spring,
    pub w: Spring,
    /// Height: follows the content (a page inside the panel changing size), never the fade.
    pub h: Spring,
    /// Visibility, 0 to 1: only opening and closing move it, so a size change inside an open
    /// panel can never make it flicker.
    pub t: Spring,
}

impl Panel {
    /// How far the panel has opened, 0 (hidden) to 1 (shown).
    pub fn progress(&self, _natural: f32) -> f32 {
        self.t.value.clamp(0.0, 1.0)
    }

    /// Content opacity: in once the shape has clearly started growing, out early on close so
    /// an empty card never shrinks with stale content in it.
    pub fn alpha(&self, natural: f32) -> f32 {
        let p = self.progress(natural);
        if self.open {
            range(p, 0.05, 0.55)
        } else {
            range(p, 0.4, 0.9)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Plan {
    region: Option<Vec<(i32, i32, i32, i32)>>,
    keyboard: u8,
}

/// A notification shown briefly under the bar.
pub struct Toast {
    pub id: u32,
    pub until: Instant,
    /// How long it shows (its time starts again in full after the pointer leaves it).
    pub secs: Duration,
}

/// On-screen display for a level change.
#[derive(Clone, Debug)]
pub struct Osd {
    pub icon: Icon,
    pub value: f32,
    /// A state instead of a level (e.g. "Caps Lock on").
    pub text: Option<String>,
    /// A second line under the text (e.g. "Connected · 80%").
    pub sub: Option<String>,
    pub until: Instant,
    /// What a click on it in the island does (opens where the level is set, say).
    pub action: Option<Message>,
}

/// A moment the island shows instead of its usual content (something was copied, a
/// screenshot taken), then springs back.
pub struct Flash {
    pub icon: Icon,
    pub title: String,
    pub detail: String,
    pub thumb: Option<iced::widget::image::Handle>,
    pub until: Instant,
    /// What a click on it does: open the screenshot or recording, open the clipboard, stop
    /// the recording.
    pub action: Option<Message>,
}

const FLASH_MS: u64 = 2200;
/// In `panel_surface`'s keyboard byte: the surface was just made and not yet laid out.
const FRESH: u8 = 0x80;

/// A drop that is not a panel: it springs open to its measured height and closes to zero.
pub struct Sheet {
    pub h: Spring,
    pub natural: f32,
}

impl Sheet {
    fn new() -> Self {
        Self {
            h: Spring::new(0.0),
            natural: 0.0,
        }
    }
    pub fn alpha(&self) -> f32 {
        if self.natural <= 0.0 {
            0.0
        } else {
            range(self.h.value / self.natural, 0.4, 0.9)
        }
    }
    pub fn visible(&self) -> bool {
        self.h.value > 0.5 || self.h.target > 0.5
    }
}

pub const TOAST_WIDTH: f32 = 380.0;
pub const OSD_WIDTH: f32 = 248.0;
const OSD_MS: u64 = 1400;

pub struct Wallpaper {
    pub handle: iced::widget::image::Handle,
}

pub struct App {
    pub config: Config,
    pub scheme: Scheme,
    /// The wallpaper's colour seed (the accent comes from it unless one is chosen).
    pub seed: crate::theme::Seed,
    pub outputs: Vec<Output>,
    roles: HashMap<SurfaceId, (OutputId, Role)>,
    /// Settings, an ordinary window (moved, tiled, closed like any app) while open.
    pub settings_window: Option<SurfaceId>,
    pub hypr: hyprland::State,
    pub now: chrono::DateTime<chrono::Local>,
    pub wallpaper: Option<Arc<Wallpaper>>,
    /// The first wallpaper is being read: its surface waits for it (no empty first frame, which
    /// cost a second full-screen buffer for good).
    pub wallpaper_pending: bool,
    /// The wallpaper is on screen and kestrel's copies of it were let go (the compositor keeps
    /// the picture it was given): its surface is not drawn again until the picture is read

    /// The pop-up under the pointer: its ✕ shows and its time stands still.
    pub toast_hover: Option<u32>,
    /// The apps using a camera (shown by the privacy indicator).
    pub camera_apps: Vec<String>,
    /// The in-use capsule: what it last showed, and its strength (1 shown; on the way to 0 it
    /// fades and narrows away instead of vanishing).
    pub privacy_glyphs: Vec<Icon>,
    /// A Bluetooth mode switch the user asked for (card, mode, when): until that card's output is
    /// back and in use, the Sound page keeps showing it (see `audio_changed`).
    pub mode_switch: Option<(u32, String, Instant)>,
    pub privacy_fade: Spring,
    /// The capsule's width, springing when an icon joins or leaves (no jump).
    pub privacy_w: Spring,
    /// Camera and microphone use that ended in the last 10 minutes: (device, app, when).
    pub privacy_recent: Vec<(Icon, String, Instant)>,
    pub panel: Option<Panel>,
    /// The open panel's own surface (floating style), its kind, and the keyboard mode last
    /// given to it.
    pub panel_surface: Option<(SurfaceId, PanelKind, u8)>,
    /// The size and margins last given to it.
    pub panel_geom: Option<PanelGeom>,
    pub natural: HashMap<PanelKind, Size>,
    pub panels: panels::State,
    pub clock: Clock,
    pub audio: audio::State,
    /// Live levels while the Sound page is open.
    pub meters: crate::services::meters::Meters,
    pub net: network::State,
    pub bt: bluetooth::State,
    /// A network or device being connected or disconnected (panels show it in progress).
    pub connecting: Option<panels::cx::Connecting>,
    /// The last device that could not be connected or paired, and why (shown beside it).
    pub device_error: Option<(String, String)>,
    /// The device being paired (its path).
    pub pairing: Option<String>,
    /// Who asked kestrel's own file question (`Action::AskFile`): the chosen file goes there.
    pub file_answer: Option<Box<dyn Fn(std::path::PathBuf) -> Message>>,
    /// A network whose last join failed, and why (shown under its name).
    pub net_error: Option<(String, String)>,
    pub power: power::State,
    pub brightness: Option<f32>,
    pub media: mpris::State,
    /// Players whose reported length grows while the same thing plays (a browser streaming in
    /// pieces reports the buffered length): their timeline is not shown, being wrong.
    pub length_unreliable: std::collections::HashSet<String>,
    pub tray: tray::State,
    pub notifications: Vec<notifications::Notification>,
    /// The history's fingerprint as last saved (`history`).
    pub notes_saved: u64,
    pub unread: bool,
    /// Notifications that arrived since the centre was last opened (the bell's badge).
    pub unseen: usize,
    /// The low-battery warning shown (its id), withdrawn when the charger is plugged in.
    pub battery_note: Option<u32>,
    /// Start of the shell's life, for time-based pulses.
    pub dnd: bool,
    pub night_light: bool,
    /// Software dimming of screens without a settable backlight, 0.3 to 1.
    pub dim: f32,
    /// External monitors that answer DDC/CI, and their brightness (0 to 1) by i2c bus.
    pub ddc: Vec<services::ddc::Display>,
    pub ddc_level: HashMap<u32, f32>,
    pub caffeine: bool,
    pub game_mode: bool,
    pub vpn: vpn::State,
    pub system: services::system::State,
    pub updates: services::updates::State,
    pub weather: Option<services::web::Weather>,
    /// What the night light schedule last decided (it acts only when that changes, so a
    /// switch by hand holds until the next sunset or sunrise).
    pub night_auto: Option<bool>,
    /// What the quiet hours last decided (same rule as `night_auto`).
    pub quiet_auto: Option<bool>,
    /// The last recording announced as saved (each file once).
    pub saved_recording: Option<String>,
    /// The recording whose start was announced (each once).
    pub started_recording: Option<String>,
    /// The connected AirPods' charge and fit (Apple's accessory protocol).
    pub pods: Option<services::airpods::Pods>,
    /// AirPods just connected: show their charge in the island when it first arrives.
    pub pods_fresh: bool,
    /// The open panel was opened from the keyboard, so it takes the keys.
    pub panel_keys: bool,
    /// Each player's artwork as shown (settled), and as it last reported it.
    pub settled_art: HashMap<String, Option<String>>,
    pub reported_art: HashMap<String, Option<String>>,
    /// The last config.toml mistake reported (so it is said once).
    pub config_error: Option<String>,
    /// `[plugins]` in config.toml as last seen (plugins reload when it changes).
    pub plugin_settings: String,
    /// The tray's hidden apps are slid out in the bar.
    pub tray_expanded: bool,
    /// The Lua layer: its bar layout and what each of its widgets shows.
    pub lua_bar: crate::lua::Bar,
    pub chips: HashMap<String, crate::lua::Chip>,
    pub flash: Option<Flash>,
    /// The music visualiser's bars: the last few loudness readings, newest first.
    /// The visualiser's bars as drawn, 0 to 1.
    pub levels: [f32; 4],
    /// The loudest band now, and as last shown.
    pub envelope: f32,
    pub loudness: f32,
    /// Each band's level as read, smoothed: where the bars are heading.
    pub level_targets: [f32; 4],
    /// Time running through the bars' motion.
    pub level_phase: f32,
    pub level_last: Option<Instant>,
    pub toasts: Vec<Toast>,
    pub toast_sheet: Sheet,
    /// The island's width while a notification grows it (from the pill's to the banner's).
    pub toast_w: Spring,
    /// The notification the grown island shows last: it stays drawn while the island shrinks
    /// back (the box went blank for its last frames).
    pub toast_last: Option<u32>,
    pub osd: Option<Osd>,
    pub osd_sheet: Sheet,
}

#[derive(Clone, Debug)]
pub enum Message {
    Output(OutputEvent),
    Hypr(hyprland::State),
    Minute,
    /// A whole second (only while the clock shows seconds): the time, nothing else.
    Second,
    Frame(Instant),
    WallpaperLoaded(Option<(Seed, Arc<Wallpaper>)>),
    /// Give freed memory back to the system (after big pictures were let go).
    Trim,
    /// Open (or close, if already open) a panel under the given button.
    Toggle(PanelKind, OutputId, Rectangle),
    /// A bar button for one part of the control centre: opens it on that page (or closes it,
    /// when it is open there already).
    ControlAt(panels::control::Page, OutputId, Rectangle),
    /// Opens Settings on a page.
    SettingsAt(panels::settings::Page),
    /// Opens Settings > Windows on the rule for an app (its window class).
    RuleFor(String),
    Close,
    Measured(PanelKind, Size),
    Dispatch(crate::services::compositor::Action),
    /// A device connected, disconnected, paired or forgotten as asked, or why not.
    DeviceDone(String, Result<(), String>),
    /// A change to the system (panels, the bar's buttons, Settings, IPC all ask this way).
    Change(Change),
    /// A join ended: the network and how.
    NetDone(String, Result<(), String>),
    /// Ends the screen recording shown (the recorder in Hyprland's state when pressed).
    StopRecording,
    Panel(panels::Message),
    Ipc(String),
    Audio(audio::State),
    Net(Box<network::State>),
    Bt(bluetooth::State),
    Power(power::State),
    Media(mpris::State),
    Tray(tray::State),
    Notification(notifications::Event),
    MeasuredToasts(Size),
    MeasuredOsd(Size),
    /// Open a tray item's menu (under the icon, when its bounds are known).
    /// Open the tray panel on an item's menu, or (None) on the list of every item.
    /// Slide the tray's hidden apps out in the bar, or tuck them away.
    TrayExpand,
    TrayMenu(Option<tray::Item>, OutputId, Option<Rectangle>),
    /// Scroll on the status cluster.
    VolumeStep(f32),
    Expire(Instant),
    /// A frame of the slow pulse on something in progress.
    Breathe,
    /// The island's content changed width on an output.
    IslandMeasured(OutputId, f32),
    /// Several messages in order (e.g. focus a window, then close the panel).
    Batch(Vec<Message>),
    /// systemd's watchdog: the update loop is alive (watchdog.rs).
    Alive,
    Vpn(vpn::State),
    /// Caps Lock / Num Lock state after the key: (caps, num), and which key it was.
    Locks(bool, bool, bool),
    System(services::system::State),
    Updates(services::updates::State),
    Weather(services::web::Weather),
    Pods(Option<services::airpods::Pods>),
    /// config.toml or init.lua was saved.
    ConfigFile(crate::subs::ConfigFile),
    Lua(crate::lua::Event),
    LuaClick(String),
    Copied(services::clipboard::Copied),
    Meters(crate::services::meters::Meters),
    Ddc(Vec<services::ddc::Display>),
    DdcLevel(u32, Option<f32>),
    /// A player's new artwork has stayed put long enough to show.
    ArtSettled(String, Option<String>),
    /// An app asks which program should open something ("Open with…").
    OpenWith(services::appchooser::Event),
    /// An app asks for a file (the portal's file chooser).
    FileRequest(services::filechooser::Request),
    Polkit(services::polkit::Event),
    /// A window was closed by the user (Settings).
    WindowClosed(iced::window::Id),
    /// A Bluetooth pairing question (or its end), from kestrel's BlueZ agent.
    BtAsk(services::bluetooth::Ask),
    /// NetworkManager needs a Wi-Fi password (kestrel is its secret agent).
    WifiAsk(services::network::Ask),
    /// A keyring's password question (or its end), from kestrel's system prompter.
    KeyringAsk(services::keyring::Ask),
    /// The apps using a camera changed.
    Camera(Vec<String>),
    Level(#[allow(dead_code)] crate::services::level::Bands),
    /// The visualiser's own frame clock (60 a second while music plays).
    VisTick(Instant),
    ReloadWallpaper,
}

impl std::fmt::Debug for Wallpaper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wallpaper")
    }
}

pub fn boot() -> (App, Task<Message>) {
    let config = config::load();
    crate::theme::set_glass(config.material != config::Material::Black);
    let mut app = App {
        scheme: Scheme::default(),
        seed: crate::theme::Seed::default(),
        outputs: Vec::new(),
        roles: HashMap::new(),
        hypr: hyprland::State::default(),
        now: chrono::Local::now(),
        night_auto: None,
        settings_window: None,
        quiet_auto: None,
        saved_recording: None,
        started_recording: None,
        wallpaper: None,
        panel: None,
        panel_surface: None,
        panel_geom: None,
        natural: HashMap::new(),
        panels: panels::State::default(),
        clock: Clock::new(),
        audio: audio::State::default(),
        meters: crate::services::meters::Meters::default(),
        net: network::State::default(),
        bt: bluetooth::State::default(),
        connecting: None,
        device_error: None,
        pairing: None,
        file_answer: None,
        net_error: None,
        power: power::State::default(),
        brightness: None,
        media: mpris::State::default(),
        length_unreliable: std::collections::HashSet::new(),
        tray: tray::State::default(),
        notifications: crate::history::load(),
        notes_saved: 0,
        unread: false,
        unseen: 0,
        battery_note: None,
        // Kestrel's own switches come back as they were (state.toml); see `boot`.
        dnd: crate::state::get(|s| s.toggles.dnd),
        night_light: toggles::night_light() || crate::state::get(|s| s.toggles.night_light),
        dim: toggles::dim_level(),
        ddc: Vec::new(),
        ddc_level: HashMap::new(),
        caffeine: crate::state::get(|s| s.toggles.caffeine),
        // Game mode survives a shell restart (Hyprland keeps its effects off meanwhile).
        game_mode: {
            let on = game_mode_flag().is_file();
            crate::motion::set_still(on);
            on
        },
        vpn: vpn::State::default(),
        system: services::system::State::default(),
        updates: services::updates::State::default(),
        weather: services::web::cached_weather(),
        pods: None,
        pods_fresh: false,
        panel_keys: true,
        settled_art: HashMap::new(),
        reported_art: HashMap::new(),
        config_error: None,
        plugin_settings: config::plugin_settings(),
        tray_expanded: false,
        lua_bar: crate::lua::Bar::default(),
        chips: HashMap::new(),
        flash: None,
        levels: [0.0; 4],
        envelope: 0.0,
        loudness: 0.0,
        level_targets: [0.0; 4],
        level_phase: 0.0,
        level_last: None,
        toasts: Vec::new(),
        toast_sheet: Sheet::new(),
        toast_w: Spring::new(0.0),
        toast_last: None,
        osd: None,
        osd_sheet: Sheet::new(),
        wallpaper_pending: config.wallpaper.is_some(),
        toast_hover: None,
        camera_apps: Vec::new(),
        privacy_glyphs: Vec::new(),
        mode_switch: None,
        privacy_fade: Spring::new(0.0),
        privacy_w: Spring::new(0.0),
        privacy_recent: Vec::new(),
        config,
    };
    let load = load_wallpaper(app.config.wallpaper.clone());
    // Monitors whose brightness can really be set (DDC/CI), found once.
    let ddc = Task::perform(services::ddc::displays(), Message::Ddc);
    let emoji = panels::emoji::warm();
    // Put back what was on before the restart: the inhibitor belonged to the old process; the
    // tint runs as its own service and is only told again (or started, if it was not).
    let night = if app.night_light {
        let k = app.config.night_light_temperature;
        Task::future(toggles::set_night_light(true, k)).discard()
    } else {
        Task::none()
    };
    let caffeine = if app.caffeine {
        Task::future(toggles::set_caffeine(true)).discard()
    } else {
        Task::none()
    };
    // The clipboard history, read once now so its panel opens on it at once.
    let clips = Task::perform(panels::clipboard::clip_list(), |c| {
        Message::Panel(panels::Message::Clipboard(
            panels::clipboard::Message::Loaded(c),
        ))
    });
    // A cursor size chosen in Settings: put back over the session's own (set before kestrel).
    let cursor = match app.config.cursor_size {
        Some(n) => Task::future(crate::hypr_settings::apply_cursor(n)).discard(),
        None => Task::none(),
    };
    // The wallpaper picker's thumbnails and the shortcuts list, read now so their first open
    // is already full.
    let warm = Task::batch([
        panels::wallpaper::preload(&app.config)
            .map(|m| Message::Panel(panels::Message::Wallpaper(m))),
        // The launcher's apps read once at start, so even its first open is instant.
        panels::launcher::reload().map(|m| Message::Panel(panels::Message::Launcher(m))),
        panels::action::perform(&mut app, panels::keys::open(), |m| {
            Message::Panel(panels::Message::Keys(m))
        }),
    ]);
    (
        app,
        Task::batch([load, ddc, caffeine, night, clips, cursor, warm, emoji]),
    )
}

impl App {}

/// The bar surface's height: the bar and room for the frame's corners under it.
const BAR_SURFACE: f32 = tok::BAR + 32.0;

pub fn update(app: &mut App, msg: Message) -> Task<Message> {
    // A part that failed while drawing is reported once.
    for failure in crate::guard::take_unreported() {
        app.notify(notifications::Event::Added(own_notification(
            "Part of kestrel stopped",
            &format!("{failure}. The rest keeps working; restart kestrel to try it again."),
        )));
    }
    if log::log_enabled!(log::Level::Debug) {
        let name = format!("{msg:?}");
        log::debug!("msg {}", name.split(['(', ' ', '{']).next().unwrap_or(""));
    }
    let (cam_before, mic_before) = (app.camera_apps.clone(), app.audio.recording.clone());
    let was_animating = app.animating();
    let task = crate::guard::isolate("Handling an event", || update_inner(app, msg), Task::none);
    app.track_privacy(&cam_before, &mic_before);
    // Frames come from the compositor's frame callbacks, which only follow a drawn change. A
    // spring starting from nothing on screen (the privacy capsule appearing) changes nothing, so
    // no frame ever came and it never moved. Its first step comes as a message: handled after
    // this update has added the frame subscription, so the redraw it causes is heard and the
    // frames follow from there.
    let task = if !was_animating && app.animating() {
        Task::batch([task, Task::done(Message::Frame(Instant::now()))])
    } else {
        task
    };
    // Banners are views of notifications: one whose notification went (dismissed, cleared,
    // closed by its app, pushed out by the cap of 50) goes with it, and with none left the sheet
    // closes. Each removal path had to remember this; three did not, and an empty banner frame
    // stayed under the bar.
    // The history kept across restarts: saved whenever it changed.
    let fp = crate::history::fingerprint(&app.notifications);
    if fp != app.notes_saved {
        if app.notes_saved != 0 {
            crate::history::save(&app.notifications);
        }
        app.notes_saved = fp;
    }
    let before = app.toasts.len();
    let notes = &app.notifications;
    app.toasts.retain(|t| notes.iter().any(|n| n.id == t.id));
    if app.toasts.len() != before || app.toasts.is_empty() {
        app.settle_toasts();
    }
    // The open panel's own surface follows the state after every update, whatever path opened,
    // moved or ended the panel (paths that set `panel = None` left their surface behind).
    let panel_surface = app.sync_panel_surface();
    let surfaces = app.sync_surfaces();
    Task::batch([task, panel_surface, surfaces])
}

pub fn view(app: &App, id: SurfaceId) -> Element<'_, Message> {
    if app.settings_window == Some(id) {
        return panels::settings_window(app);
    }
    let Some(&(output, role)) = app.roles.get(&id) else {
        return iced::widget::Space::new().into();
    };
    let Some(o) = app.output(output) else {
        return iced::widget::Space::new().into();
    };
    match role {
        Role::Wallpaper => shell::wallpaper(app),
        Role::Frame(edge) => shell::frame(app, edge),
        Role::Reserve => iced::widget::Space::new().into(),
        Role::Bar => shell::bar::view(app, o),
        Role::Overlay => shell::bar::overlay(app, o),
        Role::Panel => shell::bar::panel_surface(app, o),
    }
}

/// Static surfaces (wallpaper, frame, reserves) only redraw when what they show changes; the
/// bar always does.
pub fn redraw_key(app: &App, id: SurfaceId) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    if app.settings_window == Some(id) {
        return None;
    }
    // A surface with no role (the 1x1 placeholder) always shows empty space.
    let Some((_, role)) = app.roles.get(&id) else {
        return Some(0);
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let base = app.scheme.base;
    let colour = [base.r, base.g, base.b].map(f32::to_bits);
    match role {
        Role::Bar | Role::Panel => return None,
        Role::Overlay => {
            // Idle: nothing to draw, so nothing is repainted.
            match app.outputs.iter().find(|o| o.overlay == id) {
                Some(o) if shell::bar::overlay_busy(app, o) => return None,
                _ => 1u8.hash(&mut h),
            }
        }
        Role::Reserve => 0u8.hash(&mut h),
        Role::Frame(_) => {
            colour.hash(&mut h);
            app.frame().thickness.to_bits().hash(&mut h);
            app.frame().radius.to_bits().hash(&mut h);
        }
        Role::Wallpaper => {
            if app.wallpaper.is_none() && app.wallpaper_pending {
                return Some(iced::NOT_READY);
            }
            colour.hash(&mut h);
            app.wallpaper
                .as_ref()
                .map(|w| Arc::as_ptr(w) as usize)
                .hash(&mut h);
        }
    }
    Some(h.finish())
}

pub fn theme(_app: &App, _id: SurfaceId) -> Theme {
    Theme::Dark
}
