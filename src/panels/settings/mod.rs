//! Settings: kestrel's config.toml without opening it. Every change is written into the file
//! in place (comments kept) and applies at once through the live reload, so this panel and the
//! file never disagree. Opened from the gear in the control centre, or `kestrel toggle
//! settings`.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::config::{self, BarStyle};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{self, Icon, control, icon, label, label_w};
use iced::widget::{column, container, row, text_input};
use iced::{Alignment, Color, Element, Length, Task};

// One file per page (its view and the helpers only it uses); this file keeps the state,
// messages, update, the window's frame and the pieces every page shares.
mod about;
mod access;
mod account;
mod appearance;
mod bar;
mod bluetooth;
mod datetime;
mod defaults;
mod developer;
mod displays;
mod keyboard;
mod mouse;
mod network;
mod notifications;
mod plugins;
mod power;
mod privacy;
mod shortcuts;
mod sound;
mod startup;
mod storage;
mod updates;
mod weather;
mod windows;
use about::*;
use access::*;
use account::*;
use appearance::*;
use bar::*;
use bluetooth::*;
use datetime::*;
use defaults::*;
use developer::*;
use displays::*;
use keyboard::*;
use mouse::*;
use network::*;
use notifications::*;
use plugins::*;
use power::*;
use privacy::*;
use shortcuts::*;
use sound::*;
use startup::*;
use storage::*;
use updates::*;
use weather::*;
use windows::*;

pub const WIDTH: f32 = 720.0;
const SIDEBAR: f32 = 196.0;

/// A page of settings, chosen in the sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Bar,
    Appearance,
    Time,
    Plugins,
    // The system's own settings.
    Network,
    Bluetooth,
    Account,
    Sound,
    Displays,
    Windows,
    Notifications,
    Privacy,
    Startup,
    Updates,
    Storage,
    Power,
    Access,
    Keyboard,
    Shortcuts,
    Mouse,
    Defaults,
    DateTime,
    Developer,
    About,
}

/// The shell's pages, then the system's (under their own heading in the sidebar).
const SHELL: [Page; 4] = [Page::Bar, Page::Appearance, Page::Time, Page::Plugins];
const SYSTEM: [Page; 20] = [
    Page::Network,
    Page::Bluetooth,
    Page::Account,
    Page::Sound,
    Page::Displays,
    Page::Windows,
    Page::Notifications,
    Page::Privacy,
    Page::Startup,
    Page::Updates,
    Page::Storage,
    Page::Power,
    Page::Access,
    Page::Keyboard,
    Page::Shortcuts,
    Page::Mouse,
    Page::Defaults,
    Page::DateTime,
    Page::Developer,
    Page::About,
];

impl Page {
    /// Every page, as the sidebar lists them.
    pub fn all() -> impl Iterator<Item = Page> {
        SHELL.into_iter().chain(SYSTEM)
    }

    /// Everyday words for what the page holds, for the launcher ("wifi" finds Network).
    pub fn words(self) -> &'static str {
        match self {
            Page::Bar => "panel taskbar modules clock tray status",
            Page::Appearance => {
                "theme wallpaper background accent colour color font dark icons cursor pointer"
            }
            Page::Time => "weather city temperature forecast",
            Page::Plugins => "extensions lua widgets",
            Page::Network => {
                "wifi wi-fi wireless internet ethernet cable vpn hotspot ip dns airplane"
            }
            Page::Bluetooth => "airpods headphones earbuds pair devices",
            Page::Account => "user name password avatar login",
            Page::Sound => "audio volume speakers headphones microphone mic output input alerts",
            Page::Displays => "monitor screen resolution refresh rate scale mirror night light",
            Page::Windows => "gaps blur rounding border shadows rules tiling focus",
            Page::Notifications => "do not disturb dnd alerts banners quiet",
            Page::Privacy => "lock screen recent files clipboard history",
            Page::Startup => "autostart login apps launch at start",
            Page::Updates => "upgrade packages firmware bios",
            Page::Storage => "disk space drive",
            Page::Power => "battery lid sleep suspend charging power button",
            Page::Access => "accessibility zoom magnifier colour filters greyscale mono",
            Page::Keyboard => "layout language typing input method repeat",
            Page::Shortcuts => "keybinds hotkeys keys bindings",
            Page::Mouse => "touchpad trackpad pointer scroll speed",
            Page::Defaults => "default apps browser terminal editor open with",
            Page::DateTime => "clock time zone timezone date region formats",
            Page::Developer => "debug log restart commands",
            Page::About => "system info version hardware computer",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Page::Bar => "Bar",
            Page::Appearance => "Appearance",
            Page::Time => "Weather",
            Page::Plugins => "Plugins",
            Page::Network => "Network",
            Page::Bluetooth => "Bluetooth",
            Page::Account => "Account",
            Page::Sound => "Sound",
            Page::Displays => "Displays",
            Page::Windows => "Windows",
            Page::Notifications => "Notifications",
            Page::Privacy => "Privacy",
            Page::Startup => "Startup apps",
            Page::Updates => "Updates",
            Page::Storage => "Storage",
            Page::Power => "Power",
            Page::Access => "Accessibility",
            Page::Keyboard => "Keyboard",
            Page::Shortcuts => "Shortcuts",
            Page::Mouse => "Mouse & touchpad",
            Page::Defaults => "Default apps",
            Page::DateTime => "Date & time",
            Page::Developer => "Developer",
            Page::About => "About",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Page::Bar => Icon::LayoutGrid,
            Page::Appearance => Icon::Image,
            Page::Time => Icon::CloudSun,
            Page::Plugins => Icon::Package,
            Page::Network => Icon::Wifi,
            Page::Bluetooth => Icon::Bluetooth,
            Page::Account => Icon::User,
            Page::Sound => Icon::Volume2,
            Page::Displays => Icon::Monitor,
            Page::Windows => Icon::AppWindow,
            Page::Notifications => Icon::Bell,
            Page::Privacy => Icon::Shield,
            Page::Startup => Icon::Rocket,
            Page::Updates => Icon::Package,
            Page::Storage => Icon::HardDrive,
            Page::Power => Icon::Battery,
            Page::Access => Icon::Accessibility,
            Page::Keyboard => Icon::Keyboard,
            Page::Shortcuts => Icon::Zap,
            Page::Mouse => Icon::Mouse,
            Page::Defaults => Icon::Check,
            Page::DateTime => Icon::CalendarClock,
            Page::Developer => Icon::Code,
            Page::About => Icon::Laptop,
        }
    }
}

static SEARCH_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("settings-search"));

#[derive(Default)]
pub struct State {
    pub page: Page,
    /// The search in the sidebar.
    query: String,
    /// config.toml as last read, for the modules' options.
    doc: toml::Table,
    city: String,
    /// Installed plugins, read when the panel opens.
    plugins: Vec<(String, String, String, bool)>,
    /// About this computer, read when its page opens.
    about: Option<crate::services::about::About>,
    /// Date and time, read when its page opens.
    time: Option<crate::services::timedate::State>,
    /// Choosing a time zone: the list is open, filtered by this.
    zones_open: bool,
    zone_query: String,
    /// A system change that did not work, and why.
    error: Option<String>,
    /// Network: the connection online through (settings path, name) and its IP settings as
    /// saved; the same being edited; the hotspot (running: as NetworkManager keeps it).
    ip_for: Option<(String, String)>,
    ip_saved: crate::services::network::IpSettings,
    ip_edit: IpEdit,
    hotspot: Option<crate::services::network::Hotspot>,
    hotspot_edit: (String, String),
    /// Network: the saved network whose Forget was pressed once (pressed again: forgotten).
    forget_net: Option<String>,
    /// Network: joining a hidden network (its form shown).
    hidden_open: bool,
    /// Bluetooth: a name being typed, for this computer (None) or a device (its path).
    bt_rename: Option<(Option<String>, String)>,
    /// Appearance: installed fonts, monospace fonts, icon themes and cursor themes, with the
    /// icon and cursor themes in use (read when the page opens); which list is open (0 font,
    /// 1 monospace, 2 icons, 3 pointer) and its search.
    look: Option<LookLists>,
    look_open: Option<u8>,
    look_query: String,
    /// Privacy: locking before sleep, recent files kept (and for how many days, -1 always);
    /// which Clear was pressed once (asks before clearing).
    privacy: Option<(Option<bool>, bool, i32)>,
    /// Firmware updates fwupd offers.
    firmware: Vec<crate::services::updates::Firmware>,
    /// Developer: Restart was pressed once.
    restart_armed: bool,
    /// Mouse & touchpad: the pointing devices and the speeds set for them.
    pointers: Vec<String>,
    device_speeds: Vec<crate::hypr_settings::DeviceSpeed>,
    /// Bar: the module whose options are open; Reset was pressed once.
    bar_open: Option<String>,
    bar_reset_armed: bool,
    /// Mono audio (None: WirePlumber did not say).
    mono: Option<bool>,
    /// Sound tests under way: the speakers; the microphone (1 recording, 2 playing back).
    speaker_test: bool,
    mic_test: u8,
    /// Region formats: the regions, (language, formats) now, the list open and its search.
    regions: Vec<crate::services::region::Region>,
    region_now: (String, String),
    region_open: bool,
    region_query: String,
    /// Default apps: the terminal in use (desktop file id), the installed terminals (id,
    /// name) and terminal editors, read when the page opens.
    terminal: Option<String>,
    terminals: Vec<(String, String)>,
    editors: Vec<&'static str>,
    clear_armed: Option<u8>,
    /// Power: the lid and the power button, as logind has them; the low-battery policy.
    lid: Option<crate::services::power::Lid>,
    low: Option<crate::services::power::LowBattery>,
    /// Windows: the app rules made here; the one open for editing; choosing an app (and the
    /// search for it); installed apps to choose from (read when the page opens).
    rules: Vec<crate::hypr_settings::WindowRule>,
    rule_edit: Option<crate::hypr_settings::WindowRule>,
    rule_pick: Option<String>,
    rule_apps: Vec<crate::services::apps::App>,
    /// Hyprland's input options as they are now (keyboard, mouse, touchpad pages).
    hypr: std::collections::HashMap<&'static str, crate::hypr_settings::Value>,
    /// Choosing a keyboard layout to add.
    layouts_open: bool,
    /// Default apps: the app list, and the app opening each kind now (by `defaults::KINDS`).
    apps: Vec<crate::services::apps::App>,
    defaults: Vec<Option<String>>,
    /// The kind whose list of apps is open.
    default_open: Option<usize>,
    /// Power: the battery's wear and the idle steps (seconds; None: not set up).
    health: Option<crate::services::power::Health>,
    /// The battery's charging modes and the one in use (where the firmware has them).
    charging: Option<(
        Vec<crate::services::power::Charging>,
        crate::services::power::Charging,
    )>,
    idle: Vec<(crate::services::idle::Step, Option<u64>)>,
    /// The idle step whose list of times is open.
    idle_open: Option<crate::services::idle::Step>,
    /// Displays as shown (and applied), the layout last kept, and a change waiting to be kept
    /// (its number and when it reverts by itself).
    displays: Vec<crate::services::displays::Display>,
    kept: Vec<crate::services::displays::Display>,
    pending: Option<(u64, std::time::Instant)>,
    /// The display whose list of modes is open.
    mode_open: Option<usize>,
    /// Shortcuts: every described bind (read from Hyprland), the ones added here, and one
    /// being made (keys recorded, name, command).
    binds: Vec<(String, Vec<crate::services::hyprland::Bind>)>,
    custom: Vec<crate::hypr_settings::Shortcut>,
    new_keys: Option<String>,
    recording: bool,
    /// The bind (its keys) whose new keys are being recorded.
    rebinding: Option<String>,
    new_name: String,
    new_command: String,
    /// Sound: the chooser that is open ("out", "in", "mode", or an app's name).
    sound_pick: Option<String>,
    /// Text size in apps (GTK's scaling factor).
    text_scale: f32,
    /// The quiet-hours time whose list is open (0 from, 1 to).
    quiet_open: Option<u8>,
    /// The computer's new name while it is typed (About).
    rename: Option<String>,
    /// Startup apps, and choosing one to add.
    startup: Vec<crate::services::autostart::Entry>,
    startup_add: bool,
    startup_query: String,
    /// The user (login, full name, administrator) and the full name being typed.
    account: Option<(String, String, bool)>,
    full_name: Option<String>,
    /// What fills the home folder (total, biggest entries), measured on the Storage page; and
    /// whether that is under way.
    usage: Option<(u64, Vec<(std::path::PathBuf, u64)>)>,
    measuring: bool,
    /// When the home folder was last measured (it is measured again on a later visit).
    measured: Option<std::time::Instant>,
    /// Every page's data has been read once (later opens read only the page shown).
    preloaded: bool,
    /// Filesystems and plugged-in drives (Storage).
    volumes: Vec<crate::services::disks::Volume>,
    /// Pending updates (None while checking).
    updates: Option<Vec<crate::services::updates::Package>>,
    layout_query: String,
}

/// What Appearance offers: fonts, monospace fonts, icon themes, cursor themes, and the icon
/// and cursor themes in use now.
#[derive(Clone, Debug, Default)]
pub struct LookLists {
    fonts: Vec<String>,
    monos: Vec<String>,
    icons: Vec<String>,
    cursors: Vec<String>,
    icons_now: String,
    cursor_now: String,
    /// The app and monospace fonts in use now (gsettings: "Geist 11"), shown until one is
    /// chosen here.
    font_now: Option<(String, u32)>,
    mono_now: Option<(String, u32)>,
}

/// "Geist 11" as ("Geist", 11).
fn font_pair(s: &str) -> Option<(String, u32)> {
    let (name, size) = s.trim().rsplit_once(' ')?;
    Some((name.to_string(), size.parse().ok()?))
}

/// IP settings as typed: manual address on, its address and router; custom DNS on, its servers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IpEdit {
    manual: bool,
    address: String,
    router: String,
    custom_dns: bool,
    dns: String,
}

impl IpEdit {
    fn from(ip: &crate::services::network::IpSettings) -> Self {
        Self {
            manual: ip.manual.is_some(),
            address: ip.manual.as_ref().map(|m| m.0.clone()).unwrap_or_default(),
            router: ip.manual.as_ref().map(|m| m.1.clone()).unwrap_or_default(),
            custom_dns: !ip.dns.is_empty(),
            dns: ip.dns.join(", "),
        }
    }

    fn settings(&self) -> crate::services::network::IpSettings {
        crate::services::network::IpSettings {
            manual: self.manual.then(|| {
                (
                    self.address.trim().to_string(),
                    self.router.trim().to_string(),
                )
            }),
            dns: if self.custom_dns {
                self.dns
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                    .map(str::to_string)
                    .collect()
            } else {
                Vec::new()
            },
        }
    }
}

#[derive(Clone, Debug)]
pub enum OptValue {
    Bool(bool),
    Str(&'static str),
}

/// Where the second display sits next to the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Left,
    Right,
    Above,
    Below,
}

#[derive(Clone, Debug)]
pub enum DisplayChange {
    On(bool),
    Mode(u32, u32, f32),
    Scale(f32),
    Rotate(u8),
    Place(Place),
    /// Show another screen's picture (its connector), or its own space again.
    Mirror(Option<String>),
}

/// How long new display settings wait to be kept before going back by themselves.
pub const KEEP_SECS: u64 = 15;

#[derive(Clone, Debug)]
pub enum Message {
    Page(Page),
    Query(String),
    Style(BarStyle),
    Workspaces(i64),
    ThemeApps(bool),
    Material(crate::config::Material),
    City(String),
    SubmitCity,
    /// Bar layout: a module moved within its section (-1 up, 1 down) or to the next section
    /// (-1 left, 1 right); removed; added (to its home section); the default layout back (the
    /// first press asks); a module's options shown under it (or closed).
    BarMove(String, i32),
    BarSection(String, i32),
    BarRemove(String),
    BarAdd(String),
    BarReset,
    BarOpen(Option<String>),
    Plugin(String, bool),
    /// A module's option (module and option index in `modules::MODULES`) to a value.
    Opt(usize, usize, OptValue),
    About(crate::services::about::About),
    Time(crate::services::timedate::State),
    Ntp(bool),
    WeekStart(bool),
    ZonesOpen(bool),
    ZoneQuery(String),
    Zone(String),
    /// A system change ended (then read again).
    Done(Result<(), String>),
    /// Hyprland's input options as read.
    Hypr(std::collections::HashMap<&'static str, crate::hypr_settings::Value>),
    /// An input option to a value: applied at once; kept when `save`.
    HyprSet(crate::hypr_settings::Opt, crate::hypr_settings::Value, bool),
    /// A slider let go: its value is kept.
    HyprKeep(crate::hypr_settings::Opt),
    LayoutsOpen(bool),
    LayoutQuery(String),
    LayoutAdd(String),
    /// An xkb choice (index in `hypr_settings::XKB`) to this option.
    Xkb(usize, String),
    LayoutRemove(String),
    Defaults(Vec<crate::services::apps::App>, Vec<Option<String>>),
    DefaultOpen(Option<usize>),
    /// A kind (index in `defaults::KINDS`) to this app (desktop file id).
    DefaultSet(usize, String),
    Power(
        Option<crate::services::power::Health>,
        Option<(
            Vec<crate::services::power::Charging>,
            crate::services::power::Charging,
        )>,
        Vec<(crate::services::idle::Step, Option<u64>)>,
    ),
    Charging(crate::services::power::Charging),
    Displays(Vec<crate::services::displays::Display>),
    /// A change to display `i`.
    Display(usize, DisplayChange),
    ModeOpen(Option<usize>),
    /// Keep the new display settings (written to hyprmon.lua) or go back.
    DisplayKeep,
    DisplayRevert,
    /// The time to keep change `n` ran out.
    DisplayTimeout(u64),
    /// The countdown moves.
    DisplayTick,
    Rename(Option<String>),
    Startup(
        Vec<crate::services::autostart::Entry>,
        Vec<crate::services::apps::App>,
    ),
    StartupOn(String, bool),
    StartupRemove(String),
    StartupAddOpen(bool),
    StartupQuery(String),
    StartupAdd(String),
    Updates(Vec<crate::services::updates::Package>),
    Volumes(Vec<crate::services::disks::Volume>),
    Usage(u64, Vec<(std::path::PathBuf, u64)>),
    Account((String, String, bool)),
    FullName(Option<String>),
    FullNameSubmit,
    /// Choose the account's picture (kestrel's file dialog), then the picture chosen.
    PickPicture,
    Picture(Option<std::path::PathBuf>),
    /// Open a volume in the file manager (mounting it first if needed).
    VolumeOpen(String, Option<String>),
    VolumeEject(String, String),
    /// A volume mounted: open it there.
    Mounted(Result<String, String>),
    RenameSubmit,
    /// Quiet hours on (with these times) or off.
    Quiet(Option<(String, String)>),
    TextScale(f32),
    SoundStep(u32),
    SoundMax(u32),
    /// A `[sound]` option: alerts (percent), feedback, screenshot.
    SoundOpt(&'static str, toml_edit::Value),
    /// Open (or close) a Sound chooser.
    SoundPick(Option<String>),
    Binds(Vec<(String, Vec<crate::services::hyprland::Bind>)>),
    Record(bool),
    /// Keys pressed while recording ("SUPER + T").
    Recorded(String),
    /// Change one of the user's binds (its keys now): record the new keys for it.
    Rebind(String),
    NewName(String),
    NewCommand(String),
    AddShortcut,
    RemoveShortcut(String),
    CursorSize(u32),
    /// Which end of the quiet hours has its list of times open (0 from, 1 to).
    QuietOpen(Option<u8>),
    /// An app's notifications: shown (0), quiet (1: no banner) or off (2).
    NotifyRule(String, u8),
    /// Network: the primary connection (path, name, IP settings) and the hotspot, read.
    NetRead(
        Option<(String, String, crate::services::network::IpSettings)>,
        Option<crate::services::network::Hotspot>,
    ),
    IpEdit(IpEdit),
    IpApply,
    HotspotEdit(String, String),
    /// Start (with the typed name and password) or stop the hotspot.
    HotspotOn(bool),
    /// Forget a saved network (the first press asks).
    ForgetNet(String),
    /// A saved network joins by itself (or not).
    AutoJoinNet(String, bool),
    HiddenOpen(bool),
    /// A fixed accent colour, or the wallpaper's (None).
    Accent(Option<&'static str>),
    /// Region formats: the regions glibc can make and (language, formats) now; the list
    /// opened or closed; its search; one chosen; back to the language's own.
    Regions(Vec<crate::services::region::Region>, (String, String)),
    RegionOpen(bool),
    RegionQuery(String),
    RegionSet(crate::services::region::Region),
    RegionReset,
    /// Mouse & touchpad: the devices and their speeds, read; a device's speed while dragging
    /// (false) or let go (true).
    Pointers(Vec<String>, Vec<crate::hypr_settings::DeviceSpeed>),
    DeviceSpeed(String, f32, bool),
    /// Mono audio, read or set (WirePlumber).
    Mono(Option<bool>),
    MonoSet(bool),
    /// Sound tests: the speakers (left, right); the microphone (records 3 s, plays it back);
    /// the alert sound at its volume. Each says when it is over.
    SpeakerTest,
    SpeakerDone,
    MicTest,
    MicRecorded(Result<(), String>),
    MicDone,
    AlertPlay,
    /// Weather: temperatures in "celsius" or "fahrenheit".
    Units(&'static str),
    /// Developer: open a file of kestrel's in the default app for it; restart the shell (the
    /// first press asks).
    OpenFile(std::path::PathBuf),
    Restart,
    /// Firmware updates (fwupd), read; one to install (fwupd's own updater, in the terminal).
    Firmware(Vec<crate::services::updates::Firmware>),
    FirmwareUpdate(String),
    /// Run a command in the default terminal (read when pressed), with this window title.
    InTerminal(&'static str, &'static str),
    /// Default apps: the terminal in use, the terminals and the editors installed, read.
    Terminals(String, Vec<(String, String)>, Vec<&'static str>),
    /// Default apps: the terminal (desktop file id) and the editor ($EDITOR).
    TerminalSet(String),
    EditorSet(String),
    /// Privacy: read (lock on sleep, remember recent files, days kept).
    Privacy(Option<bool>, bool, i32),
    LockOnSleep(bool),
    RememberRecent(bool),
    RecentDays(i32),
    /// Clear recent files (0) or the clipboard history (1); the first press asks.
    Clear(u8),
    /// Appearance: the lists, read; a list opened (or closed); its search; a choice from list
    /// 0 to 3; a font size (0 app, 1 monospace).
    Look(LookLists),
    LookOpen(Option<u8>),
    LookQuery(String),
    LookSet(u8, String),
    LookSize(u8, u32),
    /// Power: the low-battery policy (UPower), read; or set (warn at, end action).
    LowBattery(Option<crate::services::power::LowBattery>),
    LowSet(u32, String),
    /// Power: what the lid and the power button do (as logind has it), read; or set.
    Lid(Option<crate::services::power::Lid>),
    LidSet(crate::services::power::Lid),
    /// Windows: the rules and the installed apps, read.
    Rules(
        Vec<crate::hypr_settings::WindowRule>,
        Vec<crate::services::apps::App>,
    ),
    /// Choose an app for a new rule (Some: the search typed), or stop choosing.
    RulePick(Option<String>),
    /// Open an app's rule for editing (made if it has none), or close it.
    RuleOpen(Option<String>),
    /// An app's rule changed: applied and kept.
    RuleSet(crate::hypr_settings::WindowRule),
    RuleRemove(String),
    /// Bluetooth: start naming this computer (None) or a device (Some(path)), from this name.
    BtRename(Option<String>, String),
    BtRenameText(String),
    BtRenameDone,
    /// Join a network from here (a password asked for is typed in this window).
    JoinNet(crate::services::network::Network),
    /// A notification option in `[notifications]` (sound, banner, urgent_in_dnd).
    NotifyOpt(&'static str, toml_edit::Value),
    /// An app's banners sound (false: shown silently).
    NotifySilent(String, bool),
    Profile(crate::services::power::Profile),
    IdleOpen(Option<crate::services::idle::Step>),
    Idle(crate::services::idle::Step, u64),
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Settings(m))
}

/// config.toml as it is on disk (module options are read from it).
fn read_doc() -> toml::Table {
    std::fs::read_to_string(config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default()
}

pub fn open(app: &mut App) -> Task<AppMessage> {
    // Each open starts fresh: no search left from last time.
    app.panels.settings.query.clear();
    app.panels.settings.doc = read_doc();
    app.panels.settings.city = app.config.weather_city.clone().unwrap_or_default();
    app.panels.settings.plugins = crate::lua::installed();
    // The first open reads every page's data in the background, so switching pages shows a full
    // page at once (a page read only when chosen drew empty for a frame, then filled in). Later
    // opens keep that data and read again only the page shown: each page refreshes itself when
    // chosen. Updates is left to its page: checking for updates is slow and uses the network.
    let mut tasks: Vec<Task<AppMessage>> = Vec::new();
    if !app.panels.settings.preloaded {
        app.panels.settings.preloaded = true;
        tasks.extend(
            SYSTEM
                .into_iter()
                .filter(|p| *p != Page::Updates && *p != app.panels.settings.page)
                .map(load),
        );
    }
    tasks.push(load(app.panels.settings.page));
    // Typing searches at once.
    tasks.push(Task::Iced(iced::widget::focus(SEARCH_ID.clone())));
    Task::batch(tasks)
}

static BODY_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("settings-body"));

/// What a system page shows, read fresh when it opens (off the UI thread).
fn load(p: Page) -> Task<AppMessage> {
    match p {
        Page::About => Task::perform(
            async {
                tokio::task::spawn_blocking(crate::services::about::read_all)
                    .await
                    .unwrap_or_default()
            },
            |a| msg(Message::About(a)),
        ),
        Page::DateTime => Task::batch([
            Task::perform(crate::services::timedate::read(), |t| msg(Message::Time(t))),
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        (
                            crate::services::region::supported(),
                            crate::services::region::current(),
                        )
                    })
                    .await
                    .unwrap_or_default()
                },
                |(r, now)| msg(Message::Regions(r, now)),
            ),
        ]),
        Page::Power => Task::batch([
            Task::perform(crate::services::power::lid(), |l| msg(Message::Lid(l))),
            Task::perform(crate::services::power::low_battery(), |l| {
                msg(Message::LowBattery(l))
            }),
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        (
                            crate::services::power::health(),
                            crate::services::power::charging(),
                            crate::services::idle::read(),
                        )
                    })
                    .await
                    .unwrap_or_default()
                },
                |(h, c, i)| msg(Message::Power(h, c, i)),
            ),
        ]),
        Page::Account => Task::perform(
            async {
                tokio::task::spawn_blocking(crate::services::about::account)
                    .await
                    .unwrap_or_default()
            },
            |a| msg(Message::Account(a)),
        ),
        Page::Storage => {
            Task::perform(crate::services::disks::list(), |v| msg(Message::Volumes(v)))
        }
        Page::Privacy => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    let get = |k: &str| {
                        std::process::Command::new("gsettings")
                            .args(["get", "org.gnome.desktop.privacy", k])
                            .output()
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                            .unwrap_or_default()
                    };
                    (
                        crate::services::idle::lock_on_sleep(),
                        get("remember-recent-files") != "false",
                        get("recent-files-max-age").parse::<i32>().unwrap_or(-1),
                    )
                })
                .await
                .unwrap_or((None, true, -1))
            },
            |(l, r, d)| msg(Message::Privacy(l, r, d)),
        ),
        Page::Appearance => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    let (fonts, monos) = crate::look::fonts();
                    let get = |k: &str| {
                        std::process::Command::new("gsettings")
                            .args(["get", "org.gnome.desktop.interface", k])
                            .output()
                            .map(|o| {
                                String::from_utf8_lossy(&o.stdout)
                                    .trim()
                                    .trim_matches('\'')
                                    .to_string()
                            })
                            .unwrap_or_default()
                    };
                    LookLists {
                        fonts,
                        monos,
                        icons: crate::look::icon_themes(),
                        cursors: crate::look::cursor_themes(),
                        icons_now: get("icon-theme"),
                        cursor_now: get("cursor-theme"),
                        font_now: font_pair(&get("font-name")),
                        mono_now: font_pair(&get("monospace-font-name")),
                    }
                })
                .await
                .unwrap_or_default()
            },
            |l| msg(Message::Look(l)),
        ),
        Page::Network => Task::batch([
            Task::perform(
                async {
                    tokio::join!(
                        crate::services::network::primary(),
                        crate::services::network::hotspot()
                    )
                },
                |(p, h)| msg(Message::NetRead(p, h)),
            ),
            // Networks in range, fresh (NetworkManager scans rarely while connected).
            Task::future(crate::services::network::command(
                crate::services::network::Command::Scan,
            ))
            .discard(),
        ]),
        Page::Updates => Task::batch([
            Task::perform(crate::services::updates::list(), |v| {
                msg(Message::Updates(v))
            }),
            Task::perform(crate::services::updates::firmware(), |v| {
                msg(Message::Firmware(v))
            }),
        ]),
        Page::Startup => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    (
                        crate::services::autostart::list(),
                        crate::services::apps::load(),
                    )
                })
                .await
                .unwrap_or_default()
            },
            |(l, a)| msg(Message::Startup(l, a)),
        ),
        Page::Displays => Task::perform(crate::services::displays::read(), |d| {
            msg(Message::Displays(d))
        }),
        Page::Defaults => Task::batch([
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        let apps = crate::services::apps::load();
                        (
                            crate::services::defaults::terminal(),
                            crate::services::defaults::terminals(&apps),
                            crate::services::defaults::editors(),
                        )
                    })
                    .await
                    .unwrap_or_default()
                },
                |(t, ts, es)| msg(Message::Terminals(t, ts, es)),
            ),
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        let apps = crate::services::apps::load();
                        let now = crate::services::defaults::KINDS
                            .iter()
                            .map(|k| crate::services::defaults::current(k.types[0]))
                            .collect();
                        (apps, now)
                    })
                    .await
                    .unwrap_or_default()
                },
                |(a, n)| msg(Message::Defaults(a, n)),
            ),
        ]),
        Page::Shortcuts => Task::perform(crate::services::hyprland::binds(), |b| {
            msg(Message::Binds(b))
        }),
        Page::Access => Task::batch([
            Task::perform(crate::services::audio::mono(), |m| msg(Message::Mono(m))),
            Task::perform(crate::hypr_settings::text_scale(), |f| {
                msg(Message::TextScale(f))
            }),
            Task::perform(
                crate::hypr_settings::read(crate::hypr_settings::ACCESS.to_vec()),
                |v| msg(Message::Hypr(v)),
            ),
        ]),
        Page::Windows => {
            use crate::hypr_settings::{WINDOW_BEHAVIOUR, WINDOW_LOOK};
            Task::batch([
                Task::perform(
                    crate::hypr_settings::read([WINDOW_LOOK, WINDOW_BEHAVIOUR].concat()),
                    |v| msg(Message::Hypr(v)),
                ),
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(|| {
                            (
                                crate::hypr_settings::window_rules(),
                                crate::services::apps::load(),
                            )
                        })
                        .await
                        .unwrap_or_default()
                    },
                    |(r, a)| msg(Message::Rules(r, a)),
                ),
            ])
        }
        Page::Mouse => {
            use crate::hypr_settings::{KEYBOARD, MOUSE, Opt, TOUCHPAD};
            let mut opts: Vec<Opt> = [KEYBOARD, MOUSE, TOUCHPAD].concat();
            opts.push(LAYOUTS);
            opts.push(crate::hypr_settings::KB_OPTIONS);
            Task::batch([
                Task::perform(crate::hypr_settings::read(opts), |v| msg(Message::Hypr(v))),
                Task::perform(
                    async {
                        let devices = crate::services::hyprland::pointer_devices().await;
                        let speeds =
                            tokio::task::spawn_blocking(crate::hypr_settings::device_speeds)
                                .await
                                .unwrap_or_default();
                        (devices, speeds)
                    },
                    |(d, sp)| msg(Message::Pointers(d, sp)),
                ),
            ])
        }
        Page::Keyboard => {
            use crate::hypr_settings::{KEYBOARD, MOUSE, Opt, TOUCHPAD};
            let mut opts: Vec<Opt> = [KEYBOARD, MOUSE, TOUCHPAD].concat();
            opts.push(LAYOUTS);
            opts.push(crate::hypr_settings::KB_OPTIONS);
            Task::perform(crate::hypr_settings::read(opts), |v| msg(Message::Hypr(v)))
        }
        _ => Task::none(),
    }
}

/// The network changed: the Network page, when shown, reads its connection and hotspot again
/// (another network's IP settings, a hotspot started).
pub fn network_changed(app: &App, before: &crate::services::network::State) -> Task<AppMessage> {
    let n = &app.net;
    let moved = n.link != before.link
        || n.ssid != before.ssid
        || n.hotspot != before.hotspot
        || n.wired.as_ref().map(|w| w.connected) != before.wired.as_ref().map(|w| w.connected);
    if moved && app.settings_window.is_some() && app.panels.settings.page == Page::Network {
        load(Page::Network)
    } else {
        Task::none()
    }
}

pub fn update(app: &mut App, m: Message) -> Task<AppMessage> {
    match m {
        Message::Page(p) => {
            let st = &mut app.panels.settings;
            st.page = p;
            st.query.clear();
            st.error = None;
            st.zones_open = false;
            // Each page starts at its top (the last page's scroll carried over).
            return Task::batch([
                load(p),
                Task::Iced(iced::widget::scroll_to(
                    BODY_ID.clone(),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                )),
            ]);
        }
        Message::About(a) => app.panels.settings.about = Some(a),
        Message::Time(t) => app.panels.settings.time = Some(t),
        Message::WeekStart(sunday) => {
            app.config.clock.week_starts_sunday = sunday;
            config::set(&["clock"], "week_starts_sunday", toml_edit::value(sunday));
        }
        Message::Ntp(on) => {
            if let Some(t) = &mut app.panels.settings.time {
                t.ntp = on;
            }
            return Task::perform(crate::services::timedate::set_ntp(on), |r| {
                msg(Message::Done(r))
            });
        }
        Message::ZonesOpen(open) => {
            let st = &mut app.panels.settings;
            st.zones_open = open;
            st.zone_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("zone-search")));
            }
        }
        Message::ZoneQuery(q) => app.panels.settings.zone_query = q,
        Message::Zone(z) => {
            let st = &mut app.panels.settings;
            st.zones_open = false;
            if let Some(t) = &mut st.time {
                t.timezone = z.clone();
            }
            return Task::perform(crate::services::timedate::set_timezone(z), |r| {
                msg(Message::Done(r))
            });
        }
        Message::Hypr(v) => app.panels.settings.hypr.extend(v),
        Message::HyprSet(o, v, save) => {
            app.panels.settings.hypr.insert(o.option, v.clone());
            return Task::future(async move {
                if save {
                    crate::hypr_settings::save(o, v).await;
                } else {
                    crate::hypr_settings::apply(o, v).await;
                }
            })
            .discard();
        }
        Message::HyprKeep(o) => {
            if let Some(v) = app.panels.settings.hypr.get(o.option).cloned() {
                return Task::future(crate::hypr_settings::save(o, v)).discard();
            }
        }
        Message::LayoutsOpen(open) => {
            let st = &mut app.panels.settings;
            st.layouts_open = open;
            st.layout_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("layout-search")));
            }
        }
        Message::LayoutQuery(q) => app.panels.settings.layout_query = q,
        Message::Xkb(k, v) => {
            let o = crate::hypr_settings::KB_OPTIONS;
            let now = app
                .panels
                .settings
                .hypr
                .get(o.option)
                .map(crate::hypr_settings::Value::as_str)
                .unwrap_or_default();
            let new = crate::hypr_settings::xkb_with(&now, &crate::hypr_settings::XKB[k], &v);
            let v = crate::hypr_settings::Value::Str(new);
            app.panels.settings.hypr.insert(o.option, v.clone());
            return Task::future(crate::hypr_settings::save(o, v)).discard();
        }
        Message::Defaults(apps, now) => {
            let st = &mut app.panels.settings;
            st.apps = apps;
            st.defaults = now;
        }
        Message::DefaultOpen(k) => app.panels.settings.default_open = k,
        Message::Power(h, c, idle) => {
            let st = &mut app.panels.settings;
            st.health = h;
            st.charging = c;
            st.idle = idle;
        }
        Message::Charging(c) => {
            if let Some((_, now)) = &mut app.panels.settings.charging {
                *now = c;
            }
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::services::power::set_charging(c))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                |r| msg(Message::Done(r)),
            );
        }
        Message::Profile(p) => {
            app.power.profile = Some(p);
            return Task::future(crate::services::power::set_profile(p)).discard();
        }
        Message::IdleOpen(step) => app.panels.settings.idle_open = step,
        Message::Displays(d) => {
            let st = &mut app.panels.settings;
            if st.pending.is_none() {
                st.kept = d.clone();
                st.displays = d;
            }
        }
        Message::ModeOpen(i) => app.panels.settings.mode_open = i,
        Message::Display(i, change) => return change_display(app, i, change),
        Message::DisplayKeep => {
            let st = &mut app.panels.settings;
            st.pending = None;
            st.kept = st.displays.clone();
            let all = st.displays.clone();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::services::displays::save(&all))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                |r| msg(Message::Done(r)),
            );
        }
        Message::DisplayRevert => return revert_displays(app),
        Message::DisplayTimeout(n) => {
            if app.panels.settings.pending.is_some_and(|(p, _)| p == n) {
                return revert_displays(app);
            }
        }
        Message::DisplayTick => {}
        Message::Startup(list, apps) => {
            let st = &mut app.panels.settings;
            st.startup = list;
            if !apps.is_empty() {
                st.apps = apps;
            }
        }
        Message::StartupOn(id, on) => {
            if let Some(e) = app.panels.settings.startup.iter_mut().find(|e| e.id == id) {
                e.enabled = on;
            }
            return startup_job(move || crate::services::autostart::set_enabled(&id, on));
        }
        Message::StartupRemove(id) => {
            app.panels.settings.startup.retain(|e| e.id != id);
            return startup_job(move || crate::services::autostart::remove(&id));
        }
        Message::StartupAddOpen(open) => {
            let st = &mut app.panels.settings;
            st.startup_add = open;
            st.startup_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("startup-search")));
            }
        }
        Message::StartupQuery(q) => app.panels.settings.startup_query = q,
        Message::Updates(v) => app.panels.settings.updates = Some(v),
        Message::Volumes(v) => {
            let st = &mut app.panels.settings;
            st.volumes = v;
            // The home folder's use: measured only while Storage is shown (never by the preload),
            // again when the last count is over five minutes old.
            let stale = st
                .measured
                .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(300));
            if st.page == Page::Storage && stale && !st.measuring {
                st.measuring = true;
                let home = std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
                return Task::perform(crate::services::disks::usage(home), |(t, v)| {
                    msg(Message::Usage(t, v))
                });
            }
        }
        Message::Usage(total, items) => {
            let st = &mut app.panels.settings;
            st.measuring = false;
            st.measured = Some(std::time::Instant::now());
            st.usage = Some((total, items));
        }
        Message::Account(a) => app.panels.settings.account = Some(a),
        Message::FullName(t) => {
            let open = t.is_some() && app.panels.settings.full_name.is_none();
            app.panels.settings.full_name = t;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("full-name")));
            }
        }
        Message::PickPicture => {
            let filter = crate::services::filechooser::Filter {
                name: "Pictures".into(),
                globs: vec![],
                mimes: vec!["image/png".into(), "image/jpeg".into(), "image/webp".into()],
            };
            let pics =
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Pictures"));
            let (req, rx) =
                crate::services::filechooser::local("Choose a picture", Some(filter), pics);
            return Task::batch([
                Task::done(AppMessage::FileRequest(req)),
                Task::perform(rx, |a| {
                    msg(Message::Picture(match a {
                        Ok(crate::services::filechooser::Answer::Chosen(v)) => v.into_iter().next(),
                        _ => None,
                    }))
                }),
            ]);
        }
        Message::Picture(p) => {
            if let Some(p) = p {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || set_face(&p))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| msg(Message::Done(r)),
                );
            }
        }
        Message::FullNameSubmit => {
            if let Some(n) = app.panels.settings.full_name.take() {
                return Task::perform(crate::services::about::set_full_name(n), |r| {
                    msg(Message::Done(r))
                });
            }
        }
        Message::VolumeOpen(path, mount) => {
            return match mount {
                Some(m) => open_folder(m),
                None => Task::perform(crate::services::disks::mount(path), |r| {
                    msg(Message::Mounted(r))
                }),
            };
        }
        Message::Mounted(r) => {
            return match r {
                Ok(m) => Task::batch([open_folder(m), load(Page::Storage)]),
                Err(e) => {
                    app.panels.settings.error = Some(e);
                    Task::none()
                }
            };
        }
        Message::VolumeEject(path, drive) => {
            return Task::perform(crate::services::disks::eject(path, drive), |r| {
                msg(Message::Done(r))
            });
        }
        Message::StartupAdd(id) => {
            app.panels.settings.startup_add = false;
            return startup_job(move || {
                let f = crate::services::apps::desktop_file(&id).ok_or("app not found")?;
                crate::services::autostart::add(&f)
            });
        }
        Message::Rename(t) => {
            let open = t.is_some() && app.panels.settings.rename.is_none();
            app.panels.settings.rename = t;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("rename")));
            }
        }
        Message::RenameSubmit => {
            if let Some(name) = app.panels.settings.rename.take()
                && !name.trim().is_empty()
            {
                return Task::perform(crate::services::about::rename(name), |r| {
                    msg(Message::Done(r))
                });
            }
        }
        Message::Quiet(q) => {
            app.panels.settings.quiet_open = None;
            let (from, to) = q.unzip();
            app.config.quiet_from = from.clone();
            app.config.quiet_to = to.clone();
            config::set(
                &[],
                "quiet_from",
                toml_edit::value(from.unwrap_or_default()),
            );
            config::set(&[], "quiet_to", toml_edit::value(to.unwrap_or_default()));
            app.quiet_auto = None;
            app.quiet_schedule();
        }
        Message::QuietOpen(w) => app.panels.settings.quiet_open = w,
        Message::Binds(b) => {
            let st = &mut app.panels.settings;
            st.binds = b;
            st.custom = crate::hypr_settings::shortcuts();
        }
        Message::Record(on) => {
            let st = &mut app.panels.settings;
            st.recording = on;
            st.rebinding = None;
            if on {
                st.new_keys = None;
            }
        }
        Message::Rebind(old) => {
            let st = &mut app.panels.settings;
            st.recording = true;
            st.error = None;
            st.rebinding = Some(old);
        }
        Message::Recorded(k) => {
            let st = &mut app.panels.settings;
            st.recording = false;
            if let Some(old) = st.rebinding.take() {
                if let Some(what) = taken_by(st, &k) {
                    st.error = Some(format!("{k} is already {what}"));
                    return Task::none();
                }
                return Task::perform(crate::hypr_settings::rebind(old, k), |r| {
                    msg(Message::Done(r))
                });
            }
            st.new_keys = Some(k);
        }
        Message::NewName(t) => app.panels.settings.new_name = t,
        Message::NewCommand(t) => app.panels.settings.new_command = t,
        Message::AddShortcut => {
            let st = &mut app.panels.settings;
            if let Some(keys) = st.new_keys.take()
                && !st.new_command.trim().is_empty()
            {
                let sc = crate::hypr_settings::Shortcut {
                    keys,
                    name: if st.new_name.trim().is_empty() {
                        st.new_command.trim().to_string()
                    } else {
                        st.new_name.trim().to_string()
                    },
                    command: st.new_command.trim().to_string(),
                };
                st.new_name.clear();
                st.new_command.clear();
                return Task::batch([
                    Task::future(crate::hypr_settings::add_shortcut(sc)).discard(),
                    // Read back once Hyprland has it.
                    Task::perform(
                        async {
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            crate::services::hyprland::binds().await
                        },
                        |b| msg(Message::Binds(b)),
                    ),
                ]);
            }
        }
        Message::RemoveShortcut(keys) => {
            app.panels.settings.custom.retain(|c| c.keys != keys);
            return Task::batch([
                Task::future(crate::hypr_settings::remove_shortcut(keys)).discard(),
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        crate::services::hyprland::binds().await
                    },
                    |b| msg(Message::Binds(b)),
                ),
            ]);
        }
        Message::SoundPick(k) => app.panels.settings.sound_pick = k,
        Message::SoundStep(v) => {
            app.config.sound.step = v;
            config::set(&["sound"], "step", toml_edit::value(i64::from(v)));
        }
        Message::SoundMax(v) => {
            app.config.sound.max = v;
            config::set(&["sound"], "max", toml_edit::value(i64::from(v)));
        }
        Message::SoundOpt(key, v) => {
            config::set(&["sound"], key, toml_edit::Item::Value(v));
        }
        Message::TextScale(f) => {
            app.panels.settings.text_scale = f;
            return Task::future(crate::hypr_settings::set_text_scale(f)).discard();
        }
        Message::CursorSize(n) => {
            app.config.cursor_size = Some(n);
            config::set(&[], "cursor_size", toml_edit::value(i64::from(n)));
            return Task::future(crate::hypr_settings::apply_cursor(n)).discard();
        }
        Message::NotifyRule(name, rule) => {
            let rules = &app.config.notifications;
            let mut muted: Vec<String> = rules
                .muted
                .iter()
                .filter(|a| **a != name)
                .cloned()
                .collect();
            let mut quiet: Vec<String> = rules
                .quiet
                .iter()
                .filter(|a| **a != name)
                .cloned()
                .collect();
            match rule {
                1 => quiet.push(name),
                2 => muted.push(name),
                _ => {}
            }
            config::set(&["notifications"], "muted", config::list(&muted));
            config::set(&["notifications"], "quiet", config::list(&quiet));
        }
        Message::NetRead(primary, hot) => {
            let st = &mut app.panels.settings;
            let (now, ip) = match primary {
                Some((path, name, ip)) => (Some((path, name)), ip),
                None => (None, Default::default()),
            };
            // What is being typed stays, unless the connection changed or it was saved as is.
            if st.ip_for != now || st.ip_edit == IpEdit::from(&st.ip_saved) {
                st.ip_edit = IpEdit::from(&ip);
            }
            st.ip_for = now;
            st.ip_saved = ip;
            if hot.is_none() && st.hotspot_edit.0.is_empty() {
                st.hotspot_edit = (hostname(), new_password());
            }
            st.hotspot = hot;
        }
        Message::IpEdit(e) => app.panels.settings.ip_edit = e,
        Message::IpApply => {
            let st = &mut app.panels.settings;
            let Some((path, _)) = st.ip_for.clone() else {
                return Task::none();
            };
            let ip = st.ip_edit.settings();
            if let Err(e) = crate::services::network::check_ip(&ip) {
                st.error = Some(e);
                return Task::none();
            }
            st.error = None;
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::SetIp(
                    path, ip,
                )),
                |r| msg(Message::Done(r)),
            );
        }
        Message::HotspotEdit(n, p) => app.panels.settings.hotspot_edit = (n, p),
        Message::HotspotOn(on) => {
            let st = &mut app.panels.settings;
            let (name, pass) = st.hotspot_edit.clone();
            let cmd = if on {
                if name.trim().is_empty() || pass.chars().count() < 8 {
                    st.error = Some("The password needs at least 8 characters".into());
                    return Task::none();
                }
                crate::services::network::Command::Hotspot(Some((name.trim().to_string(), pass)))
            } else {
                crate::services::network::Command::Hotspot(None)
            };
            st.error = None;
            return Task::perform(crate::services::network::try_command(cmd), |r| {
                msg(Message::Done(r))
            });
        }
        Message::ForgetNet(ssid) => {
            let st = &mut app.panels.settings;
            if st.forget_net.as_deref() != Some(ssid.as_str()) {
                st.forget_net = Some(ssid);
                return Task::none();
            }
            st.forget_net = None;
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::Forget(
                    ssid,
                )),
                |r| msg(Message::Done(r)),
            );
        }
        Message::AutoJoinNet(ssid, on) => {
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::AutoJoin(
                    ssid, on,
                )),
                |r| msg(Message::Done(r)),
            );
        }
        Message::Terminals(t, ts, es) => {
            let st = &mut app.panels.settings;
            st.terminal = Some(t);
            st.terminals = ts;
            st.editors = es;
        }
        Message::Regions(r, now) => {
            let st = &mut app.panels.settings;
            st.regions = r;
            st.region_now = now;
        }
        Message::RegionOpen(open) => {
            let st = &mut app.panels.settings;
            st.region_open = open;
            st.region_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("region-search")));
            }
        }
        Message::RegionQuery(q) => app.panels.settings.region_query = q,
        Message::RegionSet(r) => {
            app.panels.settings.region_open = false;
            return Task::perform(crate::services::region::set(r), |r| msg(Message::Done(r)));
        }
        Message::RegionReset => {
            app.panels.settings.region_open = false;
            return Task::perform(crate::services::region::reset(), |r| msg(Message::Done(r)));
        }
        Message::Pointers(d, sp) => {
            app.panels.settings.pointers = d;
            app.panels.settings.device_speeds = sp;
        }
        Message::DeviceSpeed(name, v, keep) => {
            let st = &mut app.panels.settings;
            let d = crate::hypr_settings::DeviceSpeed {
                name: name.clone(),
                sensitivity: v,
            };
            match st.device_speeds.iter_mut().find(|x| x.name == name) {
                Some(x) => x.sensitivity = v,
                None => st.device_speeds.push(d.clone()),
            }
            return Task::future(crate::hypr_settings::set_device_speed(d, keep)).discard();
        }
        Message::Mono(m) => app.panels.settings.mono = m,
        Message::MonoSet(on) => {
            app.panels.settings.mono = Some(on);
            return Task::future(crate::services::audio::set_mono(on)).discard();
        }
        Message::SpeakerTest => {
            app.panels.settings.speaker_test = true;
            return Task::perform(crate::services::sounds::speaker_test(), |()| {
                msg(Message::SpeakerDone)
            });
        }
        Message::SpeakerDone => app.panels.settings.speaker_test = false,
        Message::MicTest => {
            app.panels.settings.mic_test = 1;
            return Task::perform(crate::services::sounds::mic_record(3), |r| {
                msg(Message::MicRecorded(r))
            });
        }
        Message::MicRecorded(r) => {
            let st = &mut app.panels.settings;
            if let Err(e) = r {
                st.mic_test = 0;
                st.error = Some(e);
                return Task::none();
            }
            st.mic_test = 2;
            return Task::perform(crate::services::sounds::mic_playback(), |()| {
                msg(Message::MicDone)
            });
        }
        Message::MicDone => app.panels.settings.mic_test = 0,
        Message::AlertPlay => {
            let v = app.config.sound.alert_volume();
            return Task::future(async move {
                crate::services::sounds::play_now(crate::services::sounds::MESSAGE, v).await;
            })
            .discard();
        }
        Message::Units(u) => config::set(&[], "weather_units", toml_edit::value(u)),
        Message::OpenFile(path) => {
            // A file not made yet is made empty (a folder, made), so it opens.
            if !path.exists() {
                if path.extension().is_none() {
                    let _ = std::fs::create_dir_all(&path);
                } else {
                    if let Some(d) = path.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    let _ = std::fs::write(&path, "");
                }
            }
            return Task::future(async move {
                let _ = tokio::process::Command::new("xdg-open").arg(&path).spawn();
            })
            .discard();
        }
        Message::Restart => {
            let st = &mut app.panels.settings;
            if !st.restart_armed {
                st.restart_armed = true;
                return Task::none();
            }
            st.restart_armed = false;
            return Task::future(async {
                let _ = tokio::process::Command::new("systemctl")
                    .args(["--user", "restart", "kestrel"])
                    .spawn();
            })
            .discard();
        }
        Message::Firmware(v) => app.panels.settings.firmware = v,
        Message::FirmwareUpdate(id) => {
            // The device id is fwupd's hex: nothing else reaches the shell.
            let id: String = id.chars().filter(char::is_ascii_hexdigit).collect();
            let line = crate::services::defaults::in_terminal(
                &format!("sh -c 'fwupdmgr update {id}; echo; read -p \"Press Enter to close.\" _'"),
                Some("Firmware"),
            );
            return Task::done(AppMessage::Dispatch(
                crate::services::compositor::Action::Exec(line),
            ));
        }
        Message::InTerminal(cmd, title) => {
            let line = crate::services::defaults::in_terminal(cmd, Some(title));
            return Task::done(AppMessage::Dispatch(
                crate::services::compositor::Action::Exec(line),
            ));
        }
        Message::TerminalSet(id) => {
            app.panels.settings.terminal = Some(id.clone());
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::services::defaults::set_terminal(&id)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                |r| msg(Message::Done(r)),
            );
        }
        Message::EditorSet(e) => {
            config::set(&[], "editor", toml_edit::value(e.clone()));
            let line = format!("hl.env(\"EDITOR\", \"{e}\") hl.env(\"VISUAL\", \"{e}\")");
            return Task::future(async move {
                crate::services::hyprland::eval(line).await;
                crate::hypr_settings::write_lua();
            })
            .discard();
        }
        Message::Privacy(l, r, d) => {
            app.panels.settings.privacy = Some((l, r, d));
            app.panels.settings.clear_armed = None;
        }
        Message::LockOnSleep(on) => {
            if let Some(p) = &mut app.panels.settings.privacy {
                p.0 = Some(on);
            }
            return Task::perform(crate::services::idle::set_lock_on_sleep(on), |r| {
                msg(Message::Done(r))
            });
        }
        Message::RememberRecent(on) => {
            if let Some(p) = &mut app.panels.settings.privacy {
                p.1 = on;
            }
            return Task::future(gsettings_privacy("remember-recent-files", on.to_string()))
                .discard();
        }
        Message::RecentDays(d) => {
            if let Some(p) = &mut app.panels.settings.privacy {
                p.2 = d;
            }
            return Task::future(gsettings_privacy("recent-files-max-age", d.to_string()))
                .discard();
        }
        Message::Clear(which) => {
            let st = &mut app.panels.settings;
            if st.clear_armed != Some(which) {
                st.clear_armed = Some(which);
                return Task::none();
            }
            st.clear_armed = None;
            return Task::perform(
                async move {
                    if which == 0 {
                        // GTK keeps them in one file; it starts a new one when it is gone.
                        let f = std::env::var_os("HOME")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_default()
                            .join(".local/share/recently-used.xbel");
                        match std::fs::remove_file(f) {
                            Ok(()) => Ok(()),
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                            Err(e) => Err(e.to_string()),
                        }
                    } else {
                        tokio::process::Command::new("cliphist")
                            .arg("wipe")
                            .status()
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|s| {
                                if s.success() {
                                    Ok(())
                                } else {
                                    Err("The clipboard history was not cleared".into())
                                }
                            })
                    }
                },
                move |r| {
                    let mut all = vec![];
                    // The clipboard panel's list goes with the history (once it is gone).
                    if which == 1 && r.is_ok() {
                        all.push(AppMessage::Panel(PanelMessage::Clipboard(
                            super::clipboard::Message::Loaded(Vec::new()),
                        )));
                    }
                    all.push(msg(Message::Done(r)));
                    AppMessage::Batch(all)
                },
            );
        }
        Message::Look(l) => app.panels.settings.look = Some(l),
        Message::LookOpen(which) => {
            let st = &mut app.panels.settings;
            st.look_open = which;
            st.look_query.clear();
            if which.is_some() {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("look-search")));
            }
        }
        Message::LookQuery(q) => app.panels.settings.look_query = q,
        Message::LookSet(which, name) => {
            let st = &mut app.panels.settings;
            st.look_open = None;
            let key = ["font", "mono", "icons", "cursor"][usize::from(which.min(3))];
            config::set(&["look"], key, toml_edit::value(name.clone()));
            let look = {
                let mut l = app.config.look.clone();
                // The other font (and sizes) stay as they are in use now.
                let now = app.panels.settings.look.as_ref();
                if let Some((f, fs)) = now.and_then(|x| x.font_now.clone()) {
                    l.font.get_or_insert(f);
                    l.font_size.get_or_insert(fs);
                }
                if let Some((m, ms)) = now.and_then(|x| x.mono_now.clone()) {
                    l.mono.get_or_insert(m);
                    l.mono_size.get_or_insert(ms);
                }
                match which {
                    0 => l.font = Some(name.clone()),
                    1 => l.mono = Some(name.clone()),
                    2 => l.icons = Some(name.clone()),
                    _ => l.cursor = Some(name.clone()),
                }
                l
            };
            if let Some(lists) = &mut app.panels.settings.look {
                match which {
                    2 => lists.icons_now = name.clone(),
                    3 => lists.cursor_now = name.clone(),
                    _ => {}
                }
            }
            let size = app.config.cursor_size.unwrap_or(24);
            return Task::future(async move {
                match which {
                    0 | 1 => {
                        let ((f, fs), (m, ms)) = (look.font(), look.mono());
                        crate::look::apply_fonts(f, fs, m, ms).await;
                    }
                    2 => crate::look::apply_icons(name).await,
                    _ => {
                        crate::look::apply_cursor(name, size).await;
                        crate::hypr_settings::write_lua();
                    }
                }
            })
            .discard();
        }
        Message::LookSize(which, n) => {
            let key = if which == 0 { "font_size" } else { "mono_size" };
            config::set(&["look"], key, toml_edit::value(i64::from(n)));
            let mut l = app.config.look.clone();
            // A size alone keeps the font in use (not kestrel's default in its place).
            let now = app.panels.settings.look.as_ref();
            if l.font.is_none()
                && let Some((f, _)) = now.and_then(|x| x.font_now.clone())
            {
                l.font = Some(f);
            }
            if l.mono.is_none()
                && let Some((m, _)) = now.and_then(|x| x.mono_now.clone())
            {
                l.mono = Some(m);
            }
            if which == 0 {
                l.font_size = Some(n);
            } else {
                l.mono_size = Some(n);
            }
            return Task::future(async move {
                let ((f, fs), (m, ms)) = (l.font(), l.mono());
                crate::look::apply_fonts(f, fs, m, ms).await;
            })
            .discard();
        }
        Message::Accent(a) => config::set(
            &[],
            "accent",
            match a {
                Some(hex) => toml_edit::value(hex),
                None => toml_edit::Item::None,
            },
        ),
        Message::Lid(l) => app.panels.settings.lid = l,
        Message::LowBattery(l) => app.panels.settings.low = l,
        Message::LowSet(warn, action) => {
            if let Some(l) = &mut app.panels.settings.low {
                l.warn_at = warn;
                l.action = action.clone();
            }
            return Task::perform(
                async move { crate::services::power::set_low_battery(warn, &action).await },
                |r| msg(Message::Done(r)),
            );
        }
        Message::LidSet(l) => {
            // Shown at once; logind is changed once the password is given (or it reads back).
            app.panels.settings.lid = Some(l);
            return Task::perform(crate::services::power::set_lid(l), |r| {
                msg(Message::Done(r))
            });
        }
        Message::Rules(r, a) => {
            let st = &mut app.panels.settings;
            // A rule opened before the saved ones were read: its saved form, if it has one.
            if let Some(e) = &st.rule_edit
                && let Some(saved) = r.iter().find(|x| x.class == e.class)
            {
                st.rule_edit = Some(saved.clone());
            }
            st.rules = r;
            st.rule_apps = a;
        }
        Message::RulePick(q) => {
            let st = &mut app.panels.settings;
            let start = st.rule_pick.is_none() && q.is_some();
            st.rule_pick = q;
            st.rule_edit = None;
            if start {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("rule-search")));
            }
        }
        Message::RuleOpen(class) => {
            let st = &mut app.panels.settings;
            st.rule_pick = None;
            st.rule_edit = class.map(|c| {
                st.rules.iter().find(|r| r.class == c).cloned().unwrap_or(
                    crate::hypr_settings::WindowRule {
                        class: c,
                        ..Default::default()
                    },
                )
            });
        }
        Message::RuleSet(r) => {
            let st = &mut app.panels.settings;
            match st.rules.iter_mut().find(|x| x.class == r.class) {
                Some(x) => *x = r.clone(),
                None => st.rules.push(r.clone()),
            }
            st.rule_edit = Some(r.clone());
            return Task::future(crate::hypr_settings::set_window_rule(r)).discard();
        }
        Message::RuleRemove(class) => {
            let st = &mut app.panels.settings;
            st.rules.retain(|r| r.class != class);
            st.rule_edit = None;
            return Task::future(crate::hypr_settings::remove_window_rule(class)).discard();
        }
        Message::BtRename(target, now) => {
            app.panels.settings.bt_rename = Some((target, now));
            return Task::Iced(iced::widget::focus(iced::widget::Id::new("bt-rename")));
        }
        Message::BtRenameText(t) => {
            if let Some((_, text)) = &mut app.panels.settings.bt_rename {
                *text = t;
            }
        }
        Message::BtRenameDone => {
            let Some((target, name)) = app.panels.settings.bt_rename.take() else {
                return Task::none();
            };
            let name = name.trim().to_string();
            if name.is_empty() {
                return Task::none();
            }
            use crate::services::bluetooth::Command as B;
            let cmd = match target {
                None => B::Rename(name),
                Some(path) => B::RenameDevice(path, name),
            };
            return Task::perform(crate::services::bluetooth::command(cmd), |r| {
                msg(Message::Done(r))
            });
        }
        Message::JoinNet(n) => {
            let joined = super::control::update(app, super::control::Message::Join(n));
            return Task::batch([
                joined,
                Task::Iced(iced::widget::focus(iced::widget::Id::new("net-password"))),
            ]);
        }
        Message::HiddenOpen(open) => {
            app.panels.settings.hidden_open = open;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("hidden-name")));
            }
        }
        Message::NotifyOpt(key, v) => {
            config::set(&["notifications"], key, toml_edit::Item::Value(v));
        }
        Message::NotifySilent(name, sound) => {
            let mut silent: Vec<String> = app
                .config
                .notifications
                .silent
                .iter()
                .filter(|a| **a != name)
                .cloned()
                .collect();
            if !sound {
                silent.push(name);
            }
            config::set(&["notifications"], "silent", config::list(&silent));
        }
        Message::Idle(step, secs) => {
            let st = &mut app.panels.settings;
            st.idle_open = None;
            if let Some(e) = st.idle.iter_mut().find(|(s, _)| *s == step) {
                e.1 = Some(secs);
            }
            return Task::perform(crate::services::idle::set(step, secs), |r| {
                msg(Message::Done(r))
            });
        }
        Message::DefaultSet(k, id) => {
            let st = &mut app.panels.settings;
            st.default_open = None;
            if let Some(slot) = st.defaults.get_mut(k) {
                *slot = Some(id.clone());
            }
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::services::defaults::set(&crate::services::defaults::KINDS[k], &id)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                |r| msg(Message::Done(r)),
            );
        }
        Message::LayoutAdd(code) => return change_layouts(app, code, true),
        Message::LayoutRemove(code) => return change_layouts(app, code, false),
        Message::Done(r) => {
            app.panels.settings.error = r.err();
            return load(app.panels.settings.page);
        }
        Message::Query(q) => {
            // The first letter typed reads the quick system pages, so their settings are found.
            let first = app.panels.settings.query.is_empty() && !q.is_empty();
            app.panels.settings.query = q;
            if first {
                return Task::batch([
                    load(Page::Keyboard),
                    load(Page::Windows),
                    load(Page::Access),
                    load(Page::Power),
                    load(Page::DateTime),
                ]);
            }
        }
        Message::Style(s) => config::set(
            &["bar"],
            "style",
            toml_edit::value(match s {
                BarStyle::Full => "full",
                BarStyle::Pills => "pills",
            }),
        ),
        Message::Workspaces(n) => config::set(&[], "workspaces", toml_edit::value(n.clamp(1, 10))),
        Message::ThemeApps(on) => config::set(&[], "theme_apps", toml_edit::value(on)),
        Message::Material(m) => config::set(
            &[],
            "material",
            toml_edit::value(match m {
                crate::config::Material::Black => "black",
                crate::config::Material::Frosted => "frosted",
            }),
        ),
        Message::City(c) => app.panels.settings.city = c,
        Message::SubmitCity => {
            let c = app.panels.settings.city.trim().to_string();
            config::set(&[], "weather_city", toml_edit::value(c));
        }
        Message::Opt(m, o, v) => {
            if let Some(opt) = crate::modules::MODULES
                .get(m)
                .and_then(|m| m.options.get(o))
            {
                let item = match v {
                    OptValue::Bool(b) => toml_edit::value(b),
                    OptValue::Str(t) => toml_edit::value(t),
                };
                config::set(opt.table, opt.key, item);
                app.panels.settings.doc = read_doc();
            }
        }
        Message::Plugin(id, on) => {
            config::set(&["plugins", &id], "enabled", toml_edit::value(on));
            app.panels.settings.plugins = crate::lua::installed();
        }
        Message::BarOpen(id) => app.panels.settings.bar_open = id,
        Message::BarMove(id, d) => {
            let mut l = layout(app);
            if let Some((sec, at)) = find_in(&l, &id) {
                let to = at as i32 + d;
                if to >= 0 && (to as usize) < l[sec].len() {
                    l[sec].swap(at, to as usize);
                    save_layout(&l);
                }
            }
        }
        Message::BarSection(id, d) => {
            let mut l = layout(app);
            if let Some((sec, at)) = find_in(&l, &id) {
                let to = sec as i32 + d;
                if (0..3).contains(&to) {
                    let m = l[sec].remove(at);
                    // Into the near end of the next section (right: its start; left: its end).
                    if d > 0 {
                        l[to as usize].insert(0, m);
                    } else {
                        l[to as usize].push(m);
                    }
                    save_layout(&l);
                }
            }
        }
        Message::BarRemove(id) => {
            let mut l = layout(app);
            for list in &mut l {
                list.retain(|m| *m != id);
            }
            if app.panels.settings.bar_open.as_deref() == Some(id.as_str()) {
                app.panels.settings.bar_open = None;
            }
            save_layout(&l);
        }
        Message::BarAdd(id) => {
            let mut l = layout(app);
            let sec = match crate::modules::info(&id).map(|m| m.home) {
                Some(crate::modules::Home::Left) => 0,
                Some(crate::modules::Home::Center) => 1,
                _ => 2,
            };
            if find_in(&l, &id).is_none() {
                // The right section's end is the power button's place: added before it.
                let at = if sec == 2 {
                    l[2].iter().position(|m| m == "power").unwrap_or(l[2].len())
                } else {
                    l[sec].len()
                };
                l[sec].insert(at, id);
                save_layout(&l);
            }
        }
        Message::BarReset => {
            let st = &mut app.panels.settings;
            if !st.bar_reset_armed {
                st.bar_reset_armed = true;
                return Task::none();
            }
            st.bar_reset_armed = false;
            let d = crate::config::BarConfig::default();
            save_layout(&[d.left, d.center, d.right]);
        }
    }
    Task::none()
}

thread_local! {
    /// The Settings search, lowercased, while a view is being built (empty: everything).
    static QUERY: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Whether `name` is shown for the search being typed (always when not searching).
fn matches(name: &str) -> bool {
    QUERY.with(|q| {
        let q = q.borrow();
        q.is_empty() || name.to_lowercase().contains(q.as_str())
    })
}

/// A setting: its name (and a line under it) on the left, its control on the right.
fn row_of<'a>(
    s: Scheme,
    name: &'a str,
    note: Option<&'a str>,
    control_: Element<'a, AppMessage>,
) -> Option<Element<'a, AppMessage>> {
    // While searching, only the settings whose name matches.
    if !QUERY.with(|q| {
        let q = q.borrow();
        q.is_empty() || name.to_lowercase().contains(q.as_str())
    }) {
        return None;
    }
    let mut words = column![label(name, text::BODY, s.ink)].spacing(tok::XXS);
    if let Some(n) = note {
        words = words.push(label(n, text::CAPTION, s.ink_dim));
    }
    Some(
        container(
            row![container(words).width(Length::Fill), control_]
                .spacing(tok::L)
                .align_y(Alignment::Center),
        )
        .padding([tok::NONE, tok::XL])
        .height(if note.is_some() { 56 } else { 46 })
        .align_y(Alignment::Center)
        .into(),
    )
}

/// Rows grouped on one rounded card, a hairline between them, a heading above if given.
fn group<'a>(
    s: Scheme,
    title: Option<&'a str>,
    rows: Vec<Option<Element<'a, AppMessage>>>,
) -> Option<Element<'a, AppMessage>> {
    let rows: Vec<Element<'a, AppMessage>> = rows.into_iter().flatten().collect();
    if rows.is_empty() {
        return None;
    }
    let mut card = column![].spacing(tok::NONE);
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        card = card.push(r);
        if i + 1 < n {
            card = card.push(
                container(
                    container(iced::widget::Space::new())
                        .width(Length::Fill)
                        .height(1)
                        .style(move |_| container::Style {
                            background: Some(s.ink.scale_alpha(0.07 * s.opacity).into()),
                            ..container::Style::default()
                        }),
                )
                .padding([tok::NONE, tok::XL]),
            );
        }
    }
    let card = container(card)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(0.055 * s.opacity).into()),
            border: iced::Border::default().rounded(size::CARD_RADIUS),
            ..container::Style::default()
        });
    Some(match title {
        Some(t) => column![crate::ui::section(s, t, tok::XS), card]
            .spacing(tok::M)
            .into(),
        None => card.into(),
    })
}

/// Two or more choices, one of them chosen.
fn segments<'a>(s: Scheme, options: &[(&'a str, bool, AppMessage)]) -> Element<'a, AppMessage> {
    let mut r = row![].spacing(tok::XXS);
    for (name, on, m) in options {
        r = r.push(
            control(label(
                *name,
                text::SMALL,
                if *on { s.on_accent } else { s.ink },
            ))
            .padding([tok::S, tok::L])
            .radius(tok::FULL)
            .kind(
                s,
                if *on {
                    crate::ui::control::Kind::Primary
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .on_press(m.clone()),
        );
    }
    container(r)
        .padding(tok::XXS)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(0.06 * s.opacity).into()),
            border: iced::Border::default().rounded(tok::FULL),
            ..container::Style::default()
        })
        .into()
}

/// Settings > Sound is showing (its live levels run then).
pub fn on_sound(st: &State) -> bool {
    st.page == Page::Sound && st.query.is_empty()
}

/// Settings as its own window: the same view on the shell's solid surface colour.
pub fn window(app: &App) -> Element<'_, AppMessage> {
    let s = app.scheme.faded(1.0);
    let body = container(view(app, s))
        .width(Length::Fill)
        .height(Length::Fill);
    // The shell's look: Black is solid; Frosted is the panels' own glass (the compositor's
    // blur of what is behind, under the same dark tint).
    if crate::theme::glass() {
        iced::blur_container(body)
            .style(move |_| container::Style {
                background: Some(s.base.scale_alpha(0.42).into()),
                ..container::Style::default()
            })
            .into()
    } else {
        body.style(move |_| container::Style {
            background: Some(s.base.into()),
            ..container::Style::default()
        })
        .into()
    }
}

pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let page = app.panels.settings.page;
    let shell_pages: Vec<Page> = SHELL
        .into_iter()
        .filter(|p| *p != Page::Plugins || !app.panels.settings.plugins.is_empty())
        .collect();
    // Sidebar: the pages, the open one on a soft neutral fill.
    let search = container(
        row![
            icon(Icon::Search, 15.0, s.ink_dim),
            text_input("Search", &app.panels.settings.query)
                .id(SEARCH_ID.clone())
                .on_input(|q| msg(Message::Query(q)))
                .padding([tok::S, tok::XS])
                .size(text::BODY)
                .font(theme::FONT)
                .style(move |_, _| text_input::Style {
                    background: Color::TRANSPARENT.into(),
                    border: iced::Border::default(),
                    icon: s.ink_dim,
                    placeholder: s.ink_faint,
                    value: s.ink,
                    selection: s.accent_soft,
                })
        ]
        .spacing(tok::XS)
        .align_y(Alignment::Center),
    )
    .padding([tok::NONE, tok::L])
    .height(34)
    .center_y(34)
    .style(move |_| {
        let f = ui::field_style(s);
        container::Style {
            border: iced::Border {
                radius: 17.0.into(),
                ..f.border
            },
            ..f
        }
    });
    let mut side = column![
        container(label_w("Settings", text::HEADING, s.ink, theme::SEMIBOLD))
            .padding([tok::S, tok::L]),
        container(search).padding(iced::Padding {
            bottom: 8.0,
            ..iced::Padding::ZERO
        }),
    ]
    .spacing(tok::XXS);
    let heading = |t: &'static str| {
        container(label_w(t, text::CAPTION, s.ink_faint, theme::SEMIBOLD)).padding(iced::Padding {
            top: 10.0,
            right: 10.0,
            bottom: 4.0,
            left: 10.0,
        })
    };
    let mut entries: Vec<Option<Page>> = shell_pages.into_iter().map(Some).collect();
    entries.push(None);
    entries.extend(SYSTEM.into_iter().map(Some));
    for p in entries {
        let Some(p) = p else {
            side = side.push(heading("System"));
            continue;
        };
        let on = p == page;
        let fg = if on { s.ink } else { s.ink_dim };
        side = side.push(
            control(
                row![
                    icon(p.icon(), 16.0, fg),
                    label_w(
                        p.name(),
                        text::BODY,
                        fg,
                        if on { theme::SEMIBOLD } else { theme::MEDIUM }
                    )
                ]
                .spacing(tok::L)
                .align_y(Alignment::Center),
            )
            .padding([tok::NONE, tok::L])
            .height(36)
            .width(Length::Fill)
            .radius(size::ROW_RADIUS)
            .kind(
                s,
                if on {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .no_press_scale()
            .on_press(msg(Message::Page(p))),
        );
    }
    let side = container(
        iced::widget::scrollable(side).direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(0)
                .scroller_width(0),
        )),
    )
    .width(SIDEBAR)
    .height(Length::Fill)
    .padding([tok::L, tok::L]);

    // Searching: every page's matching settings at once.
    let q = app.panels.settings.query.trim().to_lowercase();
    QUERY.with(|cell| *cell.borrow_mut() = q.clone());
    let body = if q.is_empty() {
        column![label_w(page.name(), text::HEADING, s.ink, theme::SEMIBOLD)]
            .spacing(tok::XL)
            .extend(match page {
                Page::Bar => bar_page(app, s),
                Page::Appearance => appearance_page(app, s),
                Page::Time => time_page(app, s),
                Page::Plugins => plugins_page(app, s),
                Page::Keyboard => keyboard_page(app, s),
                Page::Mouse => mouse_page(app, s),
                Page::Defaults => defaults_page(app, s),
                Page::Power => power_page(app, s),
                Page::Access => access_page(app, s),
                Page::Shortcuts => shortcuts_page(app, s),
                Page::Displays => displays_page(app, s),
                Page::Windows => windows_page(app, s),
                Page::Notifications => notifications_page(app, s),
                Page::Privacy => privacy_page(app, s),
                Page::Startup => startup_page(app, s),
                Page::Updates => updates_page(app, s),
                Page::Storage => storage_page(app, s),
                Page::Network => network_page(app, s),
                Page::Bluetooth => bluetooth_page(app, s),
                Page::Account => account_page(app, s),
                Page::Sound => sound_page(app, s),
                Page::DateTime => datetime_page(app, s),
                Page::Developer => developer_page(app, s),
                Page::About => about_page(app, s),
            })
    } else {
        column![label_w("Search", text::HEADING, s.ink, theme::SEMIBOLD)]
            .spacing(tok::XL)
            .extend(bar_page(app, s))
            .extend(appearance_page(app, s))
            .extend(time_page(app, s))
            .extend(plugins_page(app, s))
            // The system pages that have been read (searching reads the quick ones).
            .extend(power_page(app, s))
            .extend(access_page(app, s))
            .extend(keyboard_page(app, s))
            .extend(mouse_page(app, s))
            .extend(windows_page(app, s))
            .extend(datetime_page(app, s))
            .extend(notifications_page(app, s))
    };
    QUERY.with(|cell| cell.borrow_mut().clear());
    // A comfortable reading width however wide the window (rows and sliders stretched over a
    // whole screen read badly).
    let body = iced::widget::scrollable(
        container(container(body.max_width(760)).padding([20, 22])).center_x(Length::Fill),
    )
    .id(BODY_ID.clone())
    .height(Length::Fill)
    .width(Length::Fill)
    .direction(iced::widget::scrollable::Direction::Vertical(
        iced::widget::scrollable::Scrollbar::new()
            .width(0)
            .scroller_width(0),
    ));
    let body = crate::ui::smooth(body);
    let divider = container(iced::widget::Space::new())
        .width(1)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(s.ink.scale_alpha(0.07 * s.opacity).into()),
            ..container::Style::default()
        });
    row![side, divider, body].height(Length::Fill).into()
}

fn stepper<'a>(s: Scheme, n: i64) -> Element<'a, AppMessage> {
    let arrow = |i: Icon, m: Message| {
        control(container(icon(i, 16.0, s.ink)).center(28))
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(msg(m))
    };
    row![
        arrow(Icon::ChevronLeft, Message::Workspaces(n - 1)),
        container(label_w(n.to_string(), text::BODY, s.ink, theme::SEMIBOLD)).center_x(28),
        arrow(Icon::ChevronRight, Message::Workspaces(n + 1)),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A setting that only shows a value (About): its name and the value, right-aligned.
fn value_row<'a>(s: Scheme, name: &'a str, value: String) -> Option<Element<'a, AppMessage>> {
    row_of(s, name, None, label(value, text::BODY, s.ink_dim).into())
}

/// This computer's name (a hotspot is named after it at first).
fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_string())
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "kestrel".into())
}

/// A hotspot password: 12 letters and digits, none that read alike (0/O, 1/l/I).
fn new_password() -> String {
    const SET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 12];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes))
        .is_err()
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (t >> (i * 5)) as u8;
        }
    }
    bytes
        .iter()
        .map(|b| SET[*b as usize % SET.len()] as char)
        .collect()
}

/// A text field in a row: the same quiet capsule as every other field.
fn field<'a>(
    s: Scheme,
    id: &'static str,
    placeholder: &'a str,
    value: &str,
    on_input: impl Fn(String) -> AppMessage + 'a,
    on_submit: Option<AppMessage>,
    secure: bool,
) -> Element<'a, AppMessage> {
    let mut t = text_input(placeholder, value)
        .id(iced::widget::Id::new(id))
        .on_input(on_input)
        .secure(secure)
        .padding([tok::S, tok::L])
        .size(text::BODY)
        .font(theme::FONT)
        .width(240)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: iced::Border::default(),
            icon: s.ink_dim,
            placeholder: s.ink_faint,
            value: s.ink,
            selection: s.accent_soft,
        });
    if let Some(m) = on_submit {
        t = t.on_submit(m);
    }
    container(t).style(move |_| ui::field_style(s)).into()
}

/// A value that copies itself when clicked (addresses, passwords).
fn copy_row<'a>(s: Scheme, name: &'a str, value: String) -> Option<Element<'a, AppMessage>> {
    row_of(
        s,
        name,
        None,
        control(label(value.clone(), text::BODY, s.ink_dim))
            .padding([tok::XS, tok::L])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .no_press_scale()
            .on_press(snd(super::control::Message::CopyIp(value)))
            .into(),
    )
}

/// A connection's addresses (Wi-Fi or Ethernet), each copied by a click.
fn detail_rows<'a>(
    s: Scheme,
    d: &crate::services::network::Details,
) -> Vec<Option<Element<'a, AppMessage>>> {
    let mut rows = Vec::new();
    if let Some(ip) = &d.ipv4 {
        rows.push(copy_row(s, "IP address", ip.clone()));
    }
    if let Some(ip) = &d.ipv6 {
        rows.push(copy_row(s, "IPv6 address", ip.clone()));
    }
    if let Some(g) = &d.gateway {
        rows.push(copy_row(s, "Router", g.clone()));
    }
    if !d.dns.is_empty() {
        rows.push(copy_row(s, "DNS", d.dns.join(", ")));
    }
    rows
}

/// A search field for a pick list (time zones, layouts).
fn pick_search<'a>(
    s: Scheme,
    id: &'static str,
    value: &'a str,
    on: fn(String) -> Message,
) -> Element<'a, AppMessage> {
    container(
        text_input("Search", value)
            .id(iced::widget::Id::new(id))
            .on_input(move |q| msg(on(q)))
            .padding([tok::S, tok::L])
            .size(text::BODY)
            .font(theme::FONT)
            .style(move |_, _| text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: iced::Border::default(),
                icon: s.ink_dim,
                placeholder: s.ink_faint,
                value: s.ink,
                selection: s.accent_soft,
            }),
    )
    .style(move |_| ui::field_style(s))
    .into()
}

/// One entry of a pick list.
fn pick_row<'a>(s: Scheme, name: String, on: bool, m: Message) -> Element<'a, AppMessage> {
    control(label(name, text::BODY, if on { s.accent } else { s.ink }))
        .padding([tok::NONE, tok::L])
        .height(34)
        .width(Length::Fill)
        .radius(size::ROW_RADIUS)
        .kind(s, crate::ui::control::Kind::Ghost)
        .no_press_scale()
        .on_press(msg(m))
        .into()
}

/// A row for one Hyprland input option: a switch, a choice, or a slider with its value.
fn hypr_row<'a>(
    app: &'a App,
    s: Scheme,
    o: crate::hypr_settings::Opt,
) -> Option<Element<'a, AppMessage>> {
    use crate::hypr_settings::{Kind, Value};
    let v = app.panels.settings.hypr.get(o.option)?;
    let control_: Element<'_, AppMessage> = match o.kind {
        Kind::Switch => {
            let on = v.as_bool();
            ui::switch(s, on, msg(Message::HyprSet(o, Value::Bool(!on), true)))
        }
        Kind::Choice(choices) => {
            let now = v.as_str();
            let opts: Vec<(&str, bool, AppMessage)> = choices
                .iter()
                .map(|(val, name)| {
                    (
                        *name,
                        *val == now,
                        msg(Message::HyprSet(o, Value::Str((*val).to_string()), true)),
                    )
                })
                .collect();
            segments(s, &opts)
        }
        Kind::Numbers(choices) => {
            let now = v.as_num().round() as i64;
            let opts: Vec<(&str, bool, AppMessage)> = choices
                .iter()
                .map(|(val, name)| {
                    (
                        *name,
                        *val == now,
                        msg(Message::HyprSet(o, Value::Num(*val as f32), true)),
                    )
                })
                .collect();
            segments(s, &opts)
        }
        Kind::Range {
            min,
            max,
            step,
            unit,
        } => {
            let n = v.as_num();
            let shown = if step >= 1.0 {
                format!("{n:.0}{unit}")
            } else if unit.is_empty() {
                format!("{n:+.2}")
            } else {
                format!("{n:.2}{unit}")
            };
            let bar = crate::ui::slider::slider(
                iced::widget::Space::new().width(Length::Fill),
                ((n - min) / (max - min)).clamp(0.0, 1.0),
                22.0,
                (s.ink.scale_alpha(0.08 * s.opacity), s.accent),
                move |t| {
                    let raw = min + t * (max - min);
                    let v = ((raw / step).round() * step).clamp(min, max);
                    msg(Message::HyprSet(o, Value::Num(v), false))
                },
            )
            .on_release(msg(Message::HyprKeep(o)));
            row![
                container(bar).width(200),
                container(label(shown, text::SMALL, s.ink_dim))
                    .width(56)
                    .align_right(56),
            ]
            .spacing(tok::L)
            .align_y(Alignment::Center)
            .into()
        }
    };
    row_of(s, o.label, None, control_)
}

/// Seconds left before display changes go back (for the countdown).
pub fn display_countdown(st: &State) -> Option<u64> {
    st.pending
        .map(|(_, at)| KEEP_SECS.saturating_sub(at.elapsed().as_secs()))
}

/// "60 Hz" for a rate (a segment borrows its label).
fn hz_name(r: f32) -> &'static str {
    kept(format!("{} Hz", r.round() as u32))
}

/// A label made at run time, kept for the life of the shell so a segment can borrow it. Each
/// distinct text is made once (rates, percents: a handful), so this never grows with redraws.
fn kept(text: String) -> &'static str {
    static TEXTS: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut texts = TEXTS.lock().unwrap();
    if let Some(t) = texts.iter().find(|t| **t == text) {
        return t;
    }
    let t: &'static str = Box::leak(text.into_boxed_str());
    texts.push(t);
    t
}

/// Whether Settings is recording a key combination (its keys go to the recorder).
pub fn recording(st: &State) -> bool {
    st.recording && st.page == Page::Shortcuts
}

/// A key press as Hyprland writes it ("SUPER + SHIFT + T"); None for modifiers alone and
/// keys a shortcut can't use.
pub fn combo(key: &iced::keyboard::Key, m: iced::keyboard::Modifiers) -> Option<String> {
    use iced::keyboard::Key;
    use iced::keyboard::key::Named;
    let name = match key {
        Key::Character(c) => {
            let c = c.to_uppercase();
            match c.as_str() {
                "/" => "Slash".into(),
                "." => "Period".into(),
                "," => "Comma".into(),
                ";" => "Semicolon".into(),
                "'" => "Apostrophe".into(),
                "[" => "BracketLeft".into(),
                "]" => "BracketRight".into(),
                "-" => "Minus".into(),
                "=" => "Equal".into(),
                "`" => "Grave".into(),
                "\\" => "Backslash".into(),
                c => c.to_string(),
            }
        }
        Key::Named(n) => match n {
            Named::Enter => "Return".into(),
            Named::Space => "Space".into(),
            Named::Tab => "Tab".into(),
            Named::Backspace => "BackSpace".into(),
            Named::Delete => "Delete".into(),
            Named::Home => "Home".into(),
            Named::End => "End".into(),
            Named::PageUp => "Prior".into(),
            Named::PageDown => "Next".into(),
            Named::ArrowLeft => "Left".into(),
            Named::ArrowRight => "Right".into(),
            Named::ArrowUp => "Up".into(),
            Named::ArrowDown => "Down".into(),
            Named::Insert => "Insert".into(),
            Named::PrintScreen => "Print".into(),
            n => {
                let d = format!("{n:?}");
                // F1…F24.
                if d.starts_with('F') && d[1..].parse::<u8>().is_ok() {
                    d
                } else {
                    return None;
                }
            }
        },
        _ => return None,
    };
    let mut parts: Vec<&str> = Vec::new();
    if m.logo() {
        parts.push("SUPER");
    }
    if m.control() {
        parts.push("CTRL");
    }
    if m.alt() {
        parts.push("ALT");
    }
    if m.shift() {
        parts.push("SHIFT");
    }
    // A shortcut needs a modifier (a plain letter would stop typing it), F-keys aside.
    if parts.is_empty() && !name.starts_with('F') {
        return None;
    }
    parts.push(&name);
    Some(parts.join(" + "))
}

/// A control-centre sound action (one implementation of what each does).
fn snd(m: super::control::Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Control(m))
}
