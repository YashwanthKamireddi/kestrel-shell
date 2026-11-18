//! The catalogue: every Hyprland option Settings offers (keyboard, mouse, touchpad, windows,
//! accessibility), how each is chosen, and the XKB option choices.

macro_rules! opt {
    ($label:literal, $option:literal, [$($p:literal),+], $kind:expr) => {
        Opt { label: $label, option: $option, path: &[$($p),+], kind: $kind }
    };
}

/// How a setting is chosen.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Switch,
    /// A number between `min` and `max`, shown with `unit`.
    Range {
        min: f32,
        max: f32,
        step: f32,
        unit: &'static str,
    },
    /// One of these values (value, label).
    Choice(&'static [(&'static str, &'static str)]),
    /// One of these whole numbers (value, label), for options Hyprland keeps as integers.
    Numbers(&'static [(i64, &'static str)]),
}

/// One Hyprland option shown in Settings.
#[derive(Clone, Copy, Debug)]
pub struct Opt {
    pub label: &'static str,
    /// Its name for `hyprctl getoption` ("input:touchpad:tap-to-click").
    pub option: &'static str,
    /// Its place in `hl.config` (tables, then the key: "input", "touchpad", "tap_to_click").
    pub path: &'static [&'static str],
    pub kind: Kind,
}

impl Opt {
    pub(super) fn tables(&self) -> &'static [&'static str] {
        &self.path[..self.path.len() - 1]
    }
    pub(super) fn key(&self) -> &'static str {
        self.path[self.path.len() - 1]
    }
}

pub const KEYBOARD: &[Opt] = &[
    opt!(
        "Repeat delay",
        "input:repeat_delay",
        ["input", "repeat_delay"],
        Kind::Range {
            min: 150.0,
            max: 1000.0,
            step: 10.0,
            unit: "ms"
        }
    ),
    opt!(
        "Repeat rate",
        "input:repeat_rate",
        ["input", "repeat_rate"],
        Kind::Range {
            min: 10.0,
            max: 80.0,
            step: 1.0,
            unit: "/s"
        }
    ),
    opt!(
        "Num Lock on at start",
        "input:numlock_by_default",
        ["input", "numlock_by_default"],
        Kind::Switch
    ),
];

/// The keyboard's xkb options: one comma-separated list where each choice below owns its
/// own part (so choosing a compose key keeps the Caps Lock choice).
pub const KB_OPTIONS: Opt = Opt {
    label: "Keyboard options",
    option: "input:kb_options",
    path: &["input", "kb_options"],
    kind: Kind::Choice(&[]),
};

/// One xkb choice: its name, the option prefixes it owns, and its values (option, label).
pub struct XkbChoice {
    pub label: &'static str,
    pub owns: &'static [&'static str],
    pub values: &'static [(&'static str, &'static str)],
    /// Shown only with more than one layout (switching between them).
    pub layouts: bool,
}

pub const XKB: &[XkbChoice] = &[
    XkbChoice {
        label: "Caps Lock key",
        owns: &["ctrl:nocaps", "caps:"],
        values: &[
            ("", "Caps Lock"),
            ("ctrl:nocaps", "Ctrl"),
            ("caps:escape", "Escape"),
        ],
        layouts: false,
    },
    XkbChoice {
        label: "Switch layouts with",
        owns: &["grp:"],
        // Super+Space is Hyprland's own binding (keybinds.lua: switchxkblayout), so no xkb
        // option for it: both on the same keys would switch twice, changing nothing.
        values: &[
            ("", "Super+Space"),
            ("grp:alt_shift_toggle", "Alt+Shift"),
            ("grp:ctrl_shift_toggle", "Ctrl+Shift"),
        ],
        layouts: true,
    },
    XkbChoice {
        label: "Compose key",
        owns: &["compose:"],
        values: &[
            ("", "None"),
            ("compose:ralt", "Right Alt"),
            ("compose:menu", "Menu"),
        ],
        layouts: false,
    },
];

/// The part of `options` (comma-separated) that `c` owns, or "".
pub fn xkb_value(options: &str, c: &XkbChoice) -> String {
    options
        .split(',')
        .map(str::trim)
        .find(|o| c.owns.iter().any(|p| o.starts_with(p)))
        .unwrap_or("")
        .to_string()
}

/// `options` with `c`'s part set to `value` ("" removes it), the rest kept in order.
pub fn xkb_with(options: &str, c: &XkbChoice, value: &str) -> String {
    let mut parts: Vec<&str> = options
        .split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty() && !c.owns.iter().any(|p| o.starts_with(p)))
        .collect();
    if !value.is_empty() {
        parts.push(value);
    }
    parts.join(",")
}

pub const MOUSE: &[Opt] = &[
    opt!(
        "Pointer speed",
        "input:sensitivity",
        ["input", "sensitivity"],
        Kind::Range {
            min: -1.0,
            max: 1.0,
            step: 0.05,
            unit: ""
        }
    ),
    opt!(
        "Acceleration",
        "input:accel_profile",
        ["input", "accel_profile"],
        Kind::Choice(&[("", "Adaptive"), ("flat", "Flat")])
    ),
    opt!(
        "Natural scrolling",
        "input:natural_scroll",
        ["input", "natural_scroll"],
        Kind::Switch
    ),
    opt!(
        "Left-handed",
        "input:left_handed",
        ["input", "left_handed"],
        Kind::Switch
    ),
    opt!(
        "Middle-click paste",
        "misc:middle_click_paste",
        ["misc", "middle_click_paste"],
        Kind::Switch
    ),
];

pub const TOUCHPAD: &[Opt] = &[
    opt!(
        "Tap to click",
        "input:touchpad:tap-to-click",
        ["input", "touchpad", "tap_to_click"],
        Kind::Switch
    ),
    opt!(
        "Tap and drag",
        "input:touchpad:tap-and-drag",
        ["input", "touchpad", "tap_and_drag"],
        Kind::Switch
    ),
    opt!(
        "Natural scrolling",
        "input:touchpad:natural_scroll",
        ["input", "touchpad", "natural_scroll"],
        Kind::Switch
    ),
    opt!(
        "Disable while typing",
        "input:touchpad:disable_while_typing",
        ["input", "touchpad", "disable_while_typing"],
        Kind::Switch
    ),
    opt!(
        "Two-finger right click",
        "input:touchpad:clickfinger_behavior",
        ["input", "touchpad", "clickfinger_behavior"],
        Kind::Switch
    ),
    opt!(
        "Swipe distance between workspaces",
        "gestures:workspace_swipe_distance",
        ["gestures", "workspace_swipe_distance"],
        Kind::Range {
            min: 100.0,
            max: 800.0,
            step: 25.0,
            unit: " px"
        }
    ),
    opt!(
        "Scroll speed",
        "input:touchpad:scroll_factor",
        ["input", "touchpad", "scroll_factor"],
        Kind::Range {
            min: 0.2,
            max: 2.0,
            step: 0.05,
            unit: "×"
        }
    ),
];

/// How windows look (his look.lua sets them; a choice here wins, as with input).
pub const WINDOW_LOOK: &[Opt] = &[
    opt!(
        "Space between windows",
        "general:gaps_in",
        ["general", "gaps_in"],
        Kind::Range {
            min: 0.0,
            max: 20.0,
            step: 1.0,
            unit: " px"
        }
    ),
    opt!(
        "Space around windows",
        "general:gaps_out",
        ["general", "gaps_out"],
        Kind::Range {
            min: 0.0,
            max: 32.0,
            step: 1.0,
            unit: " px"
        }
    ),
    opt!(
        "Corner rounding",
        "decoration:rounding",
        ["decoration", "rounding"],
        Kind::Range {
            min: 0.0,
            max: 24.0,
            step: 1.0,
            unit: " px"
        }
    ),
    opt!(
        "Border",
        "general:border_size",
        ["general", "border_size"],
        Kind::Range {
            min: 0.0,
            max: 4.0,
            step: 1.0,
            unit: " px"
        }
    ),
    opt!(
        "Blur behind windows",
        "decoration:blur:enabled",
        ["decoration", "blur", "enabled"],
        Kind::Switch
    ),
    opt!(
        "Shadows",
        "decoration:shadow:enabled",
        ["decoration", "shadow", "enabled"],
        Kind::Switch
    ),
    opt!(
        "Dim windows not in focus",
        "decoration:dim_inactive",
        ["decoration", "dim_inactive"],
        Kind::Switch
    ),
];

/// How windows behave.
pub const WINDOW_BEHAVIOUR: &[Opt] = &[
    opt!(
        "Focus",
        "input:follow_mouse",
        ["input", "follow_mouse"],
        Kind::Numbers(&[(1, "Follows the pointer"), (0, "On click")])
    ),
    opt!(
        "Resize from window edges",
        "general:resize_on_border",
        ["general", "resize_on_border"],
        Kind::Switch
    ),
    opt!(
        "Snap floating windows",
        "general:snap:enabled",
        ["general", "snap", "enabled"],
        Kind::Switch
    ),
    opt!(
        "Swipe to switch workspaces, inverted",
        "gestures:workspace_swipe_invert",
        ["gestures", "workspace_swipe_invert"],
        Kind::Switch
    ),
];

pub const ACCESS: &[Opt] = &[
    opt!(
        "Window animations",
        "animations:enabled",
        ["animations", "enabled"],
        Kind::Switch
    ),
    opt!(
        "Zoom",
        "cursor:zoom_factor",
        ["cursor", "zoom_factor"],
        Kind::Range {
            min: 1.0,
            max: 4.0,
            step: 0.25,
            unit: "×"
        }
    ),
    // Screen shaders kestrel installs (assets/shaders): greyscale, inverted, and corrections
    // for the three kinds of colour blindness.
    opt!(
        "Colour filter",
        "decoration:screen_shader",
        ["decoration", "screen_shader"],
        Kind::Choice(&[
            ("", "Off"),
            ("/usr/share/kestrel/shaders/greyscale.frag", "Greyscale"),
            ("/usr/share/kestrel/shaders/inverted.frag", "Inverted"),
            (
                "/usr/share/kestrel/shaders/deuteranopia.frag",
                "Deuteranopia"
            ),
            ("/usr/share/kestrel/shaders/protanopia.frag", "Protanopia"),
            ("/usr/share/kestrel/shaders/tritanopia.frag", "Tritanopia"),
        ])
    ),
];
