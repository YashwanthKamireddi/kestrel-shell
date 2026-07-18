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

/// What the system monitor shows: each core's load, memory and swap, every process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detail {
    /// Percent per core.
    pub cores: Vec<u8>,
    /// Bytes: memory used and total, swap used and total.
    pub mem: (u64, u64),
    pub swap: (u64, u64),
    /// Every process, the busiest first.
    pub procs: Vec<Proc>,
    /// The CPU package's temperature, °C (coretemp / k10temp), when the kernel reports one.
    pub cpu_temp: Option<f32>,
    /// The graphics clock now and at most, MHz (Intel's i915 / xe report clocks, not a load).
    pub gpu_mhz: Option<(u32, u32)>,
    /// Bytes per second read from and written to the disks.
    pub disk_io: (u64, u64),
    /// Bytes per second received and sent on the network (every interface but loopback).
    pub net_io: (u64, u64),
}

/// The CPU package temperature from hwmon (Intel coretemp "Package id 0", AMD k10temp
/// "Tctl").
fn cpu_temp() -> Option<f32> {
    for e in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let dir = e.path();
        let name = std::fs::read_to_string(dir.join("name")).unwrap_or_default();
        if !matches!(name.trim(), "coretemp" | "k10temp" | "zenpower") {
            continue;
        }
        for i in 1..=16 {
            let label =
                std::fs::read_to_string(dir.join(format!("temp{i}_label"))).unwrap_or_default();
            let label = label.trim();
            if label.starts_with("Package") || label == "Tctl" || label == "Tdie" {
                return std::fs::read_to_string(dir.join(format!("temp{i}_input")))
                    .ok()
                    .and_then(|v| v.trim().parse::<f32>().ok())
                    .map(|m| m / 1000.0);
            }
        }
    }
    None
}

/// The first graphics card's clock now and at most (MHz).
fn gpu_mhz() -> Option<(u32, u32)> {
    for e in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let dir = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let read = |f: &str| {
            std::fs::read_to_string(dir.join(f))
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
        };
        if let (Some(now), Some(max)) = (read("gt_act_freq_mhz"), read("gt_max_freq_mhz")) {
            return Some((now, max));
        }
    }
    None
}

/// Bytes read and written so far on whole disks (not partitions, not zram or loop devices).
fn disk_bytes() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/diskstats").unwrap_or_default();
    let mut r = 0;
    let mut w = 0;
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let Some(name) = f.get(2) else { continue };
        let whole = std::path::Path::new(&format!("/sys/block/{name}/device")).exists();
        if !whole {
            continue;
        }
        let num = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        // Sectors of 512 bytes: read is field 5, written field 9.
        r += num(5) * 512;
        w += num(9) * 512;
    }
    (r, w)
}

/// Bytes received and sent so far on every interface but loopback.
fn net_bytes() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
    let mut rx = 0;
    let mut tx = 0;
    for l in text.lines().skip(2) {
        let Some((name, rest)) = l.split_once(':') else {
            continue;
        };
        if name.trim() == "lo" {
            continue;
        }
        let f: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        rx += f.first().copied().unwrap_or(0);
        tx += f.get(8).copied().unwrap_or(0);
    }
    (rx, tx)
}

/// One process as the monitor lists it (DMS's process list columns).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// The user's own (it can be ended, paused, reprioritised without asking).
    pub mine: bool,
    pub name: String,
    /// Its full command line.
    pub command: String,
    /// CPU percent of one core over the last step.
    pub cpu: f32,
    /// Resident memory, bytes.
    pub rss: u64,
    /// Stopped (paused with SIGSTOP).
    pub paused: bool,
    /// Its nice value (-20 highest .. 19 lowest).
    pub nice: i32,
}

/// What `processes` reads of each one.
struct Raw {
    ppid: u32,
    mine: bool,
    name: String,
    ticks: u64,
    rss: u64,
    paused: bool,
    nice: i32,
}

/// Per-core (total, idle) jiffies.
fn core_times() -> Vec<(u64, u64)> {
    std::fs::read_to_string("/proc/stat")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .map(|l| {
            let v: Vec<u64> = l
                .split_whitespace()
                .skip(1)
                .filter_map(|x| x.parse().ok())
                .collect();
            (
                v.iter().sum(),
                v.get(3).unwrap_or(&0) + v.get(4).unwrap_or(&0),
            )
        })
        .collect()
}

/// Every process: pid -> what the list needs (its CPU jiffies so far, to work out its load).
fn processes() -> std::collections::HashMap<u32, Raw> {
    let uid = unsafe { libc::getuid() };
    let page = 4096u64;
    let mut out = std::collections::HashMap::new();
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return out;
    };
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        use std::os::unix::fs::MetadataExt;
        let mine = e.metadata().map(|m| m.uid()).ok() == Some(uid);
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // The name is in parentheses and may hold spaces: split after the last ')'.
        let (Some(a), Some(b)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let name = stat[a + 1..b].to_string();
        let f: Vec<&str> = stat[b + 2..].split_whitespace().collect();
        let num = |i: usize| f.get(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        // Fields after the name: state(0) ppid(1) ... utime(11) stime(12) ... nice(16) ...
        // rss(21).
        out.insert(
            pid,
            Raw {
                ppid: num(1) as u32,
                mine,
                name,
                ticks: (num(11) + num(12)) as u64,
                rss: num(21) as u64 * page,
                paused: matches!(f.first(), Some(&"T" | &"t")),
                nice: num(16) as i32,
            },
        );
    }
    out
}

/// The kernel keeps 15 characters of a name ("xdg-desktop-por"): cut there, the program's own
/// file name from its command line instead, when it starts the same.
fn full_name(comm: &str, command: &str) -> String {
    if comm.len() < 15 {
        return comm.to_string();
    }
    command
        .split(' ')
        .next()
        .and_then(|a| a.rsplit('/').next())
        .filter(|base| base.starts_with(comm))
        .map_or_else(|| comm.to_string(), str::to_string)
}

/// A process's command line ("" for kernel threads).
fn command_of(pid: u32) -> String {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| {
            b.split(|c| *c == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn meminfo() -> ((u64, u64), (u64, u64)) {
    let s = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let get = |k: &str| -> u64 {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    };
    let (mt, ma) = (get("MemTotal:"), get("MemAvailable:"));
    let (st, sf) = (get("SwapTotal:"), get("SwapFree:"));
    ((mt.saturating_sub(ma), mt), (st.saturating_sub(sf), st))
}

/// The monitor's readings every 1.5 s while polled (the system monitor is open).
pub fn detail() -> impl Stream<Item = Detail> {
    crate::stream::channel(1, async |mut out| {
        const STEP: Duration = Duration::from_millis(1500);
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f32;
        let mut cores = core_times();
        let mut procs = processes();
        let mut disk = disk_bytes();
        let mut net = net_bytes();
        // Command lines by pid: read once (a process's command does not change).
        let mut commands: std::collections::HashMap<u32, String> = Default::default();
        loop {
            tokio::time::sleep(STEP).await;
            let (c2, p2) = tokio::task::spawn_blocking(|| (core_times(), processes()))
                .await
                .unwrap_or_default();
            let load: Vec<u8> = cores
                .iter()
                .zip(&c2)
                .map(|((t0, i0), (t1, i1))| {
                    let dt = t1.saturating_sub(*t0).max(1) as f32;
                    ((1.0 - i1.saturating_sub(*i0) as f32 / dt) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8
                })
                .collect();
            let secs = STEP.as_secs_f32();
            let mut list: Vec<Proc> = p2
                .iter()
                .map(|(pid, r)| {
                    let before = procs.get(pid).map_or(r.ticks, |p| p.ticks);
                    let command = commands
                        .entry(*pid)
                        .or_insert_with(|| command_of(*pid))
                        .clone();
                    Proc {
                        pid: *pid,
                        ppid: r.ppid,
                        mine: r.mine,
                        name: full_name(&r.name, &command),
                        command,
                        cpu: r.ticks.saturating_sub(before) as f32 / hz / secs * 100.0,
                        rss: r.rss,
                        paused: r.paused,
                        nice: r.nice,
                    }
                })
                .collect();
            list.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.rss.cmp(&a.rss)));
            commands.retain(|pid, _| p2.contains_key(pid));
            let (mem, swap) = meminfo();
            let (disk2, net2) = (disk_bytes(), net_bytes());
            let per_sec = |a: u64, b: u64| (b.saturating_sub(a) as f32 / secs) as u64;
            let disk_io = (per_sec(disk.0, disk2.0), per_sec(disk.1, disk2.1));
            let net_io = (per_sec(net.0, net2.0), per_sec(net.1, net2.1));
            disk = disk2;
            net = net2;
            cores = c2;
            procs = p2;
            if out
                .send(Detail {
                    cores: load,
                    mem,
                    swap,
                    procs: list,
                    cpu_temp: cpu_temp(),
                    gpu_mhz: gpu_mhz(),
                    disk_io,
                    net_io,
                })
                .await
                .is_err()
            {
                return;
            }
        }
    })
}
