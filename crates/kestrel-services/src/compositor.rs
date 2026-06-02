//! What the shell asks of the compositor, in the shell's own words: focus a workspace or a
//! window, step through workspaces, start a program. The compositor in use carries it out;
//! Hyprland today (its IPC), and the place a Niri or Sway backend goes. Programs are started by
//! the compositor (or, without one kestrel knows, in their own systemd scope) so they never
//! belong to kestrel's service and survive its restarts.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    FocusWorkspace(i32),
    /// A window by its compositor address.
    FocusWindow(String),
    /// A command line, run by the user's shell.
    Exec(String),
    /// A key with modifiers ("CTRL SHIFT", "V") sent straight to the focused window by the
    /// compositor (no virtual keyboard: wtype's own keymap put "v" on the keycode that is
    /// Escape in the real layout, so Ctrl+Shift+V reached Hyprland as Ctrl+Shift+Escape).
    Shortcut(String, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hyprland,
    Other,
}

/// The compositor kestrel runs under.
pub fn detect() -> Kind {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        Kind::Hyprland
    } else {
        Kind::Other
    }
}

/// Carries out `a`.
pub async fn run(a: Action) {
    match detect() {
        Kind::Hyprland => super::hyprland::dispatch(hyprland_lua(&a)).await,
        Kind::Other => {
            if let Action::Exec(cmd) = a {
                let _ = tokio::process::Command::new("systemd-run")
                    .args(["--user", "--scope", "--quiet", "--", "sh", "-c", &cmd])
                    .spawn();
            } else {
                log::warn!("compositor: {a:?} is not supported here");
            }
        }
    }
}

/// The action as a Hyprland Lua dispatcher.
fn hyprland_lua(a: &Action) -> String {
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    match a {
        Action::FocusWorkspace(id) => format!("hl.dsp.focus({{ workspace = \"{id}\" }})"),
        Action::FocusWindow(addr) => {
            format!("hl.dsp.focus({{ window = \"address:{}\" }})", quote(addr))
        }
        Action::Exec(cmd) => format!("hl.dsp.exec_cmd(\"{}\")", quote(cmd)),
        Action::Shortcut(mods, key) => format!(
            "hl.dsp.send_shortcut({{ mods = \"{}\", key = \"{}\" }})",
            quote(mods),
            quote(key)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, hyprland_lua};

    #[test]
    fn hyprland_dispatchers() {
        assert_eq!(
            hyprland_lua(&Action::FocusWorkspace(3)),
            "hl.dsp.focus({ workspace = \"3\" })"
        );
        assert_eq!(
            hyprland_lua(&Action::Exec("kitty --title \"x\"".into())),
            "hl.dsp.exec_cmd(\"kitty --title \\\"x\\\"\")"
        );
    }
}
