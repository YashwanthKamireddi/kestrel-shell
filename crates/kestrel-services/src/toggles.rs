//! Night light and caffeine.
//!
//! Night light and dimming of external screens run hyprsunset (Hyprland's own colour tool)
//! while either is in use. Caffeine holds a logind idle inhibitor, which
//! hypridle honours; dropping the file descriptor releases it.

use std::os::fd::OwnedFd;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static SUNSET: Mutex<Option<Child>> = Mutex::new(None);
static INHIBIT: Mutex<Option<OwnedFd>> = Mutex::new(None);

/// The screen's tint: night light's warmth (None while off) and the dimming of screens that
/// have no backlight kestrel can set (an external monitor), in percent of full. Both are done
/// by one hyprsunset (Hyprland's colour tool): it runs while either is in use.
struct Tint {
    night: Option<u32>,
    gamma: u32,
}

static TINT: Mutex<Tint> = Mutex::new(Tint {
    night: None,
    gamma: 100,
});

/// Dimming never goes below this, so the screen can't be made unreadably dark.
pub const MIN_GAMMA: u32 = 30;

fn hyprsunset_running() -> bool {
    std::fs::read_dir("/proc").is_ok_and(|d| {
        d.flatten().any(|e| {
            std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "hyprsunset")
        })
    })
}

fn hyprctl(args: &[&str]) {
    let _ = Command::new("hyprctl")
        .arg("hyprsunset")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Makes hyprsunset match the tint: started, told, or stopped.
fn apply(t: &Tint) {
    let mut g = SUNSET.lock().unwrap();
    if let Some(c) = g.as_mut()
        && c.try_wait().ok().flatten().is_some()
    {
        *g = None;
    }
    let wanted = t.night.is_some() || t.gamma < 100;
    let running = g.is_some() || hyprsunset_running();
    if !wanted {
        if let Some(mut c) = g.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        // One started elsewhere (e.g. before kestrel) is stopped too.
        if hyprsunset_running() {
            let _ = Command::new("pkill").args(["-x", "hyprsunset"]).status();
        }
        return;
    }
    if running {
        match t.night {
            Some(k) => hyprctl(&["temperature", &k.to_string()]),
            None => hyprctl(&["identity"]),
        }
        hyprctl(&["gamma", &t.gamma.to_string()]);
        return;
    }
    let mut cmd = Command::new("hyprsunset");
    match t.night {
        Some(k) => cmd.args(["-t", &k.to_string()]),
        None => cmd.arg("-i"),
    };
    cmd.args(["-g", &t.gamma.to_string()]);
    match cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(c) => *g = Some(c),
        Err(e) => log::warn!("screen tint: {e}"),
    }
}

/// Whether night light is on (also when a hyprsunset was already running at start).
pub fn night_light() -> bool {
    let t = TINT.lock().unwrap();
    t.night.is_some() || (t.gamma == 100 && hyprsunset_running())
}

/// Turns night light on at `kelvin`, or off.
pub fn set_night_light(on: bool, kelvin: u32) {
    let mut t = TINT.lock().unwrap();
    t.night = on.then_some(kelvin);
    apply(&t);
}

/// Changes night light's warmth at once (while it is on).
pub async fn set_temperature(kelvin: u32) {
    let _ = tokio::task::spawn_blocking(move || {
        let mut t = TINT.lock().unwrap();
        if t.night.is_some() {
            t.night = Some(kelvin);
            apply(&t);
        }
    })
    .await;
}

/// Software dimming, 0 to 1 (for screens without a settable backlight).
pub fn dim_level() -> f32 {
    TINT.lock().unwrap().gamma as f32 / 100.0
}

pub async fn set_dim(level: f32) {
    let gamma = ((level.clamp(0.0, 1.0) * 100.0).round() as u32).max(MIN_GAMMA);
    let _ = tokio::task::spawn_blocking(move || {
        let mut t = TINT.lock().unwrap();
        if t.gamma != gamma {
            t.gamma = gamma;
            apply(&t);
        }
    })
    .await;
}

pub fn caffeine() -> bool {
    INHIBIT.lock().unwrap().is_some()
}

pub async fn set_caffeine(on: bool) {
    if !on {
        INHIBIT.lock().unwrap().take();
        return;
    }
    let r = async {
        let c = zbus::Connection::system().await?;
        let fd: zbus::zvariant::OwnedFd = super::dbus::proxy(
            &c,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?
        .call("Inhibit", &("idle", "kestrel", "Caffeine is on", "block"))
        .await?;
        zbus::Result::Ok(OwnedFd::from(fd))
    }
    .await;
    match r {
        Ok(fd) => *INHIBIT.lock().unwrap() = Some(fd),
        Err(e) => log::warn!("caffeine: {e}"),
    }
}
