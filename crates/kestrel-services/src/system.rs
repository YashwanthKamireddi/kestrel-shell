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

/// What the system monitor shows: each core's load, memory and swap, the busiest processes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detail {
    /// Percent per core.
    pub cores: Vec<u8>,
    /// Bytes: memory used and total, swap used and total.
    pub mem: (u64, u64),
    pub swap: (u64, u64),
    /// The busiest processes, everyone's (the system's too).
    pub top: Vec<Proc>,
    /// CPU no process seen in both readings accounts for (percent of one core): the kernel's
    /// own work and programs that started and ended between readings (a screenshot's
    /// helpers, a script's commands). With it the list adds up to the cores' load.
    pub other: f32,
}

/// A process as the monitor shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    /// Percent of one core.
    pub cpu: f32,
    /// Resident memory, bytes.
    pub rss: u64,
    /// The user's own (only those can be ended from here).
    pub own: bool,
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

/// Each process, everyone's: pid -> (name, CPU jiffies so far, resident bytes, the user's own).
fn processes() -> std::collections::HashMap<u32, (String, u64, u64, bool)> {
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
        let own = e.metadata().map(|m| m.uid()).ok() == Some(uid);
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // The name is in parentheses and may hold spaces: split after the last ')'.
        let (Some(a), Some(b)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let name = stat[a + 1..b].to_string();
        let f: Vec<&str> = stat[b + 2..].split_whitespace().collect();
        // Fields after the name: state(0) ... utime(11) stime(12) ... rss(21).
        let ticks = f.get(11).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)
            + f.get(12).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let rss = f.get(21).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) * page;
        out.insert(pid, (readable(pid, name), ticks, rss, own));
    }
    out
}

/// A name people know: the kernel cuts names at 15 characters ("xdg-desktop-por") and some
/// apps name their main thread ("MainThread"); then the program's file from its command line.
fn readable(pid: u32, comm: String) -> String {
    if comm.len() < 15 && comm != "MainThread" {
        return comm;
    }
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .and_then(|b| {
            let first = b.split(|c| *c == 0).next()?.to_vec();
            let path = String::from_utf8_lossy(&first).into_owned();
            let base = path.rsplit('/').next()?.to_string();
            (!base.is_empty()).then_some(base)
        })
        .unwrap_or(comm)
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

type Cores = Vec<(u64, u64)>;
type Procs = std::collections::HashMap<u32, (String, u64, u64, bool)>;

/// What happened between two readings `secs` apart: each core's load, the busiest processes,
/// what the cores did that no process seen twice accounts for.
fn between(cores: &Cores, procs: &Procs, c2: &Cores, p2: &Procs, secs: f32) -> Detail {
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f32;
    let load: Vec<u8> = cores
        .iter()
        .zip(c2)
        .map(|((t0, i0), (t1, i1))| {
            // No ticks between the readings (an offline core, a counter that went back): no
            // load. It read as 100 % (no time and no idle time came out as fully busy).
            let dt = t1.saturating_sub(*t0);
            if dt == 0 {
                return 0;
            }
            let dt = dt as f32;
            ((1.0 - i1.saturating_sub(*i0) as f32 / dt) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        })
        .collect();
    let mut top: Vec<Proc> = p2
        .iter()
        .map(|(pid, (name, ticks, rss, own))| {
            let before = procs.get(pid).map_or(*ticks, |p| p.1);
            let cpu = ticks.saturating_sub(before) as f32 / hz / secs * 100.0;
            Proc {
                pid: *pid,
                name: name.clone(),
                cpu,
                rss: *rss,
                own: *own,
            }
        })
        .collect();
    let busy: f32 = load.iter().map(|&l| f32::from(l)).sum();
    let seen: f32 = top.iter().map(|p| p.cpu).sum();
    let other = (busy - seen).max(0.0);
    top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.rss.cmp(&a.rss)));
    top.truncate(10);
    let (mem, swap) = meminfo();
    Detail {
        cores: load,
        mem,
        swap,
        top,
        other,
    }
}

/// How far apart the first two readings are: short, so the monitor opens complete at once
/// (it waited 1.5 s on "Reading…"); one clock tick is 10 ms, so a process's share is already
/// within a few percent.
const FIRST: Duration = Duration::from_millis(100);
/// Then a reading every so often while it is open.
const STEP: Duration = Duration::from_millis(1500);

/// A reading now, over the last `FIRST`: what the monitor opens with.
pub async fn first_detail() -> Detail {
    let (c, p) = tokio::task::spawn_blocking(|| (core_times(), processes()))
        .await
        .unwrap_or_default();
    tokio::time::sleep(FIRST).await;
    tokio::task::spawn_blocking(move || {
        let (c2, p2) = (core_times(), processes());
        between(&c, &p, &c2, &p2, FIRST.as_secs_f32())
    })
    .await
    .unwrap_or_default()
}

/// Readings every `STEP` while polled.
pub fn detail() -> impl Stream<Item = Detail> {
    crate::stream::channel(1, async |mut out| {
        let (mut cores, mut procs) = tokio::task::spawn_blocking(|| (core_times(), processes()))
            .await
            .unwrap_or_default();
        loop {
            tokio::time::sleep(STEP).await;
            let (c2, p2) = tokio::task::spawn_blocking(|| (core_times(), processes()))
                .await
                .unwrap_or_default();
            let d = between(&cores, &procs, &c2, &p2, STEP.as_secs_f32());
            cores = c2;
            procs = p2;
            if out.send(d).await.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hz() -> u64 {
        unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64
    }

    #[test]
    fn core_load_is_the_share_not_idle() {
        // (total, idle) ticks per core, before and after.
        let before = vec![(1000, 800), (1000, 1000), (5, 5)];
        let after = vec![(1100, 825), (1100, 1100), (5, 5)];
        let d = between(&before, &Procs::new(), &after, &Procs::new(), 1.0);
        assert_eq!(
            d.cores,
            [75, 0, 0],
            "a core with no ticks between is idle, not NaN"
        );
    }

    #[test]
    fn a_counter_that_went_backwards_reads_as_nothing() {
        let d = between(
            &vec![(1000, 900)],
            &Procs::new(),
            &vec![(900, 950)],
            &Procs::new(),
            1.0,
        );
        assert_eq!(
            d.cores,
            [0],
            "no time between the readings: no load, never wrapped round"
        );
    }

    #[test]
    fn busiest_processes_first_and_what_none_explains() {
        let hz = hz();
        let p = |name: &str, ticks: u64, rss: u64| (name.to_string(), ticks, rss, true);
        let before: Procs = [(1, p("a", 0, 1)), (2, p("b", 0, 5)), (3, p("c", 0, 9))].into();
        // In one second: a used half a core, b a whole one, c nothing; d is new.
        let after: Procs = [
            (1, p("a", hz / 2, 1)),
            (2, p("b", hz, 5)),
            (3, p("c", 0, 9)),
            (4, p("d", 3 * hz, 2)),
        ]
        .into();
        let cores = vec![(0, 0), (0, 0)];
        let cores2 = vec![(100, 0), (100, 0)];
        let d = between(&cores, &before, &cores2, &after, 1.0);
        let names: Vec<&str> = d.top.iter().map(|p| p.name.as_str()).collect();
        // A process not seen before counts nothing yet; equal shares go by memory.
        assert_eq!(names, ["b", "a", "c", "d"]);
        assert!((d.top[0].cpu - 100.0).abs() < 0.01);
        assert!((d.top[1].cpu - 50.0).abs() < 0.01);
        // Two busy cores (200 %), 150 % accounted for.
        assert!((d.other - 50.0).abs() < 0.01, "{}", d.other);
    }

    #[test]
    fn at_most_ten_processes() {
        let before: Procs = (0..30)
            .map(|i| (i, ("p".to_string(), 0, 0, false)))
            .collect();
        let after: Procs = (0..30)
            .map(|i| (i, ("p".to_string(), u64::from(i), 0, false)))
            .collect();
        let d = between(&vec![], &before, &vec![], &after, 2.0);
        assert_eq!(d.top.len(), 10);
        assert_eq!(d.top[0].pid, 29);
        assert_eq!(d.other, 0.0, "never below nothing");
    }
}
