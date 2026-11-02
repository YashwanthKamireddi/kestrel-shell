//! `~/.config/kestrel/config.toml`. Every field is optional; unknown keys are ignored so the
//! file can grow without breaking older builds.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub wallpaper: Option<String>,
    /// Folder the wallpaper picker shows (default: the current wallpaper's folder).
    pub wallpaper_dir: Option<String>,
    pub frame: Frame,
    /// Workspaces always shown, even when empty.
    pub workspaces: u32,
    /// Theme GTK, Qt, kitty, Hyprland borders and hyprlock with the shell's colours.
    pub theme_apps: bool,
    /// What the bar's pills and the panels are made of: "black" (solid) or "frosted" (a heavy
    /// blur of the wallpaper behind them, darkened so text stays easy to read).
    pub material: Material,
    /// A fixed accent colour ("#4c8df6"); none: the wallpaper's.
    pub accent: Option<String>,
    /// $EDITOR and $VISUAL for the session ("nvim"), set in settings.lua.
    pub editor: Option<String>,
    /// Apps' fonts, icon theme and cursor theme (`[look]`), as chosen in Settings.
    pub look: LookConfig,
    /// City for the weather (looked up with Open-Meteo). No city, no weather.
    pub weather_city: Option<String>,
    /// Exact place for the weather (a neighbourhood the city search can't find); the city
    /// name is then only the label.
    pub weather_latitude: Option<f64>,
    pub weather_longitude: Option<f64>,
    /// Night light's warmth in kelvin (lower is warmer).
    pub night_light_temperature: u32,
    /// Night light on by itself from sunset to sunrise (the weather's sun times).
    pub night_light_schedule: bool,
    /// The pointer's size everywhere (Settings > Accessibility); unset: the session's own.
    pub cursor_size: Option<u32>,
    /// Do not disturb by itself between these times ("22:00" to "07:00"); off when unset.
    pub quiet_from: Option<String>,
    pub quiet_to: Option<String>,
    /// What the bar shows, where, and in which style.
    pub bar: BarConfig,
    /// How the time reads: `[clock] format = "24h" | "12h"`, `date = true`.
    pub clock: ClockConfig,
    /// Volume keys: `[sound] step = 2` (percent per press), `max = 150` (percent the keys may
    /// reach; above 100 boosts quiet sound, as the user's keys always allowed).
    pub sound: SoundConfig,
    /// Temperatures in "celsius" or "fahrenheit".
    pub weather_units: Units,
    /// Per-module extras: `[modules.clock] middle = "kitty -e khal interactive"`.
    pub modules: std::collections::BTreeMap<String, ModuleConfig>,
    /// Per-app notification rules: `[notifications] muted = ["Discord"]` (dropped),
    /// `quiet = ["Steam"]` (kept in the list, no banner).
    pub notifications: NotificationRules,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct NotificationRules {
    pub muted: Vec<String>,
    pub quiet: Vec<String>,
    /// A sound for each notification that shows a banner (`sound = false`: none).
    pub sound: bool,
    /// Apps that show banners without a sound.
    pub silent: Vec<String>,
    /// Seconds a banner stays when the app does not say (`banner = 5`).
    pub banner: u64,
    /// Urgent notifications (a call, a low battery) show even in Do not disturb.
    pub urgent_in_dnd: bool,
}

impl Default for NotificationRules {
    fn default() -> Self {
        Self {
            muted: Vec::new(),
            quiet: Vec::new(),
            sound: true,
            silent: Vec::new(),
            banner: 5,
            urgent_in_dnd: true,
        }
    }
}

/// `[look]`: unset fields leave what is there alone.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LookConfig {
    pub font: Option<String>,
    pub font_size: Option<u32>,
    pub mono: Option<String>,
    pub mono_size: Option<u32>,
    pub icons: Option<String>,
    pub cursor: Option<String>,
}

impl LookConfig {
    /// The app font and size (the shell's own font, 11, unless chosen).
    pub fn font(&self) -> (String, u32) {
        (
            self.font
                .clone()
                .unwrap_or_else(|| crate::look::DEFAULT_FONT.into()),
            self.font_size.unwrap_or(11),
        )
    }
    pub fn mono(&self) -> (String, u32) {
        (
            self.mono
                .clone()
                .unwrap_or_else(|| crate::look::DEFAULT_MONO.into()),
            self.mono_size.unwrap_or(11),
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SoundConfig {
    pub step: u32,
    pub max: u32,
    /// Alert sounds' volume, percent of the output's (`alerts = 70`).
    pub alerts: u32,
    /// A click on each volume key press, at the new volume.
    pub feedback: bool,
    /// A shutter sound when a screenshot is taken.
    pub screenshot: bool,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            step: 2,
            max: 150,
            alerts: 70,
            feedback: false,
            screenshot: true,
        }
    }
}

impl SoundConfig {
    /// Alert volume as pw-play takes it (0 to 1).
    pub fn alert_volume(&self) -> f32 {
        self.alerts.min(100) as f32 / 100.0
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum ClockFormat {
    #[default]
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "12h")]
    H12,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClockConfig {
    pub format: ClockFormat,
    /// The date beside the time in the bar.
    pub date: bool,
    /// The calendar's weeks start on Sunday (else Monday).
    pub week_starts_sunday: bool,
    /// Seconds in the bar's clock (a tick every second only while on).
    pub seconds: bool,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: ClockFormat::H24,
            date: true,
            week_starts_sunday: false,
            seconds: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    #[default]
    Black,
    /// "glass" was a second, clearer look for a while; it reads as frosted now.
    #[serde(alias = "glass")]
    Frosted,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Celsius,
    Fahrenheit,
}

impl Units {
    /// A temperature in these units ("21°").
    pub fn show(self, celsius: f32) -> String {
        match self {
            Units::Celsius => format!("{celsius:.0}°"),
            Units::Fahrenheit => format!("{:.0}°", celsius * 9.0 / 5.0 + 32.0),
        }
    }
}

/// Mouse actions added to a module: a command to run, or `kestrel:<command>` for kestrel's own
/// (`kestrel:toggle control`, `kestrel:volume +5%`). They fill buttons the module itself does
/// not use; its own click stays.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ModuleConfig {
    pub right: Option<String>,
    pub middle: Option<String>,
    pub scroll_up: Option<String>,
    pub scroll_down: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BarStyle {
    /// One continuous bar joined to the screen frame.
    #[default]
    Full,
    /// Separate floating pills, one per section; no frame.
    Pills,
}

/// `[bar]`: module ids per section, in order. A module not listed is off.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct BarConfig {
    pub style: BarStyle,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    /// Pill style: modules that share one pill when they sit next to each other, e.g.
    /// `groups = [["network", "bluetooth", "volume", "battery"]]`. Everything else stands alone.
    pub groups: Vec<Vec<String>>,
    /// The music visualiser in the time pill.
    pub visualiser: bool,
    /// The battery's percentage beside its icon.
    pub battery_percent: bool,
}

impl Default for BarConfig {
    fn default() -> Self {
        let v = |ids: &[&str]| ids.iter().map(|s| (*s).to_string()).collect();
        Self {
            style: BarStyle::Full,
            left: v(&["workspaces", "app"]),
            center: v(&["island", "airpods"]),
            right: v(&[
                "privacy",
                "updates",
                "network",
                "bluetooth",
                "volume",
                "battery",
                "notifications",
                "power",
            ]),
            groups: vec![v(&[
                "tray",
                "privacy",
                "network",
                "bluetooth",
                "volume",
                "battery",
                "notifications",
            ])],
            visualiser: true,
            battery_percent: true,
        }
    }
}

impl BarConfig {
    /// Which group `id` belongs to, if any.
    pub fn group_of(&self, id: &str) -> Option<usize> {
        self.groups.iter().position(|g| g.iter().any(|m| m == id))
    }

    /// An older layout in today's terms: the combined `status` button becomes the separate
    /// network, bluetooth, volume and battery buttons, with the notifications bell that lived
    /// inside it (a separate `notifications` entry, which drew nothing then, goes with it).
    pub fn normalize(&mut self) {
        let has_status = self.contains("status");
        let expand = |list: &mut Vec<String>| {
            // A separate bell only goes where the status button it lived in was (in that
            // section or that group); anywhere else it stays.
            let here = list.iter().any(|m| m == "status");
            let has_status = has_status && here;
            let mut out = Vec::new();
            for id in list.drain(..) {
                if id == "status" {
                    out.extend(crate::modules::STATUS.iter().map(|m| (*m).to_string()));
                    out.push("notifications".into());
                } else if !(has_status && id == "notifications") {
                    out.push(id);
                }
            }
            *list = out;
        };
        expand(&mut self.left);
        expand(&mut self.center);
        expand(&mut self.right);
        for g in &mut self.groups {
            expand(g);
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.left
            .iter()
            .chain(&self.center)
            .chain(&self.right)
            .any(|m| m == id)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Frame {
    pub thickness: f32,
    pub radius: f32,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            thickness: 4.0,
            radius: 20.0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wallpaper: None,
            wallpaper_dir: None,
            frame: Frame::default(),
            workspaces: 5,
            theme_apps: true,
            material: Material::Black,
            accent: None,
            editor: None,
            look: LookConfig::default(),
            weather_city: None,
            weather_latitude: None,
            weather_longitude: None,
            night_light_temperature: 4500,
            night_light_schedule: false,
            cursor_size: None,
            quiet_from: None,
            quiet_to: None,
            bar: BarConfig::default(),
            modules: std::collections::BTreeMap::new(),
            clock: ClockConfig::default(),
            sound: SoundConfig::default(),
            weather_units: Units::Celsius,
            notifications: NotificationRules::default(),
        }
    }
}

/// kestrel's state directory (`$XDG_STATE_HOME/kestrel`): things the shell remembers.
pub fn state_dir() -> PathBuf {
    if cfg!(test) {
        return test_home().join("state").join("kestrel");
    }
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_default()
        .join("kestrel")
}

pub fn path() -> PathBuf {
    if cfg!(test) {
        return test_home()
            .join("config")
            .join("kestrel")
            .join("config.toml");
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default();
    base.join("kestrel").join("config.toml")
}

/// Under `cargo test`, kestrel's own folders (config, state, history, init.lua) are a folder
/// of this test run's own in the temporary directory, which is never created unless a test
/// writes there: no test reads the user's config or writes the user's state, and a test sees the defaults
/// whatever the user's config says.
fn test_home() -> PathBuf {
    std::env::temp_dir().join(format!("kestrel-test-{}", std::process::id()))
}

pub fn load() -> Config {
    read().unwrap_or_else(|e| {
        log::warn!("{e}");
        Config::default()
    })
}

/// The config, or what is wrong with it: "config.toml, line 12: unknown variant `pils`,
/// expected `full` or `pills`". A missing file is the defaults, not an error.
pub fn read() -> Result<Config, String> {
    let p = path();
    let Ok(s) = std::fs::read_to_string(&p) else {
        return Ok(Config::default());
    };
    parse(&s)
}

/// config.toml's text as the config, or what is wrong with it (the line and why).
fn parse(s: &str) -> Result<Config, String> {
    let parsed: Result<Config, toml::de::Error> = toml::from_str(s);
    parsed
        .map(|mut c| {
            c.bar.normalize();
            c
        })
        .map_err(|e| {
            let line = e
                .span()
                // Line breaks before the error, plus one: `lines().count()` of the text before it
                // came one short when the error began at a line's first character.
                .map(|r| {
                    s.as_bytes()[..r.start.min(s.len())]
                        .iter()
                        .filter(|b| **b == b'\n')
                        .count()
                        + 1
                });
            let what = e.message().trim().to_string();
            match line {
                Some(l) => format!("config.toml, line {l}: {what}"),
                None => format!("config.toml: {what}"),
            }
        })
}

/// The `[plugins]` part of config.toml, as text (to notice when it changes).
pub fn plugin_settings() -> String {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("plugins").map(ToString::to_string))
        .unwrap_or_default()
}

/// Saves the wallpaper choice, keeping the rest of the file (and its comments) as it is.
pub fn set_wallpaper(path: &str) {
    set(&[], "wallpaper", toml_edit::value(path));
}

/// Change one setting in config.toml in place (comments and the rest kept as they are).
/// `table` is the path of tables (`&["bar"]`, or `&[]` for a top-level key). The file is
/// watched, so the change applies at once.
/// One change to config.toml: a key in a table (`[]` the top level), and its new value
/// (`Item::None` removes it).
#[derive(Clone, Debug)]
pub struct Edit {
    pub table: Vec<String>,
    pub key: String,
    pub value: toml_edit::Item,
}

impl Edit {
    pub fn new(table: &[&str], key: &str, value: toml_edit::Item) -> Self {
        Self {
            table: table.iter().map(|t| (*t).to_string()).collect(),
            key: key.to_string(),
            value,
        }
    }

    /// Writes it to config.toml.
    pub fn write(self) {
        let table: Vec<&str> = self.table.iter().map(String::as_str).collect();
        set(&table, &self.key, self.value);
    }
}

pub fn set(table: &[&str], key: &str, value: toml_edit::Item) {
    let p = self::path();
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let changed = match edited(&text, table, key, value) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("config: {}: {e}; not changing it", p.display());
            return;
        }
    };
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, changed);
}

/// config.toml's `text` with one key set (`Item::None` removes it), its tables made as needed,
/// comments and everything else kept as written.
fn edited(text: &str, table: &[&str], key: &str, value: toml_edit::Item) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "it does not parse".to_string())?;
    let mut t = doc.as_table_mut();
    for name in table {
        if !t.contains_key(name) {
            t.insert(name, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        t = t
            .get_mut(name)
            .and_then(toml_edit::Item::as_table_mut)
            .ok_or_else(|| format!("[{name}] is not a table"))?;
    }
    t.insert(key, value);
    Ok(doc.to_string())
}

/// A list of strings as a config value.
pub fn list(items: &[String]) -> toml_edit::Item {
    let mut a = toml_edit::Array::new();
    for i in items {
        a.push(i.as_str());
    }
    toml_edit::value(a)
}

pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}

#[cfg(test)]
mod bar_tests {
    use super::*;

    #[test]
    fn status_becomes_separate_buttons() {
        let mut b = BarConfig {
            right: ["updates", "privacy", "status", "power", "notifications"]
                .map(String::from)
                .to_vec(),
            groups: vec![["privacy", "status", "power"].map(String::from).to_vec()],
            ..BarConfig::default()
        };
        b.normalize();
        assert_eq!(
            b.right,
            [
                "updates",
                "privacy",
                "network",
                "bluetooth",
                "volume",
                "battery",
                "notifications",
                "power"
            ]
        );
        assert!(b.groups[0].contains(&"volume".to_string()));
        // A group that never held `status` keeps its bell.
        let mut g = BarConfig {
            right: ["status", "notifications"].map(String::from).to_vec(),
            groups: vec![["battery", "notifications"].map(String::from).to_vec()],
            ..BarConfig::default()
        };
        g.normalize();
        assert_eq!(g.groups[0], ["battery", "notifications"]);
        // Already in today's terms: unchanged.
        let before = b.clone();
        b.normalize();
        assert_eq!(b, before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(parse("").unwrap(), Config::default());
        assert_eq!(parse("# only a comment\n").unwrap(), Config::default());
    }

    #[test]
    fn a_partial_table_keeps_the_other_defaults() {
        let c = parse("[bar]\nleft = [\"clock\"]\n[sound]\nstep = 5\n").unwrap();
        assert_eq!(c.bar.left, ["clock"]);
        assert_eq!(c.bar.right, BarConfig::default().right);
        assert_eq!(c.sound.step, 5);
        assert_eq!(c.sound.max, SoundConfig::default().max);
        assert_eq!(c.workspaces, 5);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let c = parse("future_option = true\n[bar]\nshiny = 3\nstyle = \"pills\"\n").unwrap();
        assert_eq!(c.bar.style, BarStyle::Pills);
    }

    #[test]
    fn a_wrong_value_names_its_line() {
        let e = parse("workspaces = 3\n\n[bar]\nstyle = \"pils\"\n").unwrap_err();
        assert!(e.starts_with("config.toml, line 4:"), "{e}");
        assert!(e.contains("pils"), "{e}");
    }

    #[test]
    fn an_error_at_the_start_of_a_line_names_that_line() {
        // The error's span starts at the line's first character.
        let e = parse("workspaces = 3\ntheme_apps = true\n= 5\n").unwrap_err();
        assert!(e.starts_with("config.toml, line 3:"), "{e}");
        let e = parse("[bar]\n[bar]\n").unwrap_err();
        assert!(e.starts_with("config.toml, line 2:"), "{e}");
    }

    #[test]
    fn a_wrong_type_is_an_error_not_a_default() {
        let e = parse("workspaces = \"five\"\n").unwrap_err();
        assert!(e.starts_with("config.toml, line 1:"), "{e}");
    }

    #[test]
    fn glass_reads_as_frosted() {
        assert_eq!(
            parse("material = \"glass\"").unwrap().material,
            Material::Frosted
        );
        assert_eq!(
            parse("material = \"frosted\"").unwrap().material,
            Material::Frosted
        );
    }

    #[test]
    fn an_old_layout_is_read_in_todays_terms() {
        let c = parse("[bar]\nright = [\"status\", \"power\"]\n").unwrap();
        assert!(!c.bar.contains("status"));
        assert!(c.bar.contains("network") && c.bar.contains("notifications"));
        assert_eq!(c.bar.right.last().map(String::as_str), Some("power"));
    }

    #[test]
    fn module_and_notification_tables_are_read() {
        let c = parse(
            "[modules.clock]\nmiddle = \"kitty\"\n[notifications]\nmuted = [\"Discord\"]\nbanner = 8\n",
        )
        .unwrap();
        assert_eq!(c.modules["clock"].middle.as_deref(), Some("kitty"));
        assert_eq!(c.notifications.muted, ["Discord"]);
        assert_eq!(c.notifications.banner, 8);
        assert!(c.notifications.sound, "unset keys keep their default");
    }

    #[test]
    fn an_edit_keeps_comments_and_other_keys() {
        let before =
            "# my config\nworkspaces = 3 # three\n\n[bar]\n# the left side\nleft = [\"app\"]\n";
        let after = edited(before, &[], "theme_apps", toml_edit::value(false)).unwrap();
        assert!(after.contains("# my config"));
        assert!(after.contains("workspaces = 3 # three"));
        assert!(after.contains("# the left side"));
        let c = parse(&after).unwrap();
        assert!(!c.theme_apps);
        assert_eq!(c.workspaces, 3);
        assert_eq!(c.bar.left, ["app"]);
    }

    #[test]
    fn an_edit_replaces_a_value_in_place() {
        let before = "[sound]\nstep = 2\nmax = 150\n";
        let after = edited(before, &["sound"], "step", toml_edit::value(5)).unwrap();
        assert_eq!(after, "[sound]\nstep = 5\nmax = 150\n");
    }

    #[test]
    fn an_edit_makes_the_tables_it_needs() {
        let after = edited("", &["plugins", "disk"], "enabled", toml_edit::value(false)).unwrap();
        let t: toml::Table = after.parse().unwrap();
        assert_eq!(t["plugins"]["disk"]["enabled"].as_bool(), Some(false));
    }

    #[test]
    fn an_edit_of_none_removes_the_key() {
        let after = edited(
            "cursor_size = 32\nworkspaces = 4\n",
            &[],
            "cursor_size",
            toml_edit::Item::None,
        )
        .unwrap();
        assert!(!after.contains("cursor_size"), "{after}");
        assert_eq!(parse(&after).unwrap().workspaces, 4);
    }

    #[test]
    fn an_edit_never_rewrites_a_broken_file() {
        assert!(edited("[bar\nleft = 1", &[], "workspaces", toml_edit::value(3)).is_err());
        // A table in the way that is a value: nothing is changed.
        assert!(edited("bar = 3\n", &["bar"], "left", toml_edit::value(1)).is_err());
    }

    #[test]
    fn a_bar_list_round_trips() {
        let ids = ["clock".to_string(), "media".to_string()];
        let after = edited("", &["bar"], "center", list(&ids)).unwrap();
        assert_eq!(parse(&after).unwrap().bar.center, ids);
        let after = edited(&after, &["bar"], "center", list(&[])).unwrap();
        assert!(parse(&after).unwrap().bar.center.is_empty());
    }

    #[test]
    fn groups_and_membership() {
        let b = BarConfig::default();
        assert_eq!(b.group_of("volume"), Some(0));
        assert_eq!(b.group_of("workspaces"), None);
        assert!(b.contains("workspaces") && b.contains("island") && b.contains("power"));
        assert!(
            !b.contains("tray"),
            "a group member that is not in the bar is not in it"
        );
    }

    #[test]
    fn temperatures_and_alert_volume() {
        assert_eq!(Units::Celsius.show(21.4), "21°");
        assert_eq!(Units::Fahrenheit.show(100.0), "212°");
        assert_eq!(Units::Fahrenheit.show(-40.0), "-40°");
        let s = SoundConfig {
            alerts: 250,
            ..SoundConfig::default()
        };
        assert_eq!(s.alert_volume(), 1.0);
        assert!((SoundConfig::default().alert_volume() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn only_a_leading_tilde_slash_is_home() {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
        assert_eq!(expand("~/Pictures"), home.join("Pictures"));
        assert_eq!(expand("/srv/x"), PathBuf::from("/srv/x"));
        assert_eq!(expand("~other/x"), PathBuf::from("~other/x"));
        assert_eq!(expand("a/~/b"), PathBuf::from("a/~/b"));
    }

    #[test]
    fn tests_never_see_his_folders() {
        let tmp = std::env::temp_dir();
        assert!(path().starts_with(&tmp));
        assert!(state_dir().starts_with(&tmp));
        assert!(crate::lua::path().starts_with(&tmp));
        assert!(crate::lua::plugins_dir().starts_with(&tmp));
    }
}
