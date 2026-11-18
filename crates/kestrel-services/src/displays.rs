//! Displays: each screen's mode (resolution and refresh), scale, rotation, place and whether
//! it is on, read from Hyprland, changed live (`hl.monitor`) and kept in
//! ~/.config/hypr/hyprmon.lua, the layout file HyprMon writes too (same format), which
//! hyprland.lua loads after conf/monitors.lua (whose failover stays as it is).

use serde::Deserialize;

/// One screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    /// Connector ("HDMI-A-1", "eDP-1").
    pub name: String,
    /// What people call it ("DELL E2216HV", "Built-in display").
    pub label: String,
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
    /// Hz.
    pub refresh: f32,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    /// 0 normal, 1 90°, 2 180°, 3 270°.
    pub transform: u8,
    /// Modes it offers: (width, height, Hz), best first.
    pub modes: Vec<(u32, u32, f32)>,
    /// Shows the same picture as this screen (its connector) instead of its own space.
    pub mirror: Option<String>,
}

impl Display {
    /// Its sizes, each once, best first.
    pub fn sizes(&self) -> Vec<(u32, u32)> {
        let mut v: Vec<(u32, u32)> = Vec::new();
        for (w, h, _) in &self.modes {
            if !v.contains(&(*w, *h)) {
                v.push((*w, *h));
            }
        }
        v
    }

    /// The refresh rates it offers at a size, highest first.
    pub fn rates(&self, w: u32, h: u32) -> Vec<f32> {
        let mut v: Vec<f32> = self
            .modes
            .iter()
            .filter(|m| m.0 == w && m.1 == h)
            .map(|m| m.2)
            .collect();
        v.sort_by(|a, b| b.total_cmp(a));
        v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        v
    }
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    id: i64,
    name: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default, rename = "refreshRate")]
    refresh_rate: f32,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    transform: u8,
    #[serde(default)]
    disabled: bool,
    #[serde(default, rename = "availableModes")]
    available_modes: Vec<String>,
    #[serde(default, rename = "mirrorOf")]
    mirror_of: String,
}

fn one() -> f32 {
    1.0
}

/// "1920x1080@60.00Hz" as (1920, 1080, 60.0).
fn mode(s: &str) -> Option<(u32, u32, f32)> {
    let (size, hz) = s.trim_end_matches("Hz").split_once('@')?;
    let (w, h) = size.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?, hz.parse().ok()?))
}

/// Every screen, on or off.
pub async fn read() -> Vec<Display> {
    let Ok(text) = crate::hyprland::request("j/monitors all").await else {
        return Vec::new();
    };
    let raw: Vec<Raw> = serde_json::from_str(&text).unwrap_or_default();
    // Hyprland names the mirrored screen by its id; kept by connector (ids change).
    let names: Vec<(i64, String)> = raw.iter().map(|r| (r.id, r.name.clone())).collect();
    let mirror_name = |of: &str| -> Option<String> {
        if of.is_empty() || of == "none" {
            return None;
        }
        match of.parse::<i64>() {
            Ok(id) => names.iter().find(|(i, _)| *i == id).map(|(_, n)| n.clone()),
            Err(_) => Some(of.to_string()),
        }
    };
    raw.into_iter()
        .map(|r| {
            let mut modes: Vec<(u32, u32, f32)> =
                r.available_modes.iter().filter_map(|m| mode(m)).collect();
            modes.dedup();
            // An off screen reports 0x0: its best mode stands in.
            let (width, height, refresh) = if r.width == 0 {
                modes.first().copied().unwrap_or((1920, 1080, 60.0))
            } else {
                (r.width, r.height, r.refresh_rate)
            };
            Display {
                label: if r.name.starts_with("eDP") || r.name.starts_with("LVDS") {
                    "Built-in display".into()
                } else if r.model.is_empty() {
                    r.name.clone()
                } else {
                    r.model.clone()
                },
                name: r.name,
                enabled: !r.disabled,
                width,
                height,
                refresh,
                x: r.x.max(0),
                y: r.y.max(0),
                scale: r.scale,
                transform: r.transform,
                modes,
                mirror: mirror_name(&r.mirror_of),
            }
        })
        .collect()
}

/// The `hl.monitor` call for a screen as it should be.
pub fn line(d: &Display) -> String {
    if !d.enabled {
        return format!("hl.monitor({{ output = \"{}\", disabled = true }})", d.name);
    }
    let mut s = format!(
        "hl.monitor({{ output = \"{}\", mode = \"{}x{}@{:.2}\", position = \"{}x{}\", scale = {:.2}",
        d.name, d.width, d.height, d.refresh, d.x, d.y, d.scale
    );
    if d.transform != 0 {
        s += &format!(", transform = {}", d.transform);
    }
    if let Some(m) = &d.mirror {
        s += &format!(", mirror = \"{m}\"");
    }
    s += " })";
    s
}

/// The line applied live: a screen leaving a mirror needs `mirror = ""` said (a rule without
/// it keeps the running mirror; checked 2026-11-14). The saved file has no need: at start an
/// absent mirror is none.
fn live_line(d: &Display) -> String {
    let l = line(d);
    if d.enabled && d.mirror.is_none() {
        format!("{}, mirror = \"\" }})", l.trim_end_matches(" })"))
    } else {
        l
    }
}

/// Applies every screen's line at once (live, not saved). Screens being turned on go first, so
/// there is never a moment with none.
pub async fn apply(all: &[Display]) {
    let mut order: Vec<&Display> = all.iter().collect();
    order.sort_by_key(|d| !d.enabled);
    for d in order {
        crate::hyprland::eval(live_line(d)).await;
    }
}

/// Keeps the layout in hyprmon.lua (HyprMon's file and format).
pub fn save(all: &[Display]) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or("no home folder")?;
    let path = std::path::Path::new(&home).join(".config/hypr/hyprmon.lua");
    let mut text = String::from(
        "-- Generated by HyprMon or kestrel (Settings > Displays). Manual changes may be overwritten.\n\n",
    );
    for d in all {
        text += &line(d);
        text.push('\n');
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdmi() -> Display {
        Display {
            name: "HDMI-A-1".into(),
            label: "DELL".into(),
            enabled: true,
            width: 1920,
            height: 1080,
            refresh: 60.0,
            x: 0,
            y: 14,
            scale: 1.0,
            transform: 0,
            modes: vec![],
            mirror: None,
        }
    }

    #[test]
    fn writes_hyprmon_lines() {
        assert_eq!(
            line(&hdmi()),
            "hl.monitor({ output = \"HDMI-A-1\", mode = \"1920x1080@60.00\", position = \"0x14\", scale = 1.00 })"
        );
        let off = Display {
            enabled: false,
            name: "eDP-1".into(),
            ..hdmi()
        };
        assert_eq!(
            line(&off),
            "hl.monitor({ output = \"eDP-1\", disabled = true })"
        );
    }

    #[test]
    fn writes_a_mirror() {
        let m = Display {
            name: "eDP-1".into(),
            mirror: Some("HDMI-A-1".into()),
            ..hdmi()
        };
        assert!(line(&m).ends_with(", mirror = \"HDMI-A-1\" })"));
    }

    #[test]
    fn leaves_a_mirror_live() {
        assert!(live_line(&hdmi()).ends_with("scale = 1.00, mirror = \"\" })"));
    }

    #[test]
    fn sizes_and_rates() {
        let d = Display {
            modes: vec![
                (1920, 1080, 60.0),
                (1920, 1080, 48.0),
                (1280, 1024, 75.03),
                (1280, 1024, 60.02),
                (1920, 1080, 59.94),
            ],
            ..hdmi()
        };
        assert_eq!(d.sizes(), vec![(1920, 1080), (1280, 1024)]);
        assert_eq!(d.rates(1920, 1080), vec![60.0, 48.0]);
    }

    #[test]
    fn reads_modes() {
        assert_eq!(mode("1280x1024@75.03Hz"), Some((1280, 1024, 75.03)));
    }
}
