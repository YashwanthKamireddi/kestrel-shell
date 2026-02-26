//! The icon set: Microsoft's Fluent UI System Icons, filled, 20 px designs, drawn as glyphs
//! from an embedded font (assets/fonts/kestrel-icons.ttf, cut down from Fluent's font to these
//! icons; MIT, assets/fonts/LICENSE-fluent-icons). Glyphs render like text, so they stay crisp
//! and evenly sized. Kept under kestrel's own names (wifi, battery-frame-3, …) for code and Lua.

use iced::Font;

/// The embedded icon font.
pub const BYTES: &[u8] = include_bytes!("../../assets/fonts/kestrel-icons.ttf");
pub const FONT: Font = Font::with_name("kestrel Icons");

macro_rules! icons {
    ($($variant:ident => $name:literal, $glyph:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($variant,)* }
        const NAMES: &[(&str, Icon, char)] = &[$(($name, Icon::$variant, $glyph),)*];
    };
}

icons! {
    Activity => "activity", '\u{e9e4}',
    Air => "air", '\u{f8b6}',
    AppWindow => "app-window", '\u{f8cd}',
    ArrowDownToLine => "arrow-down-to-line", '\u{f150}',
    ArrowLeft => "arrow-left", '\u{f15b}',
    AudioLines => "audio-lines", '\u{eb38}',
    Battery => "battery", '\u{e000}',
    BatteryCharging => "battery-charging", '\u{e001}',
    BatteryFrame1 => "battery-frame-1", '\u{e002}',
    BatteryFrame2 => "battery-frame-2", '\u{e003}',
    BatteryFrame3 => "battery-frame-3", '\u{e004}',
    BatteryFrame4 => "battery-frame-4", '\u{e005}',
    BatteryFrame5 => "battery-frame-5", '\u{e006}',
    BatteryFrame6 => "battery-frame-6", '\u{e007}',
    BatteryFrameAlert => "battery-frame-alert", '\u{e008}',
    BatteryFrameBolt => "battery-frame-bolt", '\u{e001}',
    BatteryFrameFull => "battery-frame-full", '\u{e000}',
    BatteryFull => "battery-full", '\u{e000}',
    BatteryLow => "battery-low", '\u{e009}',
    BatteryMedium => "battery-medium", '\u{e00a}',
    BatteryWarning => "battery-warning", '\u{e008}',
    Bell => "bell", '\u{f114}',
    BellDot => "bell-dot", '\u{e018}',
    BellOff => "bell-off", '\u{f118}',
    BellRing => "bell-ring", '\u{e01a}',
    Bitcoin => "bitcoin", '\u{f559}',
    Bluetooth => "bluetooth", '\u{f1de}',
    BluetoothConnected => "bluetooth-connected", '\u{e15f}',
    BluetoothOff => "bluetooth-off", '\u{e160}',
    Cable => "cable", '\u{e9ab}',
    Calculator => "calculator", '\u{f20a}',
    CalendarClock => "calendar-clock", '\u{f21d}',
    Camera => "camera", '\u{f254}',
    Check => "check", '\u{f294}',
    ChevronDown => "chevron-down", '\u{f2a3}',
    ChevronLeft => "chevron-left", '\u{f2aa}',
    ChevronRight => "chevron-right", '\u{f2b0}',
    ChevronUp => "chevron-up", '\u{f2b6}',
    CircleDot => "circle-dot", '\u{f66b}',
    Clipboard => "clipboard", '\u{f2c9}',
    Cloud => "cloud", '\u{f886}',
    CloudDrizzle => "cloud-drizzle", '\u{ee3e}',
    CloudFog => "cloud-fog", '\u{f88c}',
    CloudLightning => "cloud-lightning", '\u{f8bc}',
    CloudMoon => "cloud-moon", '\u{f89b}',
    CloudRain => "cloud-rain", '\u{f89e}',
    CloudSnow => "cloud-snow", '\u{f8aa}',
    CloudSun => "cloud-sun", '\u{f898}',
    Code => "code", '\u{f2ef}',
    Coffee => "coffee", '\u{f3c4}',
    Container => "container", '\u{f335}',
    Cpu => "cpu", '\u{e4b8}',
    Download => "download", '\u{f150}',
    Droplet => "droplet", '\u{e59c}',
    EarbudCase => "earbud-case", '\u{e1cf}',
    EarbudLeft => "earbud-left", '\u{f750}',
    EarbudRight => "earbud-right", '\u{f750}',
    Earbuds => "earbuds", '\u{f750}',
    EarbudsBattery => "earbuds-battery", '\u{f750}',
    Ellipsis => "ellipsis", '\u{e832}',
    File => "file", '\u{f378}',
    Flame => "flame", '\u{ef69}',
    Folder => "folder", '\u{f41c}',
    Gamepad2 => "gamepad-2", '\u{e697}',
    Gauge => "gauge", '\u{f4d1}',
    GitBranch => "git-branch", '\u{e1ed}',
    Globe => "globe", '\u{f45e}',
    HardDrive => "hard-drive", '\u{e6f0}',
    Headphones => "headphones", '\u{e6fb}',
    House => "house", '\u{f487}',
    Humidity => "humidity", '\u{f0ccd}',
    Image => "image", '\u{f48f}',
    Keyboard => "keyboard", '\u{f4c2}',
    Laptop => "laptop", '\u{f4cd}',
    LayoutGrid => "layout-grid", '\u{f466}',
    Leaf => "leaf", '\u{f4db}',
    Lock => "lock", '\u{e79d}',
    LogOut => "log-out", '\u{eb03}',
    Mail => "mail", '\u{f510}',
    MemoryStick => "memory-stick", '\u{e9fb}',
    Mic => "mic", '\u{e807}',
    MicOff => "mic-off", '\u{e80c}',
    Monitor => "monitor", '\u{f359}',
    Moon => "moon", '\u{f895}',
    MoonStar => "moon-star", '\u{e45e}',
    Mouse => "mouse", '\u{e44f}',
    Music => "music", '\u{e862}',
    Network => "network", '\u{e872}',
    Package => "package", '\u{e1cf}',
    Pause => "pause", '\u{f5ab}',
    Pin => "pin", '\u{f60b}',
    PinOff => "pin-off", '\u{f60d}',
    Play => "play", '\u{f60f}',
    PlugZap => "plug-zap", '\u{e9ab}',
    Power => "power", '\u{f618}',
    Printer => "printer", '\u{f634}',
    Repeat => "repeat", '\u{f171}',
    Rocket => "rocket", '\u{f681}',
    RotateCcw => "rotate-ccw", '\u{f13f}',
    Rss => "rss", '\u{ea45}',
    ScreenShare => "screen-share", '\u{eac3}',
    Search => "search", '\u{f699}',
    Server => "server", '\u{f781}',
    Settings => "settings", '\u{f6b2}',
    Shield => "shield", '\u{f6c7}',
    Shuffle => "shuffle", '\u{ef36}',
    SkipBack => "skip-back", '\u{f632}',
    SkipForward => "skip-forward", '\u{f573}',
    Smartphone => "smartphone", '\u{f5ea}',
    Smile => "smile", '\u{f3df}',
    Snowflake => "snowflake", '\u{f8b3}',
    Sparkles => "sparkles", '\u{eb3c}',
    Speaker => "speaker", '\u{f0559}',
    Sun => "sun", '\u{f8b9}',
    SunDim => "sun-dim", '\u{e211}',
    SunMedium => "sun-medium", '\u{e20b}',
    Sunset => "sunset", '\u{ee52}',
    Terminal => "terminal", '\u{ee6f}',
    Thermometer => "thermometer", '\u{f7a7}',
    Timer => "timer", '\u{ed88}',
    Trash2 => "trash-2", '\u{f34c}',
    Trophy => "trophy", '\u{f84e}',
    Video => "video", '\u{f864}',
    Volume => "volume", '\u{eb41}',
    Volume1 => "volume-1", '\u{eb46}',
    Volume2 => "volume-2", '\u{eb4b}',
    VolumeX => "volume-x", '\u{eb53}',
    Watch => "watch", '\u{f78d}',
    Wifi => "wifi", '\u{f8c4}',
    Wifi1 => "wifi-1", '\u{f8c8}',
    Wifi2 => "wifi-2", '\u{f8c6}',
    Wifi3 => "wifi-3", '\u{f8c6}',
    WifiHigh => "wifi-high", '\u{f8c6}',
    WifiLow => "wifi-low", '\u{f8c8}',
    WifiOff => "wifi-off", '\u{ee59}',
    WifiZero => "wifi-zero", '\u{ee5c}',
    X => "x", '\u{f369}',
    Zap => "zap", '\u{e625}',
}

impl Icon {
    /// The icon with this name ("hard-drive"), for the Lua layer.
    pub fn named(name: &str) -> Option<Icon> {
        NAMES
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, i, _)| *i)
    }

    /// What a partial level is drawn over, dimmed: the full Wi-Fi fan behind its bars (Fluent's
    /// weaker levels are only the lower arcs, which alone read as a smaller icon).
    pub fn backdrop(self) -> Option<Icon> {
        match self {
            Icon::Wifi1 | Icon::Wifi2 | Icon::Wifi3 | Icon::WifiHigh | Icon::WifiLow => {
                Some(Icon::Wifi)
            }
            _ => None,
        }
    }

    /// Its glyph in `FONT`.
    pub fn glyph(self) -> char {
        NAMES
            .iter()
            .find(|(_, i, _)| *i == self)
            .map_or(' ', |(_, _, g)| *g)
    }
}
