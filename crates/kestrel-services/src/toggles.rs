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
/// A hyprsunset kestrel did not start but looks after (one left running by the shell before a
/// restart): watched like its own.
static ADOPTED: Mutex<Option<u32>> = Mutex::new(None);

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

fn hyprsunset_pid() -> Option<u32> {
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
        let pid = e.file_name().to_str()?.parse::<u32>().ok()?;
        std::fs::read_to_string(e.path().join("comm"))
            .is_ok_and(|c| c.trim() == "hyprsunset")
            .then_some(pid)
    })
}

fn hyprsunset_running() -> bool {
    hyprsunset_pid().is_some()
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
        *ADOPTED.lock().unwrap() = None;
        if hyprsunset_running() {
            let _ = Command::new("pkill").args(["-x", "hyprsunset"]).status();
        }
        return;
    }
    if running {
        if g.is_none()
            && let Some(pid) = hyprsunset_pid()
        {
            let mut a = ADOPTED.lock().unwrap();
            if *a != Some(pid) {
                *a = Some(pid);
                watch(pid);
            }
        }
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
        Ok(c) => {
            watch(c.id());
            *g = Some(c);
        }
        Err(e) => log::warn!("screen tint: {e}"),
    }
}

/// Waits (blocked on a pidfd: no polling) for hyprsunset `pid` (its own or adopted) to end. If it ends on its own
/// while the tint is still wanted (it crashed: one segfaulted 2026-11-14 during a burst of
/// shell restarts), it is started again a second later, so the tint never silently vanishes.
fn watch(pid: u32) {
    let _ = std::thread::Builder::new()
        .name("kestrel-sunset".into())
        .spawn(move || {
            // Safety: pidfd_open on our own child; the fd is owned and closed below.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
            if fd < 0 {
                return;
            }
            let fd = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd as i32) };
            let mut pfd = libc::pollfd {
                fd: std::os::fd::AsRawFd::as_raw_fd(&fd),
                events: libc::POLLIN,
                revents: 0,
            };
            // Safety: one valid pollfd; blocks until the process ends.
            while unsafe { libc::poll(&mut pfd, 1, -1) } < 0 {}
            std::thread::sleep(std::time::Duration::from_secs(1));
            let t = TINT.lock().unwrap();
            let ours = SUNSET
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|c| c.id() == pid)
                || {
                    let mut a = ADOPTED.lock().unwrap();
                    (*a == Some(pid)).then(|| *a = None).is_some()
                };
            // Stopped on purpose (taken out of SUNSET first) or replaced: nothing to do.
            if ours && (t.night.is_some() || t.gamma < 100) {
                log::warn!("screen tint: hyprsunset ended on its own; starting it again");
                apply(&t);
            }
        });
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
