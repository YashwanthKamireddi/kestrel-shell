//! Asking Hyprland over its socket: requests and dispatches, Lua evaluated in its config, the
//! cursor, reloading, game mode, and the keyboard's lock keys.

use super::*;

pub(crate) fn socket_dir() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    Some(PathBuf::from(runtime).join("hypr").join(sig))
}

/// `request`, for code that is not async (it blocks: call it off the UI thread).
pub fn request_blocking(cmd: &str) -> anyhow::Result<String> {
    use std::io::{Read, Write};
    let dir = socket_dir().ok_or_else(|| anyhow::anyhow!("not running under Hyprland"))?;
    let mut s = std::os::unix::net::UnixStream::connect(dir.join(".socket.sock"))?;
    s.write_all(cmd.as_bytes())?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

pub(crate) async fn request(cmd: &str) -> anyhow::Result<String> {
    let dir = socket_dir().ok_or_else(|| anyhow::anyhow!("not running under Hyprland"))?;
    let mut s = UnixStream::connect(dir.join(".socket.sock")).await?;
    s.write_all(cmd.as_bytes()).await?;
    let mut out = String::new();
    s.read_to_string(&mut out).await?;
    Ok(out)
}

/// Runs a Hyprland Lua dispatch, e.g. `hl.dsp.focus({ workspace = "3" })`.
pub async fn dispatch(lua: String) {
    match request(&format!("dispatch {lua}")).await {
        Ok(r) if r.trim() == "ok" => {}
        Ok(r) => log::warn!("hyprland dispatch {lua}: {r}"),
        Err(e) => log::warn!("hyprland dispatch {lua}: {e}"),
    }
}

/// An option's current value as Hyprland reports it (`{"int": 35, "set": true}` and so on).
pub async fn option(name: &str) -> Option<serde_json::Value> {
    let r = request(&format!("j/getoption {name}")).await.ok()?;
    serde_json::from_str(&r).ok()
}

/// Sets the cursor theme and size for Hyprland and the apps it draws for (`setcursor`).
pub async fn set_cursor(theme: &str, size: u32) {
    if let Err(e) = request(&format!("setcursor {theme} {size}")).await {
        log::warn!("hyprland setcursor: {e}");
    }
}

/// Reloads the config files.
pub async fn reload() {
    if let Err(e) = request("reload").await {
        log::warn!("hyprland reload: {e}");
    }
}

/// Runs Hyprland Lua at runtime (not saved to the config).
pub async fn eval(lua: String) {
    match request(&format!("eval {lua}")).await {
        Ok(r) if r.trim() == "ok" => {}
        Ok(r) => log::warn!("hyprland eval: {r}"),
        Err(e) => log::warn!("hyprland eval: {e}"),
    }
}

/// Game mode: animations, blur and shadows off. Turning it off reloads the config, which
/// restores whatever the user configured.
/// Game mode: window animations, blur, shadows and rounding off and the gaps cut to a sliver,
/// so a game gets the GPU and the screen. The settings it changes are saved first and put back
/// exactly when it ends (a config reload would re-apply monitors too and flash the screen).
pub async fn game_mode(on: bool) {
    let saved = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("kestrel-game-restore.lua");
    let cmd = if on {
        if !saved.is_file()
            && let Some(restore) = current_look().await
        {
            let _ = std::fs::write(&saved, &restore);
        }
        "eval hl.config({ animations = { enabled = false }, decoration = { rounding = 0, \
         blur = { enabled = false }, shadow = { enabled = false } }, \
         general = { gaps_in = 2, gaps_out = 2 } })"
            .to_string()
    } else {
        match std::fs::read_to_string(&saved) {
            Ok(restore) => {
                let _ = std::fs::remove_file(&saved);
                format!("eval {restore}")
            }
            // Nothing saved (it was turned on by an older build): the config has the values.
            Err(_) => "reload".to_string(),
        }
    };
    match request(&cmd).await {
        Ok(r) if r.trim() == "ok" => {}
        Ok(r) => log::warn!("game mode: {r}"),
        Err(e) => log::warn!("game mode: {e}"),
    }
}

/// The current values of what game mode changes, as a Lua call that restores them.
pub(super) async fn current_look() -> Option<String> {
    #[derive(Deserialize)]
    struct Opt {
        #[serde(default)]
        bool: Option<bool>,
        #[serde(default)]
        int: Option<i64>,
        #[serde(default)]
        css: Option<String>,
    }
    async fn get(key: &str) -> Option<Opt> {
        serde_json::from_str(&request(&format!("j/getoption {key}")).await.ok()?).ok()
    }
    let flag = |o: Option<Opt>| o.and_then(|o| o.bool).unwrap_or(true);
    let gap = |o: Option<Opt>, d: i64| {
        o.and_then(|o| o.css)
            .and_then(|c| c.split_whitespace().next()?.parse().ok())
            .unwrap_or(d)
    };
    let anim = flag(get("animations:enabled").await);
    let blur = flag(get("decoration:blur:enabled").await);
    let shadow = flag(get("decoration:shadow:enabled").await);
    let rounding = get("decoration:rounding").await?.int.unwrap_or(0);
    let gaps_in = gap(get("general:gaps_in").await, 5);
    let gaps_out = gap(get("general:gaps_out").await, 8);
    Some(format!(
        "hl.config({{ animations = {{ enabled = {anim} }}, decoration = {{ rounding = {rounding}, \
         blur = {{ enabled = {blur} }}, shadow = {{ enabled = {shadow} }} }}, \
         general = {{ gaps_in = {gaps_in}, gaps_out = {gaps_out} }} }})"
    ))
}

/// Caps Lock and Num Lock: on if any keyboard reports it on.
pub async fn locks() -> (bool, bool) {
    #[derive(Deserialize)]
    struct Kb {
        #[serde(rename = "capsLock", default)]
        caps: bool,
        #[serde(rename = "numLock", default)]
        num: bool,
    }
    #[derive(Deserialize)]
    struct Devices {
        keyboards: Vec<Kb>,
    }
    let Ok(raw) = request("j/devices").await else {
        return (false, false);
    };
    let Ok(d) = serde_json::from_str::<Devices>(&raw) else {
        return (false, false);
    };
    (
        d.keyboards.iter().any(|k| k.caps),
        d.keyboards.iter().any(|k| k.num),
    )
}
