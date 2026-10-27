//! This Hyprland session's processes. Another session's, or a nested test compositor's, are
//! not this shell's to show or touch: a recorder, hypridle or hyprsunset is matched by name and
//! by the session it runs in (its `HYPRLAND_INSTANCE_SIGNATURE`). Before, the recorder check
//! matched by name only, so a recording in one session showed a recording timer in every
//! kestrel on the machine.

use std::path::PathBuf;

/// This session's running processes named `comm`: each one's pid and /proc directory.
pub fn processes(comm: &str) -> Vec<(u32, PathBuf)> {
    let ours = format!(
        "HYPRLAND_INSTANCE_SIGNATURE={}",
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default()
    );
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let pid = e.file_name().to_str()?.parse::<u32>().ok()?;
            let named =
                std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == comm);
            if !named {
                return None;
            }
            let env = std::fs::read(e.path().join("environ")).ok()?;
            env.split(|b| *b == 0)
                .any(|kv| kv == ours.as_bytes())
                .then(|| (pid, e.path()))
        })
        .collect()
}
