//! Settings: kestrel's config.toml without opening it. Every change is written into the file
//! in place (comments kept) and applies at once through the live reload, so this panel and the
//! file never disagree. Opened from the gear in the control centre, or `kestrel toggle
//! settings`.

use super::action::Action as Act;
use super::cx::Cx;
use crate::app::Change;
use crate::config::{self, BarStyle};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
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
    /// The Wi-Fi list's and nearby devices' order while their page is open (as the control
    /// centre keeps its own: a rescan never moves the row under the pointer).
    wifi_order: Vec<String>,
    nearby_order: Vec<String>,
    /// A network waiting for its password: (ssid, access point, typed text).
    password: Option<(String, String, String)>,
    /// A hidden network being typed in: name and password.
    hidden: (String, String),
    /// The device whose Forget was pressed once (asks before removing).
    forget: Option<String>,
    /// Night light's warmth while the slider moves (kelvin).
    kelvin: Option<u32>,
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
    /// Firmware updates, once checked this session (None: not asked; fwupd is a daemon that
    /// stays once started, so it is asked only when the user presses Check).
    firmware: Option<Vec<crate::services::updates::Firmware>>,
    firmware_checking: bool,
    /// Developer: Restart was pressed once.
    restart_armed: bool,
    /// Mouse & touchpad: the pointing devices and the speeds set for them.
    pointers: Vec<String>,
    device_speeds: Vec<crate::hypr_settings::DeviceSpeed>,
    /// Bar: Reset was pressed once.
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
    MicRecorded(Result<std::sync::Arc<crate::services::sounds::Pcm>, String>),
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
    /// Ask fwupd for firmware updates (only on request: asking starts its daemon).
    CheckFirmware,
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
    /// A new secured network's password, typed and sent.
    Password(String),
    SubmitPassword,
    /// A hidden network's name and password, and joining it.
    HiddenName(String),
    HiddenPassword(String),
    JoinHidden,
    /// Forget a paired device: the first press asks, the second removes it.
    Forget(String),
    /// Night light's warmth (0 warmest, 1 coolest) while dragging; then kept.
    Warmth(f32),
    WarmthDone,
    /// Put text on the clipboard (the system report, an address).
    Copy(String),
    /// Read config.toml's module options, or the installed plugins, again (after a change).
    ReadDoc,
    ReadPlugins,
    /// A change to the system (a radio, the sound, a device), asked of the shell.
    Change(crate::app::Change),
    /// Several, in order.
    Batch(Vec<Message>),
    /// Close Settings' panel (after starting something in a terminal).
    Close,
    /// Show the wallpaper picker.
    OpenWallpaper,
}

/// config.toml as it is on disk (module options are read from it).
fn read_doc() -> toml::Table {
    std::fs::read_to_string(config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default()
}

pub fn open(st: &mut State, cx: &Cx<'_>) -> Action {
    // Each open starts fresh: no search left from last time.
    st.query.clear();
    st.doc = read_doc();
    st.city = cx.config.weather_city.clone().unwrap_or_default();
    st.plugins = crate::lua::installed();
    // The first open reads every page's data in the background, so switching pages shows a full
    // page at once (a page read only when chosen drew empty for a frame, then filled in). Later
    // opens keep that data and read again only the page shown: each page refreshes itself when
    // chosen. Updates is left to its page: checking for updates is slow and uses the network.
    let mut tasks: Vec<Task<Message>> = Vec::new();
    if !st.preloaded {
        st.preloaded = true;
        tasks.extend(
            SYSTEM
                .into_iter()
                .filter(|p| *p != Page::Updates && *p != st.page)
                .map(load),
        );
    }
    tasks.push(load(st.page));
    // Typing searches at once.
    tasks.push(Task::Iced(iced::widget::focus(SEARCH_ID.clone())));
    Act::Run(Task::batch(tasks))
}

/// The page scrolled to its end (where a rule opened from the overview is).
pub fn to_end<M: Send + 'static>() -> Task<M> {
    Task::Iced(iced::widget::snap_to(
        BODY_ID.clone(),
        iced::widget::scrollable::RelativeOffset { x: 0.0, y: 1.0 },
    ))
}

static BODY_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("settings-body"));

/// What a system page shows, read fresh when it opens (off the UI thread).
fn load(p: Page) -> Task<Message> {
    match p {
        Page::About => Task::perform(
            async {
                tokio::task::spawn_blocking(crate::services::about::read_all)
                    .await
                    .unwrap_or_default()
            },
            Message::About,
        ),
        Page::DateTime => Task::batch([
            Task::perform(crate::services::timedate::read(), Message::Time),
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
                |(r, now)| Message::Regions(r, now),
            ),
        ]),
        Page::Power => Task::batch([
            Task::perform(crate::services::power::lid(), Message::Lid),
            Task::perform(crate::services::power::low_battery(), |l| {
                Message::LowBattery(l)
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
                |(h, c, i)| Message::Power(h, c, i),
            ),
        ]),
        Page::Account => Task::perform(
            async {
                tokio::task::spawn_blocking(crate::services::about::account)
                    .await
                    .unwrap_or_default()
            },
            Message::Account,
        ),
        Page::Storage => Task::perform(crate::services::disks::list(), Message::Volumes),
        Page::Privacy => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    use crate::services::gsettings as g;
                    const P: &str = "org.gnome.desktop.privacy";
                    (
                        crate::services::idle::lock_on_sleep(),
                        g::get_bool(P, "remember-recent-files").unwrap_or(true),
                        g::get_i64(P, "recent-files-max-age")
                            .and_then(|v| i32::try_from(v).ok())
                            .unwrap_or(-1),
                    )
                })
                .await
                .unwrap_or((None, true, -1))
            },
            |(l, r, d)| Message::Privacy(l, r, d),
        ),
        Page::Appearance => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    let (fonts, monos) = crate::look::fonts();
                    let get = |k: &str| {
                        crate::services::gsettings::get_string("org.gnome.desktop.interface", k)
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
            Message::Look,
        ),
        Page::Network => Task::batch([
            Task::perform(
                async {
                    tokio::join!(
                        crate::services::network::primary(),
                        crate::services::network::hotspot()
                    )
                },
                |(p, h)| Message::NetRead(p, h),
            ),
            // Networks in range, fresh (NetworkManager scans rarely while connected).
            Task::future(crate::services::network::command(
                crate::services::network::Command::Scan,
            ))
            .discard(),
        ]),
        Page::Updates => Task::perform(crate::services::updates::list(), Message::Updates),
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
            |(l, a)| Message::Startup(l, a),
        ),
        Page::Displays => Task::perform(crate::services::displays::read(), Message::Displays),
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
                |(t, ts, es)| Message::Terminals(t, ts, es),
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
                |(a, n)| Message::Defaults(a, n),
            ),
        ]),
        Page::Shortcuts => Task::perform(crate::services::hyprland::binds(), Message::Binds),
        Page::Access => Task::batch([
            Task::perform(crate::services::audio::mono(), Message::Mono),
            Task::perform(crate::hypr_settings::text_scale(), |f| {
                Message::TextScale(f)
            }),
            Task::perform(
                crate::hypr_settings::read(crate::hypr_settings::ACCESS.to_vec()),
                Message::Hypr,
            ),
        ]),
        Page::Windows => {
            use crate::hypr_settings::{WINDOW_BEHAVIOUR, WINDOW_LOOK};
            Task::batch([
                Task::perform(
                    crate::hypr_settings::read([WINDOW_LOOK, WINDOW_BEHAVIOUR].concat()),
                    Message::Hypr,
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
                    |(r, a)| Message::Rules(r, a),
                ),
            ])
        }
        Page::Mouse => {
            use crate::hypr_settings::{KEYBOARD, MOUSE, Opt, TOUCHPAD};
            let mut opts: Vec<Opt> = [KEYBOARD, MOUSE, TOUCHPAD].concat();
            opts.push(LAYOUTS);
            opts.push(crate::hypr_settings::KB_OPTIONS);
            Task::batch([
                Task::perform(crate::hypr_settings::read(opts), Message::Hypr),
                Task::perform(
                    async {
                        let devices = crate::services::hyprland::pointer_devices().await;
                        let speeds =
                            tokio::task::spawn_blocking(crate::hypr_settings::device_speeds)
                                .await
                                .unwrap_or_default();
                        (devices, speeds)
                    },
                    |(d, sp)| Message::Pointers(d, sp),
                ),
            ])
        }
        Page::Keyboard => {
            use crate::hypr_settings::{KEYBOARD, MOUSE, Opt, TOUCHPAD};
            let mut opts: Vec<Opt> = [KEYBOARD, MOUSE, TOUCHPAD].concat();
            opts.push(LAYOUTS);
            opts.push(crate::hypr_settings::KB_OPTIONS);
            Task::perform(crate::hypr_settings::read(opts), Message::Hypr)
        }
        _ => Task::none(),
    }
}

/// The network changed: the Network page, when shown, reads its connection and hotspot again
/// (another network's IP settings, a hotspot started).
pub fn network_changed(
    st: &State,
    now: &crate::services::network::State,
    before: &crate::services::network::State,
) -> Action {
    let moved = now.link != before.link
        || now.ssid != before.ssid
        || now.hotspot != before.hotspot
        || now.wired.as_ref().map(|w| w.connected) != before.wired.as_ref().map(|w| w.connected);
    if moved && st.page == Page::Network {
        Act::Run(load(Page::Network))
    } else {
        Act::None
    }
}

impl State {
    /// The Wi-Fi networks or nearby devices changed: the lists keep their rows where they
    /// are; found ones join at the end (called by the shell after each report).
    pub fn lists_changed(
        &mut self,
        net: &crate::services::network::State,
        bt: &crate::services::bluetooth::State,
    ) {
        crate::panels::control::keep_order(
            &mut self.wifi_order,
            net.networks.iter().map(|n| n.ssid.as_str()),
        );
        crate::panels::control::keep_order(
            &mut self.nearby_order,
            bt.nearby.iter().map(|d| d.path.as_str()),
        );
    }
}

/// One setting in config.toml, changed by the shell (written, then applied at once).
fn setting(table: &[&str], key: &str, value: toml_edit::Item) -> Action {
    Act::Change(Change::Settings(vec![config::Edit::new(table, key, value)]))
}

type Action = Act<Message>;

/// What a message asks of the shell (settings, the power mode, joining, files, terminals),
/// and everything else as Settings' own work.
pub fn update(st: &mut State, m: Message, cx: &Cx<'_>) -> Action {
    let run = |t: Task<Message>| Act::Run(t);
    match m {
        Message::WeekStart(sunday) => {
            setting(&["clock"], "week_starts_sunday", toml_edit::value(sunday))
        }
        Message::Profile(p) => Act::Change(Change::Profile(p)),
        Message::Quiet(q) => {
            st.quiet_open = None;
            let (from, to) = q.unzip();
            Act::Change(Change::Settings(vec![
                config::Edit::new(
                    &[],
                    "quiet_from",
                    toml_edit::value(from.unwrap_or_default()),
                ),
                config::Edit::new(&[], "quiet_to", toml_edit::value(to.unwrap_or_default())),
            ]))
        }
        Message::SoundStep(v) => setting(&["sound"], "step", toml_edit::value(i64::from(v))),
        Message::SoundMax(v) => setting(&["sound"], "max", toml_edit::value(i64::from(v))),
        Message::SoundOpt(key, v) => setting(&["sound"], key, toml_edit::Item::Value(v)),
        Message::CursorSize(n) => {
            setting(&[], "cursor_size", toml_edit::value(i64::from(n)))
                .and(run(
                    Task::future(crate::hypr_settings::apply_cursor(n)).discard()
                ))
        }
        Message::NotifyRule(name, rule) => {
            let rules = &cx.config.notifications;
            let without = |l: &[String]| -> Vec<String> {
                l.iter().filter(|a| **a != name).cloned().collect()
            };
            let (mut muted, mut quiet) = (without(&rules.muted), without(&rules.quiet));
            match rule {
                1 => quiet.push(name),
                2 => muted.push(name),
                _ => {}
            }
            Act::Change(Change::Settings(vec![
                config::Edit::new(&["notifications"], "muted", config::list(&muted)),
                config::Edit::new(&["notifications"], "quiet", config::list(&quiet)),
            ]))
        }
        Message::NotifyOpt(key, v) => setting(&["notifications"], key, toml_edit::Item::Value(v)),
        Message::NotifySilent(name, sound) => {
            let mut silent: Vec<String> = cx
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
            setting(&["notifications"], "silent", config::list(&silent))
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
            Act::Ask(req).and(run(Task::perform(rx, |a| {
                Message::Picture(match a {
                    Ok(crate::services::filechooser::Answer::Chosen(v)) => v.into_iter().next(),
                    _ => None,
                })
            })))
        }
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
            Act::Dispatch(crate::services::compositor::Action::Open(
                path.to_string_lossy().into(),
            ))
        }
        Message::FirmwareUpdate(id) => {
            // The device id is fwupd's hex: nothing else reaches the shell.
            let id: String = id.chars().filter(char::is_ascii_hexdigit).collect();
            let line = crate::services::defaults::in_terminal(
                &format!("sh -c 'fwupdmgr update {id}; echo; read -p \"Press Enter to close.\" _'"),
                Some("Firmware"),
            );
            Act::Dispatch(crate::services::compositor::Action::Exec(line))
        }
        Message::InTerminal(cmd, title) => {
            let line = crate::services::defaults::in_terminal(cmd, Some(title));
            Act::Dispatch(crate::services::compositor::Action::Exec(line))
        }
        Message::EditorSet(e) => {
            let line = format!("hl.env(\"EDITOR\", \"{e}\") hl.env(\"VISUAL\", \"{e}\")");
            setting(&[], "editor", toml_edit::value(e.clone())).and(run(Task::future(async move {
                crate::services::hyprland::eval(line).await;
                crate::hypr_settings::write_lua();
            })
            .discard()))
        }
        Message::Clear(which) => {
            if st.clear_armed != Some(which) {
                st.clear_armed = Some(which);
                return Act::None;
            }
            st.clear_armed = None;
            // The clipboard panel reads its list again whenever it opens.
            run(Task::perform(
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
                        // Pinned entries stay, as with the clipboard panel's "Clear all".
                        tokio::task::spawn_blocking(crate::services::clips::clear)
                            .await
                            .map_err(|_| "The clipboard history was not cleared".to_string())
                    }
                },
                Message::Done,
            ))
        }
        Message::LookSet(which, name) => {
            st.look_open = None;
            let key = ["font", "mono", "icons", "cursor"][usize::from(which.min(3))];
            let look = {
                let mut l = cx.config.look.clone();
                // The other font (and sizes) stay as they are in use now.
                let now = st.look.as_ref();
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
            if let Some(lists) = &mut st.look {
                match which {
                    2 => lists.icons_now = name.clone(),
                    3 => lists.cursor_now = name.clone(),
                    _ => {}
                }
            }
            let size = cx.config.cursor_size.unwrap_or(24);
            setting(&["look"], key, toml_edit::value(name.clone())).and(run(Task::future(
                async move {
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
                },
            )
            .discard()))
        }
        Message::LookSize(which, n) => {
            let key = if which == 0 { "font_size" } else { "mono_size" };
            let mut l = cx.config.look.clone();
            // A size alone keeps the font in use (not kestrel's default in its place).
            let now = st.look.as_ref();
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
            setting(&["look"], key, toml_edit::value(i64::from(n))).and(run(Task::future(
                async move {
                    let ((f, fs), (m, ms)) = (l.font(), l.mono());
                    crate::look::apply_fonts(f, fs, m, ms).await;
                },
            )
            .discard()))
        }
        Message::Accent(a) => setting(
            &[],
            "accent",
            match a {
                Some(hex) => toml_edit::value(hex),
                None => toml_edit::Item::None,
            },
        ),
        Message::Units(u) => setting(&[], "weather_units", toml_edit::value(u)),
        Message::Style(b) => setting(
            &["bar"],
            "style",
            toml_edit::value(match b {
                BarStyle::Full => "full",
                BarStyle::Pills => "pills",
            }),
        ),
        Message::Workspaces(n) => setting(&[], "workspaces", toml_edit::value(n.clamp(1, 10))),
        Message::ThemeApps(on) => setting(&[], "theme_apps", toml_edit::value(on)),
        Message::Material(m) => setting(
            &[],
            "material",
            toml_edit::value(match m {
                crate::config::Material::Black => "black",
                crate::config::Material::Frosted => "frosted",
            }),
        ),
        Message::SubmitCity => {
            let c = st.city.trim().to_string();
            setting(&[], "weather_city", toml_edit::value(c))
        }
        Message::Opt(m, o, v) => {
            let Some(opt) = crate::modules::MODULES
                .get(m)
                .and_then(|m| m.options.get(o))
            else {
                return Act::None;
            };
            let item = match v {
                OptValue::Bool(b) => toml_edit::value(b),
                OptValue::Str(t) => toml_edit::value(t),
            };
            Act::ChangeThen(
                Change::Settings(vec![config::Edit::new(opt.table, opt.key, item)]),
                Message::ReadDoc,
            )
        }
        Message::Plugin(id, on) => Act::ChangeThen(
            Change::Settings(vec![config::Edit::new(
                &["plugins", &id],
                "enabled",
                toml_edit::value(on),
            )]),
            Message::ReadPlugins,
        ),
        Message::BarMove(id, d) => {
            let mut l = layout(cx);
            if let Some((sec, at)) = find_in(&l, &id) {
                let to = at as i32 + d;
                if to >= 0 && (to as usize) < l[sec].len() {
                    l[sec].swap(at, to as usize);
                    return save_layout(&l);
                }
            }
            Act::None
        }
        Message::BarSection(id, d) => {
            let mut l = layout(cx);
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
                    return save_layout(&l);
                }
            }
            Act::None
        }
        Message::BarRemove(id) => {
            let mut l = layout(cx);
            for list in &mut l {
                list.retain(|m| *m != id);
            }
            save_layout(&l)
        }
        Message::BarAdd(id) => {
            let mut l = layout(cx);
            let sec = match crate::modules::info(&id).map(|m| m.home) {
                Some(crate::modules::Home::Left) => 0,
                Some(crate::modules::Home::Center) => 1,
                _ => 2,
            };
            if find_in(&l, &id).is_some() {
                return Act::None;
            }
            // The right section's end is the power button's place: added before it.
            let at = if sec == 2 {
                l[2].iter().position(|m| m == "power").unwrap_or(l[2].len())
            } else {
                l[sec].len()
            };
            l[sec].insert(at, id);
            save_layout(&l)
        }
        Message::BarReset => {
            if !st.bar_reset_armed {
                st.bar_reset_armed = true;
                return Act::None;
            }
            st.bar_reset_armed = false;
            let d = crate::config::BarConfig::default();
            save_layout(&[d.left, d.center, d.right])
        }
        Message::VolumeOpen(path, mount) => match mount {
            Some(m) => open_folder(m),
            None => run(Task::perform(
                crate::services::disks::mount(path),
                Message::Mounted,
            )),
        },
        Message::Mounted(r) => match r {
            Ok(m) => open_folder(m).and(run(load(Page::Storage))),
            Err(e) => {
                st.error = Some(e);
                Act::None
            }
        },
        // Joining as the control centre does: at once, or after its password.
        Message::JoinNet(n) => {
            if n.active {
                return Act::None;
            }
            if let Some(c) = Change::join(&n) {
                return Act::Change(c);
            }
            st.password = Some((n.ssid, n.ap, String::new()));
            run(Task::Iced(iced::widget::focus(iced::widget::Id::new(
                "net-password",
            ))))
        }
        Message::SubmitPassword => match st.password.take() {
            Some((ssid, ap, pw)) if !pw.is_empty() => Act::Change(Change::Join(
                ssid.clone(),
                crate::services::network::Command::ConnectNew {
                    ssid,
                    ap,
                    password: Some(pw),
                },
            )),
            _ => Act::None,
        },
        Message::JoinHidden => {
            let (ssid, pw) = std::mem::take(&mut st.hidden);
            if ssid.trim().is_empty() {
                return Act::None;
            }
            Act::Change(Change::Join(
                ssid.clone(),
                crate::services::network::Command::ConnectHidden {
                    ssid,
                    password: (!pw.is_empty()).then_some(pw),
                },
            ))
        }
        Message::Forget(path) => {
            if st.forget.as_deref() != Some(path.as_str()) {
                st.forget = Some(path);
                return Act::None;
            }
            st.forget = None;
            Act::Change(Change::ForgetDevice(path))
        }
        Message::WarmthDone => match st.kelvin.take() {
            Some(k) => Act::Change(Change::NightTemperature(k)),
            None => Act::None,
        },
        Message::Change(c) => Act::Change(c),
        Message::Batch(v) => Act::Many(v.into_iter().map(|m| update(st, m, cx)).collect()),
        Message::Close => Act::Close,
        Message::OpenWallpaper => Act::Command("toggle wallpaper".into()),
        Message::Copy(text) => {
            run(Task::future(crate::panels::clipboard::copy_text(text)).discard())
        }
        m => run(own(st, m, cx)),
    }
}

/// Settings' own work: its state and its tasks (what needs the shell is in `update`).
fn own(st: &mut State, m: Message, cx: &Cx<'_>) -> Task<Message> {
    match m {
        Message::Page(p) => {
            st.page = p;
            st.query.clear();
            st.error = None;
            st.zones_open = false;
            if p == Page::Bluetooth {
                st.nearby_order.clear();
            }
            if p == Page::Network {
                // Networks sorted fresh on opening, then kept in place (as the control
                // centre's Wi-Fi page).
                st.wifi_order.clear();
                crate::panels::control::keep_order(
                    &mut st.wifi_order,
                    cx.net.networks.iter().map(|n| n.ssid.as_str()),
                );
            }
            // Each page starts at its top (the last page's scroll carried over).
            return Task::batch([
                load(p),
                Task::Iced(iced::widget::scroll_to(
                    BODY_ID.clone(),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                )),
            ]);
        }
        Message::About(a) => st.about = Some(a),
        Message::Time(t) => st.time = Some(t),
        Message::Ntp(on) => {
            if let Some(t) = &mut st.time {
                t.ntp = on;
            }
            return Task::perform(crate::services::timedate::set_ntp(on), Message::Done);
        }
        Message::ZonesOpen(open) => {
            st.zones_open = open;
            st.zone_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("zone-search")));
            }
        }
        Message::ZoneQuery(q) => st.zone_query = q,
        Message::Zone(z) => {
            st.zones_open = false;
            if let Some(t) = &mut st.time {
                t.timezone = z.clone();
            }
            return Task::perform(crate::services::timedate::set_timezone(z), |r| {
                Message::Done(r)
            });
        }
        Message::Hypr(v) => st.hypr.extend(v),
        Message::HyprSet(o, v, save) => {
            st.hypr.insert(o.option, v.clone());
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
            if let Some(v) = st.hypr.get(o.option).cloned() {
                return Task::future(crate::hypr_settings::save(o, v)).discard();
            }
        }
        Message::LayoutsOpen(open) => {
            st.layouts_open = open;
            st.layout_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("layout-search")));
            }
        }
        Message::LayoutQuery(q) => st.layout_query = q,
        Message::Xkb(k, v) => {
            let o = crate::hypr_settings::KB_OPTIONS;
            let now = st
                .hypr
                .get(o.option)
                .map(crate::hypr_settings::Value::as_str)
                .unwrap_or_default();
            let new = crate::hypr_settings::xkb_with(&now, &crate::hypr_settings::XKB[k], &v);
            let v = crate::hypr_settings::Value::Str(new);
            st.hypr.insert(o.option, v.clone());
            return Task::future(crate::hypr_settings::save(o, v)).discard();
        }
        Message::Defaults(apps, now) => {
            st.apps = apps;
            st.defaults = now;
        }
        Message::DefaultOpen(k) => st.default_open = k,
        Message::Power(h, c, idle) => {
            st.health = h;
            st.charging = c;
            st.idle = idle;
        }
        Message::Charging(c) => {
            if let Some((_, now)) = &mut st.charging {
                *now = c;
            }
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::services::power::set_charging(c))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::IdleOpen(step) => st.idle_open = step,
        Message::Displays(d) => {
            if st.pending.is_none() {
                st.kept = d.clone();
                st.displays = d;
            }
        }
        Message::ModeOpen(i) => st.mode_open = i,
        Message::Display(i, change) => return change_display(st, i, change),
        Message::DisplayKeep => {
            st.pending = None;
            st.kept = st.displays.clone();
            let all = st.displays.clone();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::services::displays::save(&all))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::DisplayRevert => return revert_displays(st),
        Message::DisplayTimeout(n) => {
            if st.pending.is_some_and(|(p, _)| p == n) {
                return revert_displays(st);
            }
        }
        Message::DisplayTick => {}
        Message::Startup(list, apps) => {
            st.startup = list;
            if !apps.is_empty() {
                st.apps = apps;
            }
        }
        Message::StartupOn(id, on) => {
            if let Some(e) = st.startup.iter_mut().find(|e| e.id == id) {
                e.enabled = on;
            }
            return startup_job(move || crate::services::autostart::set_enabled(&id, on));
        }
        Message::StartupRemove(id) => {
            st.startup.retain(|e| e.id != id);
            return startup_job(move || crate::services::autostart::remove(&id));
        }
        Message::StartupAddOpen(open) => {
            st.startup_add = open;
            st.startup_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("startup-search")));
            }
        }
        Message::StartupQuery(q) => st.startup_query = q,
        Message::Updates(v) => st.updates = Some(v),
        Message::Volumes(v) => {
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
                    Message::Usage(t, v)
                });
            }
        }
        Message::Usage(total, items) => {
            st.measuring = false;
            st.measured = Some(std::time::Instant::now());
            st.usage = Some((total, items));
        }
        Message::Account(a) => st.account = Some(a),
        Message::FullName(t) => {
            let open = t.is_some() && st.full_name.is_none();
            st.full_name = t;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("full-name")));
            }
        }
        Message::Picture(p) => {
            if let Some(p) = p {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || set_face(&p))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::Done,
                );
            }
        }
        Message::FullNameSubmit => {
            if let Some(n) = st.full_name.take() {
                return Task::perform(crate::services::about::set_full_name(n), |r| {
                    Message::Done(r)
                });
            }
        }
        Message::VolumeEject(path, drive) => {
            return Task::perform(crate::services::disks::eject(path, drive), |r| {
                Message::Done(r)
            });
        }
        Message::StartupAdd(id) => {
            st.startup_add = false;
            return startup_job(move || {
                let f = crate::services::apps::desktop_file(&id).ok_or("app not found")?;
                crate::services::autostart::add(&f)
            });
        }
        Message::Rename(t) => {
            let open = t.is_some() && st.rename.is_none();
            st.rename = t;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("rename")));
            }
        }
        Message::RenameSubmit => {
            if let Some(name) = st.rename.take()
                && !name.trim().is_empty()
            {
                return Task::perform(crate::services::about::rename(name), Message::Done);
            }
        }
        Message::QuietOpen(w) => st.quiet_open = w,
        Message::Binds(b) => {
            st.binds = b;
            st.custom = crate::hypr_settings::shortcuts();
        }
        Message::Record(on) => {
            st.recording = on;
            st.rebinding = None;
            if on {
                st.new_keys = None;
            }
        }
        Message::Rebind(old) => {
            st.recording = true;
            st.error = None;
            st.rebinding = Some(old);
        }
        Message::Recorded(k) => {
            st.recording = false;
            if let Some(old) = st.rebinding.take() {
                if let Some(what) = taken_by(st, &k) {
                    st.error = Some(format!("{k} is already {what}"));
                    return Task::none();
                }
                return Task::perform(crate::hypr_settings::rebind(old, k), Message::Done);
            }
            st.new_keys = Some(k);
        }
        Message::NewName(t) => st.new_name = t,
        Message::NewCommand(t) => st.new_command = t,
        Message::AddShortcut => {
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
                        Message::Binds,
                    ),
                ]);
            }
        }
        Message::RemoveShortcut(keys) => {
            st.custom.retain(|c| c.keys != keys);
            return Task::batch([
                Task::future(crate::hypr_settings::remove_shortcut(keys)).discard(),
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        crate::services::hyprland::binds().await
                    },
                    Message::Binds,
                ),
            ]);
        }
        Message::SoundPick(k) => st.sound_pick = k,
        Message::TextScale(f) => {
            st.text_scale = f;
            return Task::future(crate::hypr_settings::set_text_scale(f)).discard();
        }
        Message::NetRead(primary, hot) => {
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
        Message::IpEdit(e) => st.ip_edit = e,
        Message::IpApply => {
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
                Message::Done,
            );
        }
        Message::HotspotEdit(n, p) => st.hotspot_edit = (n, p),
        Message::HotspotOn(on) => {
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
                Message::Done(r)
            });
        }
        Message::ForgetNet(ssid) => {
            if st.forget_net.as_deref() != Some(ssid.as_str()) {
                st.forget_net = Some(ssid);
                return Task::none();
            }
            st.forget_net = None;
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::Forget(
                    ssid,
                )),
                Message::Done,
            );
        }
        Message::AutoJoinNet(ssid, on) => {
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::AutoJoin(
                    ssid, on,
                )),
                Message::Done,
            );
        }
        Message::Terminals(t, ts, es) => {
            st.terminal = Some(t);
            st.terminals = ts;
            st.editors = es;
        }
        Message::Regions(r, now) => {
            st.regions = r;
            st.region_now = now;
        }
        Message::RegionOpen(open) => {
            st.region_open = open;
            st.region_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("region-search")));
            }
        }
        Message::RegionQuery(q) => st.region_query = q,
        Message::RegionSet(r) => {
            st.region_open = false;
            return Task::perform(crate::services::region::set(r), Message::Done);
        }
        Message::RegionReset => {
            st.region_open = false;
            return Task::perform(crate::services::region::reset(), Message::Done);
        }
        Message::Pointers(d, sp) => {
            st.pointers = d;
            st.device_speeds = sp;
        }
        Message::DeviceSpeed(name, v, keep) => {
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
        Message::Mono(m) => st.mono = m,
        Message::MonoSet(on) => {
            st.mono = Some(on);
            return Task::future(crate::services::audio::set_mono(on)).discard();
        }
        Message::SpeakerTest => {
            st.speaker_test = true;
            return Task::perform(crate::services::sounds::speaker_test(), |()| {
                Message::SpeakerDone
            });
        }
        Message::SpeakerDone => st.speaker_test = false,
        Message::MicTest => {
            st.mic_test = 1;
            return Task::perform(crate::services::sounds::mic_record(3), |r| {
                Message::MicRecorded(r)
            });
        }
        Message::MicRecorded(r) => {
            let pcm = match r {
                Ok(p) => p,
                Err(e) => {
                    st.mic_test = 0;
                    st.error = Some(e);
                    return Task::none();
                }
            };
            st.mic_test = 2;
            return Task::perform(crate::services::sounds::mic_playback(pcm), |()| {
                Message::MicDone
            });
        }
        Message::MicDone => st.mic_test = 0,
        Message::AlertPlay => {
            let v = cx.config.sound.alert_volume();
            return Task::future(async move {
                crate::services::sounds::play_now(crate::services::sounds::MESSAGE, v).await;
            })
            .discard();
        }
        Message::Restart => {
            if !st.restart_armed {
                st.restart_armed = true;
                return Task::none();
            }
            st.restart_armed = false;
            return Task::future(async {
                if let Err(e) = crate::services::systemd::restart_user_unit("kestrel.service").await
                {
                    log::warn!("restart: {e}");
                }
            })
            .discard();
        }
        Message::Firmware(v) => {
            st.firmware = Some(v);
            st.firmware_checking = false;
        }
        Message::CheckFirmware => {
            st.firmware_checking = true;
            return Task::perform(crate::services::updates::firmware(), |v| {
                Message::Firmware(v)
            });
        }
        Message::TerminalSet(id) => {
            st.terminal = Some(id.clone());
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::services::defaults::set_terminal(&id)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::Privacy(l, r, d) => {
            st.privacy = Some((l, r, d));
            st.clear_armed = None;
        }
        Message::LockOnSleep(on) => {
            if let Some(p) = &mut st.privacy {
                p.0 = Some(on);
            }
            return Task::perform(crate::services::idle::set_lock_on_sleep(on), |r| {
                Message::Done(r)
            });
        }
        Message::RememberRecent(on) => {
            if let Some(p) = &mut st.privacy {
                p.1 = on;
            }
            return Task::future(gsettings_privacy(
                "remember-recent-files",
                crate::services::gsettings::Set::Bool(on),
            ))
            .discard();
        }
        Message::RecentDays(d) => {
            if let Some(p) = &mut st.privacy {
                p.2 = d;
            }
            return Task::future(gsettings_privacy(
                "recent-files-max-age",
                crate::services::gsettings::Set::Int(i64::from(d)),
            ))
            .discard();
        }
        Message::Look(l) => st.look = Some(l),
        Message::LookOpen(which) => {
            st.look_open = which;
            st.look_query.clear();
            if which.is_some() {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("look-search")));
            }
        }
        Message::LookQuery(q) => st.look_query = q,
        Message::Lid(l) => st.lid = l,
        Message::LowBattery(l) => st.low = l,
        Message::LowSet(warn, action) => {
            if let Some(l) = &mut st.low {
                l.warn_at = warn;
                l.action = action.clone();
            }
            return Task::perform(
                async move { crate::services::power::set_low_battery(warn, &action).await },
                Message::Done,
            );
        }
        Message::LidSet(l) => {
            // Shown at once; logind is changed once the password is given (or it reads back).
            st.lid = Some(l);
            return Task::perform(crate::services::power::set_lid(l), Message::Done);
        }
        Message::Rules(r, a) => {
            // A rule opened before the saved ones were read (from the overview): its saved
            // form, if it has one.
            if let Some(e) = &st.rule_edit
                && let Some(saved) = r.iter().find(|x| x.class == e.class)
            {
                st.rule_edit = Some(saved.clone());
            }
            st.rules = r;
            st.rule_apps = a;
        }
        Message::RulePick(q) => {
            let start = st.rule_pick.is_none() && q.is_some();
            st.rule_pick = q;
            st.rule_edit = None;
            if start {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("rule-search")));
            }
        }
        Message::RuleOpen(class) => {
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
            match st.rules.iter_mut().find(|x| x.class == r.class) {
                Some(x) => *x = r.clone(),
                None => st.rules.push(r.clone()),
            }
            st.rule_edit = Some(r.clone());
            return Task::future(crate::hypr_settings::set_window_rule(r)).discard();
        }
        Message::RuleRemove(class) => {
            st.rules.retain(|r| r.class != class);
            st.rule_edit = None;
            return Task::future(crate::hypr_settings::remove_window_rule(class)).discard();
        }
        Message::BtRename(target, now) => {
            st.bt_rename = Some((target, now));
            return Task::Iced(iced::widget::focus(iced::widget::Id::new("bt-rename")));
        }
        Message::BtRenameText(t) => {
            if let Some((_, text)) = &mut st.bt_rename {
                *text = t;
            }
        }
        Message::BtRenameDone => {
            let Some((target, name)) = st.bt_rename.take() else {
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
                Message::Done(r)
            });
        }
        Message::HiddenOpen(open) => {
            st.hidden_open = open;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("hidden-name")));
            }
        }
        Message::Idle(step, secs) => {
            st.idle_open = None;
            if let Some(e) = st.idle.iter_mut().find(|(s, _)| *s == step) {
                e.1 = Some(secs);
            }
            return Task::perform(crate::services::idle::set(step, secs), Message::Done);
        }
        Message::DefaultSet(k, id) => {
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
                Message::Done,
            );
        }
        Message::LayoutAdd(code) => return change_layouts(st, code, true),
        Message::LayoutRemove(code) => return change_layouts(st, code, false),
        Message::Done(r) => {
            st.error = r.err();
            return load(st.page);
        }
        Message::Query(q) => {
            // The first letter typed reads the quick system pages, so their settings are found.
            let first = st.query.is_empty() && !q.is_empty();
            st.query = q;
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
        Message::City(c) => st.city = c,
        // Asked of the shell: see `update`.
        Message::WeekStart(..)
        | Message::Profile(..)
        | Message::Quiet(..)
        | Message::SoundStep(..)
        | Message::SoundMax(..)
        | Message::SoundOpt(..)
        | Message::CursorSize(..)
        | Message::NotifyRule(..)
        | Message::PickPicture
        | Message::OpenFile(..)
        | Message::FirmwareUpdate(..)
        | Message::InTerminal(..)
        | Message::EditorSet(..)
        | Message::Clear(..)
        | Message::LookSet(..)
        | Message::LookSize(..)
        | Message::Accent(..)
        | Message::JoinNet(..)
        | Message::NotifyOpt(..)
        | Message::NotifySilent(..)
        | Message::Units(..)
        | Message::Style(..)
        | Message::Workspaces(..)
        | Message::ThemeApps(..)
        | Message::Material(..)
        | Message::SubmitCity
        | Message::Opt(..)
        | Message::Plugin(..)
        | Message::BarMove(..)
        | Message::BarSection(..)
        | Message::BarRemove(..)
        | Message::BarAdd(..)
        | Message::BarReset
        | Message::VolumeOpen(..)
        | Message::Mounted(..)
        | Message::SubmitPassword
        | Message::JoinHidden
        | Message::Forget(..)
        | Message::WarmthDone
        | Message::Copy(..)
        | Message::Change(..)
        | Message::Batch(..)
        | Message::Close
        | Message::OpenWallpaper => {}
        Message::Password(p) => {
            if let Some(pw) = &mut st.password {
                pw.2 = p;
            }
        }
        Message::HiddenName(t) => st.hidden.0 = t,
        Message::HiddenPassword(t) => st.hidden.1 = t,
        // Warmth follows the slider at once (night light's own tool); kept when let go.
        Message::Warmth(v) => {
            use crate::panels::control::{COOLEST, WARMEST};
            let k = (WARMEST + v.clamp(0.0, 1.0) * (COOLEST - WARMEST)).round() as u32 / 100 * 100;
            st.kelvin = Some(k);
            return Task::future(crate::services::toggles::set_temperature(k)).discard();
        }
        Message::ReadDoc => st.doc = read_doc(),
        Message::ReadPlugins => st.plugins = crate::lua::installed(),
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
    control_: Element<'a, Message>,
) -> Option<Element<'a, Message>> {
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
    rows: Vec<Option<Element<'a, Message>>>,
) -> Option<Element<'a, Message>> {
    let rows: Vec<Element<'a, Message>> = rows.into_iter().flatten().collect();
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
                            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
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
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            border: iced::Border::default().rounded(tok::R_CARD),
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
fn segments<'a>(s: Scheme, options: &[(&'a str, bool, Message)]) -> Element<'a, Message> {
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
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
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
pub fn window<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme.faded(1.0);
    let body = container(view(st, &cx.with_scheme(s)))
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

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let page = st.page;
    let shell_pages: Vec<Page> = SHELL
        .into_iter()
        .filter(|p| *p != Page::Plugins || !st.plugins.is_empty())
        .collect();
    // Sidebar: the pages, the open one on a soft neutral fill.
    let search = container(
        row![
            icon(Icon::Search, tok::ICON, s.ink_dim),
            text_input("Search", &st.query)
                .id(SEARCH_ID.clone())
                .on_input(Message::Query)
                .padding([tok::S, tok::XS])
                .size(text::BODY)
                .font(theme::FONT)
                .style(crate::ui::input_style(s))
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
            bottom: tok::M,
            ..iced::Padding::ZERO
        }),
    ]
    .spacing(tok::XXS);
    let heading = |t: &'static str| {
        container(label_w(t, text::CAPTION, s.ink_faint, theme::SEMIBOLD)).padding(iced::Padding {
            top: tok::L,
            right: tok::M,
            bottom: tok::XS,
            left: tok::M,
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
                    icon(p.icon(), tok::ICON, fg),
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
            .radius(tok::R_ROW)
            .kind(
                s,
                if on {
                    crate::ui::control::Kind::Chosen
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .no_press_scale()
            .on_press(Message::Page(p)),
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
    let q = st.query.trim().to_lowercase();
    QUERY.with(|cell| *cell.borrow_mut() = q.clone());
    let body = if q.is_empty() {
        column![label_w(page.name(), text::HEADING, s.ink, theme::SEMIBOLD)]
            .spacing(tok::XL)
            .extend(match page {
                Page::Bar => bar_page(st, cx),
                Page::Appearance => appearance_page(st, cx),
                Page::Time => time_page(st, cx),
                Page::Plugins => plugins_page(st, cx),
                Page::Keyboard => keyboard_page(st, cx),
                Page::Mouse => mouse_page(st, cx),
                Page::Defaults => defaults_page(st, cx),
                Page::Power => power_page(st, cx),
                Page::Access => access_page(st, cx),
                Page::Shortcuts => shortcuts_page(st, cx),
                Page::Displays => displays_page(st, cx),
                Page::Windows => windows_page(st, cx),
                Page::Notifications => notifications_page(st, cx),
                Page::Privacy => privacy_page(st, cx),
                Page::Startup => startup_page(st, cx),
                Page::Updates => updates_page(st, cx),
                Page::Storage => storage_page(st, cx),
                Page::Network => network_page(st, cx),
                Page::Bluetooth => bluetooth_page(st, cx),
                Page::Account => account_page(st, cx),
                Page::Sound => sound_page(st, cx),
                Page::DateTime => datetime_page(st, cx),
                Page::Developer => developer_page(st, cx),
                Page::About => about_page(st, cx),
            })
    } else {
        // Pages first: by name or by what they hold ("wifi" → Network, "blue" → Bluetooth), the
        // same words the launcher finds them by; a click opens the page.
        let pages: Vec<Page> = Page::all()
            .filter(|p| {
                p.name().to_lowercase().contains(&q)
                    || p.words().split_whitespace().any(|w| w.starts_with(&q))
            })
            .collect();
        let page_rows = (!pages.is_empty()).then(|| {
            let mut c = column![].spacing(tok::XXS);
            for p in pages {
                c = c.push(
                    control(
                        row![
                            icon(p.icon(), tok::ICON, s.ink_dim),
                            label_w(p.name(), text::BODY, s.ink, theme::MEDIUM).width(Length::Fill),
                            icon(Icon::ChevronRight, tok::ICON_S, s.ink_faint),
                        ]
                        .spacing(tok::L)
                        .align_y(Alignment::Center),
                    )
                    .padding([tok::NONE, tok::L])
                    .height(40)
                    .width(Length::Fill)
                    .radius(tok::R_ROW)
                    .kind(s, crate::ui::control::Kind::Tonal)
                    .no_press_scale()
                    .on_press(Message::Page(p)),
                );
            }
            Element::from(c)
        });
        column![label_w("Search", text::HEADING, s.ink, theme::SEMIBOLD)]
            .spacing(tok::XL)
            .extend(page_rows)
            .extend(bar_page(st, cx))
            .extend(appearance_page(st, cx))
            .extend(time_page(st, cx))
            .extend(plugins_page(st, cx))
            // The system pages that have been read (searching reads the quick ones).
            .extend(power_page(st, cx))
            .extend(access_page(st, cx))
            .extend(keyboard_page(st, cx))
            .extend(mouse_page(st, cx))
            .extend(windows_page(st, cx))
            .extend(datetime_page(st, cx))
            .extend(notifications_page(st, cx))
    };
    QUERY.with(|cell| cell.borrow_mut().clear());
    // A comfortable reading width however wide the window (rows and sliders stretched over a
    // whole screen read badly).
    let body = iced::widget::scrollable(
        // The limit on the padded box itself: on the column alone the box still stretched to the
        // window and the content hugged the left of a wide window.
        container(
            container(body)
                .padding([tok::XXL, tok::XXL])
                .max_width(760.0 + 2.0 * tok::XXL),
        )
        .center_x(Length::Fill),
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
            background: Some(s.ink.scale_alpha(tok::TONAL * s.opacity).into()),
            ..container::Style::default()
        });
    row![side, divider, body].height(Length::Fill).into()
}

fn stepper<'a>(s: Scheme, n: i64) -> Element<'a, Message> {
    let arrow = |i: Icon, m: Message| {
        control(container(icon(i, tok::ICON, s.ink)).center(28))
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(m)
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
fn value_row<'a>(s: Scheme, name: &'a str, value: String) -> Option<Element<'a, Message>> {
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
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    secure: bool,
) -> Element<'a, Message> {
    let mut t = text_input(placeholder, value)
        .id(iced::widget::Id::new(id))
        .on_input(on_input)
        .secure(secure)
        .padding([tok::S, tok::L])
        .size(text::BODY)
        .font(theme::FONT)
        .width(240)
        .style(crate::ui::input_style(s));
    if let Some(m) = on_submit {
        t = t.on_submit(m);
    }
    container(t).style(move |_| ui::field_style(s)).into()
}

/// A value that copies itself when clicked (addresses, passwords).
fn copy_row<'a>(s: Scheme, name: &'a str, value: String) -> Option<Element<'a, Message>> {
    row_of(
        s,
        name,
        None,
        control(label(value.clone(), text::BODY, s.ink_dim))
            .padding([tok::XS, tok::L])
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .no_press_scale()
            .on_press(Message::Copy(value))
            .into(),
    )
}

/// A connection's addresses (Wi-Fi or Ethernet), each copied by a click.
fn detail_rows<'a>(
    s: Scheme,
    d: &crate::services::network::Details,
) -> Vec<Option<Element<'a, Message>>> {
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
) -> Element<'a, Message> {
    container(
        text_input("Search", value)
            .id(iced::widget::Id::new(id))
            .on_input(on)
            .padding([tok::S, tok::L])
            .size(text::BODY)
            .font(theme::FONT)
            .style(crate::ui::input_style(s)),
    )
    .style(move |_| ui::field_style(s))
    .into()
}

/// One entry of a pick list.
fn pick_row<'a>(s: Scheme, name: String, on: bool, m: Message) -> Element<'a, Message> {
    control(label(name, text::BODY, if on { s.accent } else { s.ink }))
        .padding([tok::NONE, tok::L])
        .height(34)
        .width(Length::Fill)
        .radius(tok::R_ROW)
        .kind(s, crate::ui::control::Kind::Ghost)
        .no_press_scale()
        .on_press(m)
        .into()
}

/// A row for one Hyprland input option: a switch, a choice, or a slider with its value.
fn hypr_row<'a>(
    st: &'a State,
    cx: &Cx<'a>,
    o: crate::hypr_settings::Opt,
) -> Option<Element<'a, Message>> {
    let s = cx.scheme;
    use crate::hypr_settings::{Kind, Value};
    let v = st.hypr.get(o.option)?;
    let control_: Element<'_, Message> = match o.kind {
        Kind::Switch => {
            let on = v.as_bool();
            ui::switch(s, on, Message::HyprSet(o, Value::Bool(!on), true))
        }
        Kind::Choice(choices) => {
            let now = v.as_str();
            let opts: Vec<(&str, bool, Message)> = choices
                .iter()
                .map(|(val, name)| {
                    (
                        *name,
                        *val == now,
                        Message::HyprSet(o, Value::Str((*val).to_string()), true),
                    )
                })
                .collect();
            segments(s, &opts)
        }
        Kind::Numbers(choices) => {
            let now = v.as_num().round() as i64;
            let opts: Vec<(&str, bool, Message)> = choices
                .iter()
                .map(|(val, name)| {
                    (
                        *name,
                        *val == now,
                        Message::HyprSet(o, Value::Num(*val as f32), true),
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
                (s.ink.scale_alpha(tok::FILL * s.opacity), s.accent),
                move |t| {
                    let raw = min + t * (max - min);
                    let v = ((raw / step).round() * step).clamp(min, max);
                    Message::HyprSet(o, Value::Num(v), false)
                },
            )
            .on_release(Message::HyprKeep(o));
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
    let mut texts = TEXTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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

/// A change to the system, asked of the shell.
fn chg(c: crate::app::Change) -> Message {
    Message::Change(c)
}
