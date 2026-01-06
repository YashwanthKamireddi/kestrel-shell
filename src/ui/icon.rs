//! The icon set: Lucide (ISC licence, assets/icons/LICENSE-lucide), 1.75 px stroke.
//! Each icon is embedded once and its handle reused, so the renderer caches the raster.

use iced::widget::svg::Handle;
use std::sync::OnceLock;

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($variant,)* }
        const ALL: &[(Icon, &[u8])] = &[$((Icon::$variant, include_bytes!(concat!("../../assets/icons/", $file, ".svg"))),)*];
    };
}

icons! {
    AppWindow => "app-window",
    ArrowLeft => "arrow-left",
    BatteryCharging => "battery-charging",
    BatteryFull => "battery-full",
    BatteryLow => "battery-low",
    BatteryMedium => "battery-medium",
    Battery => "battery",
    BatteryWarning => "battery-warning",
    BellDot => "bell-dot",
    BellOff => "bell-off",
    BellRing => "bell-ring",
    Bell => "bell",
    BluetoothConnected => "bluetooth-connected",
    BluetoothOff => "bluetooth-off",
    Bluetooth => "bluetooth",
    Cable => "cable",
    Calculator => "calculator",
    Camera => "camera",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    ChevronUp => "chevron-up",
    CircleDot => "circle-dot",
    Clipboard => "clipboard",
    Coffee => "coffee",
    Ellipsis => "ellipsis",
    Gamepad2 => "gamepad-2",
    Gauge => "gauge",
    Headphones => "headphones",
    Image => "image",
    Keyboard => "keyboard",
    Laptop => "laptop",
    LayoutGrid => "layout-grid",
    Leaf => "leaf",
    Lock => "lock",
    LogOut => "log-out",
    MicOff => "mic-off",
    Mic => "mic",
    Monitor => "monitor",
    MoonStar => "moon-star",
    Moon => "moon",
    Mouse => "mouse",
    Music => "music",
    Pause => "pause",
    Play => "play",
    PlugZap => "plug-zap",
    Power => "power",
    Printer => "printer",
    Repeat => "repeat",
    Rocket => "rocket",
    RotateCcw => "rotate-ccw",
    Search => "search",
    Settings => "settings",
    Shuffle => "shuffle",
    SkipBack => "skip-back",
    SkipForward => "skip-forward",
    Smartphone => "smartphone",
    Speaker => "speaker",
    SunDim => "sun-dim",
    Sunset => "sunset",
    Sun => "sun",
    Terminal => "terminal",
    Timer => "timer",
    Trash2 => "trash-2",
    Volume1 => "volume-1",
    Volume2 => "volume-2",
    Volume => "volume",
    VolumeX => "volume-x",
    Watch => "watch",
    WifiHigh => "wifi-high",
    WifiLow => "wifi-low",
    WifiOff => "wifi-off",
    Wifi => "wifi",
    WifiZero => "wifi-zero",
    X => "x",
    Zap => "zap",
}

impl Icon {
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
