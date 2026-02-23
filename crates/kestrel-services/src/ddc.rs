//! Real brightness for external monitors over DDC/CI (ddcutil, i2c). Only monitors that answer
//! are used; one with DDC/CI switched off in its own menu reports "Invalid display" and the
//! shell dims the picture instead (`toggles::set_dim`). Setting is slow (~100 ms per write), so
//! while a slider moves only the newest value is sent once the previous write finishes.

use std::sync::Mutex;

/// A monitor that answers: its connector ("HDMI-A-1") and i2c bus number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub connector: String,
    pub bus: u32,
}

async fn ddcutil(args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("ddcutil")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The monitors that answer DDC/CI.
pub async fn displays() -> Vec<Display> {
    let Some(text) = ddcutil(&["detect", "--terse"]).await else {
        return Vec::new();
    };
    parse_detect(&text)
}

fn parse_detect(text: &str) -> Vec<Display> {
    text.split("\n\n")
        .filter(|block| block.trim_start().starts_with("Display "))
        .filter_map(|block| {
            let field = |k: &str| {
                block
                    .lines()
                    .find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()))
            };
            let bus = field("I2C bus:")?.rsplit('-').next()?.parse().ok()?;
            let connector = field("DRM connector:")?;
            // "card1-HDMI-A-1" → "HDMI-A-1"
            let connector = connector
                .split_once('-')
                .map_or(connector.clone(), |(_, c)| c.to_string());
            Some(Display { connector, bus })
        })
        .collect()
}

/// The monitor's brightness, 0 to 1.
pub async fn get(bus: u32) -> Option<f32> {
    let b = bus.to_string();
    let text = ddcutil(&["--bus", &b, "getvcp", "10", "--brief"]).await?;
    // "VCP 10 C 50 100"
    let w: Vec<&str> = text.split_whitespace().collect();
    let (cur, max): (f32, f32) = (w.get(3)?.parse().ok()?, w.get(4)?.parse().ok()?);
    (max > 0.0).then(|| cur / max)
}

/// The newest brightness asked for per bus, and whether a write is under way.
static PENDING: Mutex<Vec<(u32, u32, bool)>> = Mutex::new(Vec::new());

/// Sets the monitor's brightness (0 to 1); a write already under way sends the newest value
/// when it is done.
pub async fn set(bus: u32, level: f32) {
    let value = (level.clamp(0.0, 1.0) * 100.0).round() as u32;
    {
        let mut p = PENDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match p.iter_mut().find(|e| e.0 == bus) {
            Some(e) => {
                e.1 = value;
                if e.2 {
                    return; // The running write picks it up.
                }
                e.2 = true;
            }
            None => p.push((bus, value, true)),
        }
    }
    let mut sent = None;
    loop {
        let want = {
            let mut p = PENDING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(e) = p.iter_mut().find(|e| e.0 == bus) else {
                return;
            };
            if Some(e.1) == sent {
                e.2 = false;
                return;
            }
            e.1
        };
        let (b, v) = (bus.to_string(), want.to_string());
        let _ = ddcutil(&["--bus", &b, "--noverify", "setvcp", "10", &v]).await;
        sent = Some(want);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_answering_monitors_are_used() {
        let text = "Display 1\n   I2C bus:  /dev/i2c-5\n   DRM connector:    card1-DP-2\n   Monitor:  X\n\nInvalid display\n   I2C bus:          /dev/i2c-3\n   DRM connector:    card1-HDMI-A-1\n";
        let d = super::parse_detect(text);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].connector, "DP-2");
        assert_eq!(d[0].bus, 5);
    }
}
