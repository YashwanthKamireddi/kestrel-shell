//! Night light and caffeine.
//!
//! Night light runs hyprsunset (Hyprland's own colour-temperature tool) while it is on; the
//! state is simply whether that process is alive. Caffeine holds a logind idle inhibitor, which
//! hypridle honours; dropping the file descriptor releases it.

use std::os::fd::OwnedFd;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static SUNSET: Mutex<Option<Child>> = Mutex::new(None);
static INHIBIT: Mutex<Option<OwnedFd>> = Mutex::new(None);

fn hyprsunset_running() -> bool {
    std::fs::read_dir("/proc").is_ok_and(|d| {
        d.flatten().any(|e| {
            std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "hyprsunset")
        })
    })
}

pub fn night_light() -> bool {
    let mut g = SUNSET.lock().unwrap();
    if let Some(c) = g.as_mut()
        && c.try_wait().ok().flatten().is_some()
    {
        *g = None;
    }
    g.is_some() || hyprsunset_running()
}

/// Turns night light on at `kelvin`, or off.
pub fn set_night_light(on: bool, kelvin: u32) {
    let mut g = SUNSET.lock().unwrap();
    if on {
        if g.is_none() && !hyprsunset_running() {
            match Command::new("hyprsunset")
                .args(["-t", &kelvin.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => *g = Some(c),
                Err(e) => log::warn!("night light: {e}"),
            }
        }
    } else {
        if let Some(mut c) = g.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        // One started elsewhere (e.g. before kestrel) is stopped too.
        if hyprsunset_running() {
            let _ = Command::new("pkill").args(["-x", "hyprsunset"]).status();
        }
    }
}

/// Changes a running night light's warmth at once (hyprsunset listens through hyprctl).
pub async fn set_temperature(kelvin: u32) {
    if !night_light() {
        return;
    }
    let _ = tokio::process::Command::new("hyprctl")
        .args(["hyprsunset", "temperature", &kelvin.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
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
