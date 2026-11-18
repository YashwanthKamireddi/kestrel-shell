//! Night light and caffeine.
//!
//! Night light and dimming of external screens run hyprsunset (Hyprland's own colour tool)
//! while either is in use, as a user service of its own (`kestrel-hyprsunset.service`):
//! systemd starts it again if it fails, and a shell restart or crash leaves the screen's tint
//! alone (as kestrel's child it was stopped and started again with every shell restart).
//! Caffeine holds a logind idle inhibitor, which hypridle honours; dropping the file
//! descriptor releases it.

use std::os::fd::OwnedFd;
use std::sync::Mutex;

static INHIBIT: Mutex<Option<OwnedFd>> = Mutex::new(None);
const UNIT: &str = "kestrel-hyprsunset.service";

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

/// This session's hyprsunset (another Hyprland's, a test compositor's, is not ours to touch).
fn hyprsunset_pid() -> Option<u32> {
    crate::session::processes("hyprsunset")
        .first()
        .map(|(pid, _)| *pid)
}

fn hyprsunset_running() -> bool {
    hyprsunset_pid().is_some()
}

/// Tells the running hyprsunset something (`temperature 4000`, `identity`, `gamma 80`) on its
/// own socket, as `hyprctl hyprsunset` does (Hyprland's hyprctl/src/main.cpp, requestIPC).
fn tell(cmd: &str) {
    use std::io::{Read, Write};
    let Some(dir) = crate::hyprland::socket_dir() else {
        return;
    };
    let sent =
        std::os::unix::net::UnixStream::connect(dir.join(".hyprsunset.sock")).and_then(|mut s| {
            s.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
            s.write_all(cmd.as_bytes())?;
            // The reply ("ok" or the error) ends the exchange.
            let mut reply = [0u8; 256];
            let n = s.read(&mut reply)?;
            Ok(String::from_utf8_lossy(&reply[..n]).trim().to_string())
        });
    match sent {
        Ok(r) if r.is_empty() || r == "ok" => {}
        Ok(r) => log::warn!("hyprsunset {cmd}: {r}"),
        Err(e) => log::warn!("hyprsunset {cmd}: {e}"),
    }
}

/// The program's full path (systemd runs a path, not a name).
fn find(program: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|d| std::path::Path::new(d).join(program))
        .find(|p| p.is_file())
}

/// Whether hyprsunset's socket answers yet (it says nothing when it is ready).
fn socket_ready() -> bool {
    crate::hyprland::socket_dir().is_some_and(|d| {
        std::os::unix::net::UnixStream::connect(d.join(".hyprsunset.sock")).is_ok()
    })
}

/// Makes hyprsunset match the tint as it is now: started, told, or stopped. One change at a
/// time, each with the newest values (a slider sends many).
async fn apply() {
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = ONE.lock().await;
    let (night, gamma) = {
        let t = TINT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (t.night, t.gamma)
    };
    let wanted = night.is_some() || gamma < 100;
    let running = tokio::task::spawn_blocking(hyprsunset_running)
        .await
        .unwrap_or(false);
    if !wanted {
        if let Err(e) = crate::systemd::stop_user_unit(UNIT).await {
            log::warn!("screen tint: {e}");
        }
        // One not run as kestrel's service (started by hand, or by an older kestrel): only
        // this session's (pkill -x stopped every hyprsunset on the machine).
        if let Some(pid) = hyprsunset_pid() {
            // Safety: SIGTERM to this session's hyprsunset (checked by hyprsunset_pid).
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
        return;
    }
    if running {
        let _ = tokio::task::spawn_blocking(move || {
            match night {
                Some(k) => tell(&format!("temperature {k}")),
                None => tell("identity"),
            }
            tell(&format!("gamma {gamma}"));
        })
        .await;
        return;
    }
    let Some(bin) = find("hyprsunset") else {
        log::warn!("screen tint: hyprsunset is not installed");
        return;
    };
    let bin = bin.to_string_lossy().into_owned();
    let (k, g) = (night.map(|k| k.to_string()), gamma.to_string());
    let mut argv: Vec<&str> = vec![&bin];
    match &k {
        Some(k) => argv.extend(["-t", k]),
        None => argv.push("-i"),
    }
    argv.extend(["-g", &g]);
    if let Err(e) =
        crate::systemd::run_service(UNIT, "Screen tint for kestrel (hyprsunset)", &argv).await
    {
        log::warn!("screen tint: {e}");
        return;
    }
    // Ready for the next change once it listens (it started with these values).
    for _ in 0..100 {
        if tokio::task::spawn_blocking(socket_ready)
            .await
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Whether night light is on (also when a hyprsunset was already running at start).
pub fn night_light() -> bool {
    let t = TINT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    t.night.is_some() || (t.gamma == 100 && hyprsunset_running())
}

/// Turns night light on at `kelvin`, or off.
pub async fn set_night_light(on: bool, kelvin: u32) {
    TINT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .night = on.then_some(kelvin);
    apply().await;
}

/// Changes night light's warmth at once (while it is on).
pub async fn set_temperature(kelvin: u32) {
    {
        let mut t = TINT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if t.night.is_none() {
            return;
        }
        t.night = Some(kelvin);
    }
    apply().await;
}

/// Software dimming, 0 to 1 (for screens without a settable backlight).
pub fn dim_level() -> f32 {
    TINT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .gamma as f32
        / 100.0
}

pub async fn set_dim(level: f32) {
    let gamma = ((level.clamp(0.0, 1.0) * 100.0).round() as u32).max(MIN_GAMMA);
    {
        let mut t = TINT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if t.gamma == gamma {
            return;
        }
        t.gamma = gamma;
    }
    apply().await;
}

pub fn caffeine() -> bool {
    INHIBIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
}

pub async fn set_caffeine(on: bool) {
    if !on {
        INHIBIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
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
        Ok(fd) => {
            *INHIBIT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(fd)
        }
        Err(e) => log::warn!("caffeine: {e}"),
    }
}
