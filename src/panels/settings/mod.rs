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
use iced::widget::{container, row, text_input};
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
mod load;
mod mouse;
mod network;
mod notifications;
mod own;
mod plugins;
mod power;
mod privacy;
mod shortcuts;
mod sound;
mod startup;
mod storage;
mod update;
mod updates;
mod view;
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
pub use load::*;
use mouse::*;
use network::*;
use notifications::*;
use own::*;
use plugins::*;
use power::*;
use privacy::*;
use shortcuts::*;
use sound::*;
use startup::*;
use storage::*;
pub use update::*;
use updates::*;
pub use view::*;
use weather::*;
use windows::*;

type Action = Act<Message>;

/// A card of the bar editor is being dragged (its release ends it, wherever it lands).
pub fn dragging(st: &State) -> bool {
    st.bar_drag.is_some()
}

/// Settings' window content: Tab moves only inside it.
pub static ROOT: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("kestrel-settings-root"));

const SIDEBAR: f32 = 196.0;
/// The pages' reading width, however wide the window (rows stretched over a screen read badly).
const READING_W: f32 = 760.0;

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
    /// Its name for `kestrel settings <page>`: lower case, words joined by "-" ("sound",
    /// "mouse-touchpad", "date-time").
    pub fn slug(self) -> String {
        self.name()
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }

    /// The page `kestrel settings <name>` names: its slug, or its slug's first word.
    pub fn from_slug(name: &str) -> Option<Page> {
        let name = name.to_lowercase();
        Page::all()
            .find(|p| p.slug() == name)
            .or_else(|| Page::all().find(|p| p.slug().split('-').next() == Some(name.as_str())))
    }

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
    /// The bar editor: the module whose options show, the one being dragged and where it
    /// would land (lane, place), and the lane whose "+" list is open.
    bar_pick: Option<String>,
    bar_drag: Option<String>,
    bar_target: Option<(usize, usize)>,
    bar_adding: Option<usize>,
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
    BarReset,
    /// The bar editor: a module's options shown (or hidden), a card picked up by its grip, the
    /// pointer over a place it would land, let go (or Esc), a lane's "+" list, adding there.
    BarPick(Option<String>),
    BarDrag(String),
    BarOver(usize, usize),
    BarDrop,
    BarCancel,
    BarAddOpen(Option<usize>),
    BarAddTo(usize, String),
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

/// A change to the system, asked of the shell.
fn chg(c: crate::app::Change) -> Message {
    Message::Change(c)
}
