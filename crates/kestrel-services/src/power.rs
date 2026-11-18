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
    /// Plugged in and deliberately not charging (UPower's "pending charge": a charge limit or
    /// adaptive charging holding it). Not charging: no pulse, the bolt stays still.
    pub held: bool,
    pub full: bool,
    /// Seconds until empty (discharging) or full (charging); 0 when unknown.
    pub time_left: i64,
    pub profile: Option<Profile>,
    /// UPower's warning level for the battery: 1 none, 3 low, 4 critical, 5 action (it is
    /// about to act); from its own thresholds.
    pub warning: u32,
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
    /// Charges as fast as it can (Dell ExpressCharge): handy, wears the battery more.
    Fast,
    /// For a laptop that stays on the charger (Dell "Primarily AC use").
    Trickle,
    /// Stops at 80% (charges again below 75%): the battery wears much slower.
    Limit80,
    /// Custom limits set elsewhere (the firmware setup): stops at this percent. Shown as it
    /// is, never passed off as 80%.
    Custom(u8),
}

impl Charging {
    pub fn label(self) -> String {
        match self {
            Self::Standard => "Standard".into(),
            Self::Adaptive => "Adaptive".into(),
            Self::Fast => "Fast".into(),
            Self::Trickle => "Mostly plugged in".into(),
            Self::Limit80 => "Stop at 80%".into(),
            Self::Custom(n) => format!("Stop at {n}%"),
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
    let has = |m: &str| {
        types
            .split_whitespace()
            .any(|t| t.trim_matches(['[', ']']) == m)
    };
    let mut offered = vec![Charging::Standard];
    if has("Adaptive") {
        offered.push(Charging::Adaptive);
    }
    if has("Fast") {
        offered.push(Charging::Fast);
    }
    if has("Trickle") {
        offered.push(Charging::Trickle);
    }
    let custom = has("Custom") && dir.join("charge_control_end_threshold").exists();
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
        "Fast" => Charging::Fast,
        "Trickle" => Charging::Trickle,
        "Custom" => {
            let end = std::fs::read_to_string(dir.join("charge_control_end_threshold"))
                .ok()
                .and_then(|t| t.trim().parse::<u8>().ok())
                .unwrap_or(80);
            if end == 80 {
                Charging::Limit80
            } else {
                Charging::Custom(end)
            }
        }
        _ => Charging::Standard,
    };
    // Limits set elsewhere stay shown (and chosen) beside the others.
    if let Charging::Custom(_) = current {
        offered.push(current);
    }
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
        Charging::Fast => write("charge_types", "Fast"),
        Charging::Trickle => write("charge_types", "Trickle"),
        // Choosing the limits already in place: only the mode.
        Charging::Custom(_) => write("charge_types", "Custom"),
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
    // 1 charging; 5 pending charge (held below full by a limit), which is not charging.
    let charging = state == 1;
    let held = state == 5;
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
    let warning: u32 = dbus::get(c, UPOWER, DISPLAY, DEVICE, "WarningLevel")
        .await
        .unwrap_or(1);
    State {
        has_battery: present,
        on_battery,
        percent: percent as f32,
        charging,
        held,
        full: state == 4,
        time_left,
        profile,
        warning,
    }
}

// --- Low battery -----------------------------------------------------------------------------
//
// UPower decides when the battery is low and critical and what happens when it is nearly empty
// (/etc/UPower/UPower.conf). kestrel warns at its levels; a choice made in Settings goes into
// /etc/UPower/UPower.conf.d/50-kestrel.conf (pkexec), then UPower is restarted (harmless: it
// only reads batteries).

/// The low-battery policy: warn at this percent, and what happens when nearly empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LowBattery {
    pub warn_at: u32,
    pub action_at: u32,
    /// UPower's word for the end action as it will be taken ("PowerOff", "Hibernate",
    /// "HybridSleep", "Suspend"/"Sleep").
    pub action: String,
}

fn upower_conf_value(key: &str) -> Option<String> {
    // The main file, then drop-ins in name order: the last one wins, as UPower reads them.
    let mut files = vec![std::path::PathBuf::from("/etc/UPower/UPower.conf")];
    if let Ok(rd) = std::fs::read_dir("/etc/UPower/UPower.conf.d") {
        let mut more: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "conf"))
            .collect();
        more.sort();
        files.extend(more);
    }
    let mut value = None;
    for f in files {
        if let Ok(t) = std::fs::read_to_string(f) {
            for l in t.lines() {
                if let Some(v) = l.trim().strip_prefix(&format!("{key}=")) {
                    value = Some(v.trim().to_string());
                }
            }
        }
    }
    value
}

/// The policy as UPower has it now.
pub async fn low_battery() -> Option<LowBattery> {
    let c = Connection::system().await.ok()?;
    let action: String = dbus::proxy(&c, UPOWER, "/org/freedesktop/UPower", UPOWER)
        .await
        .ok()?
        .call("GetCriticalAction", &())
        .await
        .ok()?;
    let num = |k: &str, d: u32| {
        upower_conf_value(k)
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(d, |f| f.round() as u32)
    };
    Some(LowBattery {
        warn_at: num("PercentageLow", 20),
        action_at: num("PercentageAction", 2),
        action,
    })
}

/// Saves the warning level and the end action ("PowerOff" or "Suspend"), asking once.
pub async fn set_low_battery(warn_at: u32, action: &str) -> Result<(), String> {
    let action = if action == "Suspend" {
        "Suspend"
    } else {
        "PowerOff"
    };
    let warn_at = warn_at.clamp(10, 50);
    // Critical stays UPower's 5%, below the warning (UPower requires low > critical > action).
    let critical = 5.min(warn_at - 1);
    let conf = format!(
        "# Written by kestrel (Settings > Power).\n[UPower]\nPercentageLow={warn_at}.0\nPercentageCritical={critical}.0\nCriticalPowerAction={action}\nAllowRiskyCriticalPowerAction={}\n",
        action == "Suspend"
    );
    let script = format!(
        "mkdir -p /etc/UPower/UPower.conf.d && printf '{conf}' > /etc/UPower/UPower.conf.d/50-kestrel.conf && systemctl restart upower"
    );
    let ok = tokio::process::Command::new("pkexec")
        .args(["/bin/sh", "-c", &script])
        .status()
        .await
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("The low battery settings were not changed".into())
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

// --- Lid and power button ----------------------------------------------------------------------
//
// What closing the lid and pressing the power button do, as logind decides it. Read from logind
// itself (so the system's own drop-ins show as they are); a change goes into
// /etc/systemd/logind.conf.d/50-kestrel-power.conf (late in the order, so it wins over other
// drop-ins) through pkexec, then logind is reloaded (never restarted: a restart ends sessions).

/// What an event does: one of logind's actions kestrel offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Suspend,
    Lock,
    PowerOff,
    Nothing,
}

impl Action {
    fn from_logind(s: &str) -> Self {
        match s {
            "lock" => Self::Lock,
            "poweroff" => Self::PowerOff,
            "ignore" => Self::Nothing,
            _ => Self::Suspend,
        }
    }
    fn logind(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Lock => "lock",
            Self::PowerOff => "poweroff",
            Self::Nothing => "ignore",
        }
    }
}

/// The lid on battery, on AC, with an external display; and the power button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lid {
    pub battery: Action,
    pub charging: Action,
    pub docked: Action,
    pub power_key: Action,
}

/// The actions as logind has them now.
pub async fn lid() -> Option<Lid> {
    let c = zbus::Connection::system().await.ok()?;
    let get = |k: &'static str| {
        let c = c.clone();
        async move {
            crate::dbus::get::<String>(
                &c,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                k,
            )
            .await
            .map(|s| Action::from_logind(&s))
            .ok()
        }
    };
    Some(Lid {
        battery: get("HandleLidSwitch").await?,
        charging: get("HandleLidSwitchExternalPower").await?,
        docked: get("HandleLidSwitchDocked").await?,
        power_key: get("HandlePowerKey").await?,
    })
}

/// The drop-in kestrel writes for `l`.
pub fn lid_conf(l: &Lid) -> String {
    format!(
        "# Written by kestrel (Settings > Power). Loaded after the other drop-ins, so it wins.\n\
         [Login]\n\
         HandleLidSwitch={}\n\
         HandleLidSwitchExternalPower={}\n\
         HandleLidSwitchDocked={}\n\
         HandlePowerKey={}\n",
        l.battery.logind(),
        l.charging.logind(),
        l.docked.logind(),
        l.power_key.logind()
    )
}

/// Saves `l` (asks for the administrator password once) and reloads logind.
pub async fn set_lid(l: Lid) -> Result<(), String> {
    // Every value is one of logind's fixed words: nothing typed reaches the shell.
    let script = format!(
        "mkdir -p /etc/systemd/logind.conf.d && printf '%s' '{}' > /etc/systemd/logind.conf.d/50-kestrel-power.conf && systemctl reload systemd-logind",
        lid_conf(&l)
    );
    let ok = tokio::process::Command::new("pkexec")
        .args(["/bin/sh", "-c", &script])
        .status()
        .await
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("The lid and power button settings were not changed".into())
    }
}

#[cfg(test)]
mod lid_tests {
    use super::*;

    #[test]
    fn writes_the_drop_in() {
        let l = Lid {
            battery: Action::Suspend,
            charging: Action::Nothing,
            docked: Action::Nothing,
            power_key: Action::PowerOff,
        };
        let c = lid_conf(&l);
        assert!(c.contains("HandleLidSwitch=suspend\n"));
        assert!(c.contains("HandleLidSwitchExternalPower=ignore\n"));
        assert!(c.contains("HandlePowerKey=poweroff\n"));
        // Nothing in it may break the single-quoted shell string.
        assert!(!c.contains('\''));
    }
}
