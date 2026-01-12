//! CPU, memory and CPU temperature from /proc and hwmon. These have no change events, so they
//! are sampled every few seconds, and only reported when a shown value changes.

use futures::{SinkExt, Stream};
use std::path::PathBuf;
use std::time::Duration;

const PERIOD: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    /// Percent, rounded.
    pub cpu: u8,
    pub mem: u8,
    /// Degrees C.
    pub temp: Option<u8>,
}

fn cpu_times() -> Option<(u64, u64)> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let line = s.lines().next()?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    let idle = v.get(3)? + v.get(4).unwrap_or(&0);
    Some((v.iter().sum(), idle))
}

fn mem() -> Option<u8> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    let get = |k: &str| {
        s.lines()
            .find(|l| l.starts_with(k))?
            .split_whitespace()
            .nth(1)?
            .parse::<f64>()
            .ok()
    };
    let total = get("MemTotal:")?;
    let avail = get("MemAvailable:")?;
    Some(((1.0 - avail / total) * 100.0).round() as u8)
}

/// The CPU package sensor (coretemp "Package id 0", or k10temp "Tctl").
fn temp_file() -> Option<PathBuf> {
    for e in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let name = std::fs::read_to_string(e.path().join("name")).unwrap_or_default();
        if !matches!(name.trim(), "coretemp" | "k10temp" | "zenpower") {
            continue;
        }
        for i in 1..10 {
            let label = std::fs::read_to_string(e.path().join(format!("temp{i}_label")))
                .unwrap_or_default();
            if label.starts_with("Package")
                || label.starts_with("Tctl")
                || i == 1 && label.is_empty()
            {
                return Some(e.path().join(format!("temp{i}_input")));
            }
        }
    }
    None
}

/// The service's state stream: a snapshot on start and after every change.
pub fn stream() -> impl Stream<Item = State> {
    crate::stream::channel(1, async |mut out| {
        let temp = temp_file();
        let mut prev = cpu_times();
        let mut last = None;
        loop {
            tokio::time::sleep(PERIOD).await;
            let now = cpu_times();
            let cpu = match (prev, now) {
                (Some((t0, i0)), Some((t1, i1))) if t1 > t0 => {
                    (100.0 * (1.0 - (i1 - i0) as f64 / (t1 - t0) as f64)).round() as u8
                }
                _ => 0,
            };
            prev = now;
            let s = State {
                cpu,
                mem: mem().unwrap_or(0),
                temp: temp
                    .as_ref()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .map(|m| (m / 1000) as u8),
            };
            if last != Some(s) {
                last = Some(s);
                if out.send(s).await.is_err() {
                    return;
                }
            }
        }
    })
}
