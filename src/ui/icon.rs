//! The icon set: Material Symbols Rounded, filled, weight 500, drawn as glyphs from an
//! embedded font (assets/fonts/kestrel-symbols.ttf, made from Google's variable font and cut
//! down to these icons; Apache-2.0, assets/fonts/LICENSE-material-symbols). Glyphs are
//! rendered like text, so they are crisp and evenly sized at bar sizes (the SVGs were thin and
//! grainy). Kept under kestrel's own names (wifi, battery-frame-3, …) for code and Lua.

use iced::Font;

/// The embedded icon font.
pub const BYTES: &[u8] = include_bytes!("../../assets/fonts/kestrel-symbols.ttf");
pub const FONT: Font = Font::with_name("kestrel Symbols");

macro_rules! icons {
    ($($variant:ident => $name:literal, $glyph:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($variant,)* }
        const NAMES: &[(&str, Icon, char)] = &[$(($name, Icon::$variant, $glyph),)*];
    };
}

icons! {
    Activity => "activity", '\u{f190}',
    Air => "air", '\u{efd8}',
    AppWindow => "app-window", '\u{e069}',
    ArrowDownToLine => "arrow-down-to-line", '\u{f090}',
    ArrowLeft => "arrow-left", '\u{e5c4}',
    AudioLines => "audio-lines", '\u{e1b8}',
    Battery => "battery", '\u{f0a0}',
    BatteryCharging => "battery-charging", '\u{e1a3}',
    BatteryFrame1 => "battery-frame-1", '\u{f257}',
    BatteryFrame2 => "battery-frame-2", '\u{f256}',
    BatteryFrame3 => "battery-frame-3", '\u{f255}',
    BatteryFrame4 => "battery-frame-4", '\u{f254}',
    BatteryFrame5 => "battery-frame-5", '\u{f253}',
    BatteryFrame6 => "battery-frame-6", '\u{f252}',
    BatteryFrameAlert => "battery-frame-alert", '\u{f251}',
    BatteryFrameBolt => "battery-frame-bolt", '\u{f250}',
    BatteryFrameFull => "battery-frame-full", '\u{f24f}',
    BatteryFull => "battery-full", '\u{e1a5}',
    BatteryLow => "battery-low", '\u{f09d}',
    BatteryMedium => "battery-medium", '\u{f09f}',
    BatteryWarning => "battery-warning", '\u{e19c}',
    Bell => "bell", '\u{e7f5}',
    BellDot => "bell-dot", '\u{f4fe}',
    BellOff => "bell-off", '\u{e7f6}',
    BellRing => "bell-ring", '\u{e7f7}',
    Bitcoin => "bitcoin", '\u{ebc5}',
    Bluetooth => "bluetooth", '\u{e1a7}',
    BluetoothConnected => "bluetooth-connected", '\u{e1a8}',
    BluetoothOff => "bluetooth-off", '\u{e1a9}',
    Cable => "cable", '\u{efe6}',
    Calculator => "calculator", '\u{ea5f}',
    CalendarClock => "calendar-clock", '\u{f540}',
    Camera => "camera", '\u{e412}',
    Check => "check", '\u{e668}',
    ChevronDown => "chevron-down", '\u{e313}',
    ChevronLeft => "chevron-left", '\u{e5cb}',
    ChevronRight => "chevron-right", '\u{e5cc}',
    ChevronUp => "chevron-up", '\u{e316}',
    CircleDot => "circle-dot", '\u{e837}',
    Clipboard => "clipboard", '\u{e14f}',
    Cloud => "cloud", '\u{f15c}',
    CloudDrizzle => "cloud-drizzle", '\u{f61e}',
    CloudFog => "cloud-fog", '\u{e818}',
    CloudLightning => "cloud-lightning", '\u{ebdb}',
    CloudMoon => "cloud-moon", '\u{f174}',
    CloudRain => "cloud-rain", '\u{f176}',
    CloudSnow => "cloud-snow", '\u{e2cd}',
    CloudSun => "cloud-sun", '\u{f172}',
    Code => "code", '\u{e86f}',
    Coffee => "coffee", '\u{efef}',
    Container => "container", '\u{f720}',
    Cpu => "cpu", '\u{e322}',
    Download => "download", '\u{f090}',
    Droplet => "droplet", '\u{e798}',
    EarbudCase => "earbud-case", '\u{f327}',
    EarbudLeft => "earbud-left", '\u{f326}',
    EarbudRight => "earbud-right", '\u{f325}',
    Earbuds => "earbuds", '\u{f324}',
    EarbudsBattery => "earbuds-battery", '\u{f004}',
    Ellipsis => "ellipsis", '\u{e5d3}',
    File => "file", '\u{e873}',
    Flame => "flame", '\u{ef55}',
    Folder => "folder", '\u{e2c7}',
    Gamepad2 => "gamepad-2", '\u{ea28}',
    Gauge => "gauge", '\u{e9e4}',
    GitBranch => "git-branch", '\u{e97a}',
    Globe => "globe", '\u{ea07}',
    HardDrive => "hard-drive", '\u{f80e}',
    Headphones => "headphones", '\u{f01f}',
    House => "house", '\u{e9b2}',
    Humidity => "humidity", '\u{f87e}',
    Image => "image", '\u{e3f4}',
    Keyboard => "keyboard", '\u{e312}',
    Laptop => "laptop", '\u{e320}',
    LayoutGrid => "layout-grid", '\u{e9b0}',
    Leaf => "leaf", '\u{ea35}',
    Lock => "lock", '\u{e899}',
    LogOut => "log-out", '\u{e9ba}',
    Mail => "mail", '\u{e159}',
    MemoryStick => "memory-stick", '\u{f7a3}',
    Mic => "mic", '\u{e31d}',
    MicOff => "mic-off", '\u{e02b}',
    Monitor => "monitor", '\u{e30c}',
    Moon => "moon", '\u{f036}',
    MoonStar => "moon-star", '\u{f159}',
    Mouse => "mouse", '\u{e323}',
    Music => "music", '\u{e405}',
    Network => "network", '\u{eb2f}',
    Package => "package", '\u{f569}',
    Pause => "pause", '\u{e034}',
    Pin => "pin", '\u{f027}',
    PinOff => "pin-off", '\u{e6f9}',
    Play => "play", '\u{e037}',
    PlugZap => "plug-zap", '\u{e63c}',
    Power => "power", '\u{f8c7}',
    Printer => "printer", '\u{e8ad}',
    Repeat => "repeat", '\u{e040}',
    Rocket => "rocket", '\u{eb9b}',
    RotateCcw => "rotate-ccw", '\u{f053}',
    Rss => "rss", '\u{e0e5}',
    ScreenShare => "screen-share", '\u{e0e2}',
    Search => "search", '\u{ef7a}',
    Server => "server", '\u{e875}',
    Settings => "settings", '\u{e8b8}',
    Shield => "shield", '\u{e9e0}',
    Shuffle => "shuffle", '\u{e043}',
    SkipBack => "skip-back", '\u{e045}',
    SkipForward => "skip-forward", '\u{e044}',
    Smartphone => "smartphone", '\u{e7ba}',
    Smile => "smile", '\u{ea22}',
    Snowflake => "snowflake", '\u{eb3b}',
    Sparkles => "sparkles", '\u{f31d}',
    Speaker => "speaker", '\u{e32d}',
    Sun => "sun", '\u{e81a}',
    SunDim => "sun-dim", '\u{e3aa}',
    SunMedium => "sun-medium", '\u{e1ae}',
    Sunset => "sunset", '\u{e1c6}',
    Terminal => "terminal", '\u{eb8e}',
    Thermometer => "thermometer", '\u{f076}',
    Timer => "timer", '\u{e425}',
    Trash2 => "trash-2", '\u{e92e}',
    Trophy => "trophy", '\u{ea23}',
    Video => "video", '\u{e04b}',
    Volume => "volume", '\u{e04e}',
    Volume1 => "volume-1", '\u{e04d}',
    Volume2 => "volume-2", '\u{e050}',
    VolumeX => "volume-x", '\u{e04f}',
    Watch => "watch", '\u{e334}',
    Wifi => "wifi", '\u{e1ba}',
    Wifi1 => "wifi-1", '\u{ebe4}',
    Wifi2 => "wifi-2", '\u{ebd6}',
    Wifi3 => "wifi-3", '\u{ebe1}',
    WifiHigh => "wifi-high", '\u{ebe1}',
    WifiLow => "wifi-low", '\u{ebe4}',
    WifiOff => "wifi-off", '\u{e648}',
    WifiZero => "wifi-zero", '\u{f0b0}',
    X => "x", '\u{e5cd}',
    Zap => "zap", '\u{ea0b}',
}

impl Icon {
    /// The icon with this name ("hard-drive"), for the Lua layer.
    pub fn named(name: &str) -> Option<Icon> {
        NAMES
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, i, _)| *i)
    }

    /// Its glyph in `FONT`.
    pub fn glyph(self) -> char {
        NAMES
            .iter()
            .find(|(_, i, _)| *i == self)
            .map_or(' ', |(_, _, g)| *g)
    }
}
