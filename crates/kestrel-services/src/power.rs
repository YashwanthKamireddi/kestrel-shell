//! Battery (UPower) and power mode (power-profiles-daemon) on the system bus, and screen
//! brightness (sysfs to read, logind to write — no root, no helper).

use super::dbus;
use futures::{SinkExt, Stream, StreamExt};
use std::path::PathBuf;
use zbus::Connection;

const UPOWER: &str = "org.freedesktop.UPower";
const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE: &str = "org.freedesktop.UPower.Device";
const PPD: &str = "org.freedesktop.UPower.PowerProfiles";
const PPD_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    PowerSaver,
    #[default]
    Balanced,
    Performance,
}

impl Profile {
    fn from_str(s: &str) -> Self {
        match s {
            "power-saver" => Self::PowerSaver,
            "performance" => Self::Performance,
            _ => Self::Balanced,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::PowerSaver => "Power saver",
            Self::Balanced => "Balanced",
            Self::Performance => "Performance",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::PowerSaver => Self::Balanced,
            Self::Balanced => Self::Performance,
            Self::Performance => Self::PowerSaver,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub has_battery: bool,
    /// Running on battery (no charger).
    pub on_battery: bool,
    pub percent: f32,
    pub charging: bool,
    pub full: bool,
    /// Seconds until empty (discharging) or full (charging); 0 when unknown.
    pub time_left: i64,
    pub profile: Option<Profile>,
}

/// How worn the battery is: what it holds now as a share of when new, and charge cycles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Health {
    pub percent: u8,
    pub cycles: Option<u32>,
}

/// The battery's health from the kernel (charge_full / charge_full_design, or the energy
/// pair); None without a battery.
pub fn health() -> Option<Health> {
    let dir = std::fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| std::fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery"))?;
    let num = |f: &str| -> Option<f64> {
        std::fs::read_to_string(dir.join(f))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    let (now, design) = match (num("charge_full"), num("charge_full_design")) {
        (Some(n), Some(d)) => (n, d),
        _ => (num("energy_full")?, num("energy_full_design")?),
    };
    if design <= 0.0 {
        return None;
    }
    Some(Health {
        percent: (now / design * 100.0).round().clamp(0.0, 100.0) as u8,
        cycles: num("cycle_count").map(|c| c as u32).filter(|c| *c > 0),
    })
}

/// How the battery charges (the firmware's own modes, where the laptop has them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charging {
    /// Up to full, the usual way.
    Standard,
    /// The firmware learns the routine and keeps the battery lower when on the charger.
    Adaptive,
    /// Stops at 80% (charges again below 75%): the battery wears much slower.
    Limit80,
}

impl Charging {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Adaptive => "Adaptive",
            Self::Limit80 => "Stop at 80%",
        }
    }
}

fn battery_dir() -> Option<std::path::PathBuf> {
    std::fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| std::fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery"))
}

/// The charging modes this battery offers, and the one in use (None when the laptop has no
/// such control, or it cannot be written: kestrel's udev rule makes it writable).
pub fn charging() -> Option<(Vec<Charging>, Charging)> {
    let dir = battery_dir()?;
    let types = std::fs::read_to_string(dir.join("charge_types")).ok()?;
    let writable = std::fs::OpenOptions::new()
        .write(true)
        .open(dir.join("charge_types"))
        .is_ok();
    if !writable {
        return None;
    }
    let mut offered = vec![Charging::Standard];
    if types
        .split_whitespace()
        .any(|t| t.trim_matches(['[', ']']) == "Adaptive")
    {
        offered.push(Charging::Adaptive);
    }
    let custom = types
        .split_whitespace()
        .any(|t| t.trim_matches(['[', ']']) == "Custom")
        && dir.join("charge_control_end_threshold").exists();
    if custom {
        offered.push(Charging::Limit80);
    }
    let now = types
        .split_whitespace()
        .find(|t| t.starts_with('['))
        .map(|t| t.trim_matches(['[', ']']))
        .unwrap_or("Standard");
    let current = match now {
        "Adaptive" => Charging::Adaptive,
        "Custom" => Charging::Limit80,
        _ => Charging::Standard,
    };
    Some((offered, current))
}

/// Sets the charging mode. For "Stop at 80%" the limits are written first (75 to start,
/// 80 to stop; the end before the start, so start < end holds at every step), then the mode.
pub fn set_charging(c: Charging) -> Result<(), String> {
    let dir = battery_dir().ok_or("no battery")?;
    let write = |f: &str, v: &str| std::fs::write(dir.join(f), v).map_err(|e| format!("{f}: {e}"));
    match c {
        Charging::Standard => write("charge_types", "Standard"),
        Charging::Adaptive => write("charge_types", "Adaptive"),
        Charging::Limit80 => {
            write("charge_control_end_threshold", "80")?;
            write("charge_control_start_threshold", "75")?;
            write("charge_types", "Custom")
        }
    }
}

async fn read(c: &Connection) -> State {
    let present: bool = dbus::get(c, UPOWER, DISPLAY, DEVICE, "IsPresent")
        .await
        .unwrap_or(false);
    let percent: f64 = dbus::get(c, UPOWER, DISPLAY, DEVICE, "Percentage")
        .await
        .unwrap_or(0.0);
    let state: u32 = dbus::get(c, UPOWER, DISPLAY, DEVICE, "State")
        .await
        .unwrap_or(0);
    let charging = state == 1 || state == 5;
    let time_left: i64 = dbus::get(
        c,
        UPOWER,
        DISPLAY,
        DEVICE,
        if charging {
            "TimeToFull"
        } else {
            "TimeToEmpty"
        },
    )
    .await
    .unwrap_or(0);
    let profile = dbus::get::<String>(c, PPD, PPD_PATH, PPD, "ActiveProfile")
        .await
        .ok()
        .map(|s| Profile::from_str(&s));
    let on_battery: bool = dbus::get(c, UPOWER, "/org/freedesktop/UPower", UPOWER, "OnBattery")
        .await
        .unwrap_or(false);
    State {
        has_battery: present,
        on_battery,
        percent: percent as f32,
        charging,
        full: state == 4,
        time_left,
        profile,
    }
}

pub async fn set_profile(p: Profile) {
    let r = async {
        let c = Connection::system().await?;
        dbus::set(&c, PPD, PPD_PATH, PPD, "ActiveProfile", p.as_str()).await
    }
    .await;
    if let Err(e) = r {
        log::warn!("power profile: {e}");
    }
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(4, async |mut out| {
        let Ok(c) = Connection::system().await else {
            return;
        };
        let (Ok(a), Ok(b)) = (
            dbus::changes(&c, UPOWER).await,
            dbus::changes(&c, PPD).await,
        ) else {
            return;
        };
        let mut changes = futures::stream::select(a, b);
        let mut last = None;
        loop {
            let s = read(&c).await;
            if last.as_ref() != Some(&s) {
                last = Some(s.clone());
                let _ = out.send(s).await;
            }
            if changes.next().await.is_none() {
                return;
            }
        }
    })
}

// --- Brightness ---------------------------------------------------------------------------

fn backlight() -> Option<PathBuf> {
    let dir = std::fs::read_dir("/sys/class/backlight").ok()?;
    let mut all: Vec<PathBuf> = dir.flatten().map(|e| e.path()).collect();
    // Prefer the vendor/firmware device over generic ACPI ones.
    all.sort_by_key(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("acpi"))
    });
    all.into_iter().next()
}

fn read_num(p: &std::path::Path) -> Option<f32> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// Current brightness 0..1, if there is a backlight.
pub fn brightness() -> Option<f32> {
    let b = backlight()?;
    let max = read_num(&b.join("max_brightness"))?;
    let cur = read_num(&b.join("brightness"))?;
    (max > 0.0).then(|| cur / max)
}

pub async fn set_brightness(v: f32) {
    let r = async {
        let b = backlight().ok_or_else(|| zbus::Error::Failure("no backlight".into()))?;
        let name = b
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let max = read_num(&b.join("max_brightness")).unwrap_or(1.0);
        // Never fully dark: 1 % is the floor.
        let raw = (v.clamp(0.01, 1.0) * max).round() as u32;
        let c = Connection::system().await?;
        let _: () = dbus::proxy(
            &c,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )
        .await?
        .call("SetBrightness", &("backlight", name.as_str(), raw))
        .await?;
        zbus::Result::Ok(())
    }
    .await;
    if let Err(e) = r {
        log::warn!("brightness: {e}");
    }
}
