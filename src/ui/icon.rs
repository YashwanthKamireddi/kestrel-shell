//! The icon set: Material Symbols Rounded, filled (Apache-2.0, assets/icons/
//! LICENSE-material-symbols), the icon language of Material 3. Kept under kestrel's own names
//! (wifi, battery-3, …); level variants let status icons show the real level.
//! Each icon is embedded once and its handle reused, so the renderer caches the raster.

use iced::widget::svg::Handle;
use std::sync::OnceLock;

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($variant,)* }
        const ALL: &[(Icon, &[u8])] = &[$((Icon::$variant, include_bytes!(concat!("../../assets/icons/", $file, ".svg"))),)*];
        const NAMES: &[(&str, Icon)] = &[$(($file, Icon::$variant),)*];
    };
}

icons! {
    Activity => "activity",
    AppWindow => "app-window",
    ArrowDownToLine => "arrow-down-to-line",
    ArrowLeft => "arrow-left",
    AudioLines => "audio-lines",
    Battery => "battery",
    Battery0 => "battery-0",
    Battery1 => "battery-1",
    Battery2 => "battery-2",
    Battery3 => "battery-3",
    Battery4 => "battery-4",
    Battery5 => "battery-5",
    Battery6 => "battery-6",
    BatteryCharging => "battery-charging",
    BatteryCharging20 => "battery-charging-20",
    BatteryCharging30 => "battery-charging-30",
    BatteryCharging50 => "battery-charging-50",
    BatteryCharging60 => "battery-charging-60",
    BatteryCharging80 => "battery-charging-80",
    BatteryCharging90 => "battery-charging-90",
    BatteryFull => "battery-full",
    BatteryLow => "battery-low",
    BatteryMedium => "battery-medium",
    BatteryWarning => "battery-warning",
    Bell => "bell",
    BellDot => "bell-dot",
    BellOff => "bell-off",
    BellRing => "bell-ring",
    Bitcoin => "bitcoin",
    Bluetooth => "bluetooth",
    BluetoothConnected => "bluetooth-connected",
    BluetoothOff => "bluetooth-off",
    Cable => "cable",
    Calculator => "calculator",
    CalendarClock => "calendar-clock",
    Camera => "camera",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    ChevronUp => "chevron-up",
    CircleDot => "circle-dot",
    Clipboard => "clipboard",
    Cloud => "cloud",
    CloudDrizzle => "cloud-drizzle",
    CloudFog => "cloud-fog",
    CloudLightning => "cloud-lightning",
    CloudRain => "cloud-rain",
    CloudSnow => "cloud-snow",
    CloudSun => "cloud-sun",
    Code => "code",
    Coffee => "coffee",
    Container => "container",
    Cpu => "cpu",
    Download => "download",
    Ellipsis => "ellipsis",
    File => "file",
    Flame => "flame",
    Folder => "folder",
    Gamepad2 => "gamepad-2",
    Gauge => "gauge",
    GitBranch => "git-branch",
    Globe => "globe",
    HardDrive => "hard-drive",
    Headphones => "headphones",
    House => "house",
    Image => "image",
    Keyboard => "keyboard",
    Laptop => "laptop",
    LayoutGrid => "layout-grid",
    Leaf => "leaf",
    Lock => "lock",
    LogOut => "log-out",
    Mail => "mail",
    MemoryStick => "memory-stick",
    Mic => "mic",
    MicOff => "mic-off",
    Monitor => "monitor",
    Moon => "moon",
    MoonStar => "moon-star",
    Mouse => "mouse",
    Music => "music",
    Network => "network",
    Package => "package",
    Pause => "pause",
    Pin => "pin",
    PinOff => "pin-off",
    Play => "play",
    PlugZap => "plug-zap",
    Power => "power",
    Printer => "printer",
    Repeat => "repeat",
    Rocket => "rocket",
    RotateCcw => "rotate-ccw",
    Rss => "rss",
    ScreenShare => "screen-share",
    Search => "search",
    Server => "server",
    Settings => "settings",
    Shield => "shield",
    Shuffle => "shuffle",
    SkipBack => "skip-back",
    SkipForward => "skip-forward",
    Smartphone => "smartphone",
    Smile => "smile",
    Snowflake => "snowflake",
    Sparkles => "sparkles",
    Speaker => "speaker",
    Sun => "sun",
    SunDim => "sun-dim",
    SunMedium => "sun-medium",
    Sunset => "sunset",
    Terminal => "terminal",
    Thermometer => "thermometer",
    Timer => "timer",
    Trash2 => "trash-2",
    Trophy => "trophy",
    Video => "video",
    Volume => "volume",
    Volume1 => "volume-1",
    Volume2 => "volume-2",
    VolumeX => "volume-x",
    Watch => "watch",
    Wifi => "wifi",
    Wifi1 => "wifi-1",
    Wifi2 => "wifi-2",
    Wifi3 => "wifi-3",
    WifiHigh => "wifi-high",
    WifiLow => "wifi-low",
    WifiOff => "wifi-off",
    WifiZero => "wifi-zero",
    X => "x",
    Zap => "zap",
}
impl Icon {
    /// The icon with this name ("hard-drive"), for the Lua layer.
    pub fn named(name: &str) -> Option<Icon> {
        NAMES.iter().find(|(n, _)| *n == name).map(|(_, i)| *i)
    }

    pub fn handle(self) -> Handle {
        static HANDLES: OnceLock<Vec<(Icon, Handle)>> = OnceLock::new();
        let all = HANDLES.get_or_init(|| {
            ALL.iter()
                .map(|(i, bytes)| (*i, Handle::from_memory(*bytes)))
                .collect()
        });
        all.iter()
            .find(|(i, _)| *i == self)
            .map(|(_, h)| h.clone())
            .expect("every icon is embedded")
    }
}
