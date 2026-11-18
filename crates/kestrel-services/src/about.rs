//! About this computer: the machine, the system and what it is made of, read from the kernel's
//! files and a few standard tools (for Settings' About page). Blocking: read it off the UI
//! thread.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {
    /// The maker and model ("LENOVO ThinkPad X1 Carbon").
    pub device: String,
    pub hostname: String,
    /// "Arch Linux".
    pub os: String,
    pub kernel: String,
    /// "AMD Ryzen 7 7840U · 16 threads".
    pub processor: String,
    /// "16 GB".
    pub memory: String,
    pub graphics: Vec<String>,
    /// "131 GB free of 349 GB".
    pub disk: String,
    pub compositor: String,
    /// "3 hours, 12 minutes".
    pub uptime: String,
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The display controllers (PCI class 0x03), named as lspci names them: from sysfs's ids and
/// the PCI id database (`hwdata/pci.ids`), "Vendor Device".
fn graphics() -> Vec<String> {
    let ids = std::fs::read_to_string("/usr/share/hwdata/pci.ids").unwrap_or_default();
    let Ok(rd) = std::fs::read_dir("/sys/bus/pci/devices") else {
        return Vec::new();
    };
    let mut devs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    devs.sort();
    devs.iter()
        .filter(|d| read(&d.join("class").to_string_lossy()).starts_with("0x03"))
        .filter_map(|d| {
            let hex = |f: &str| {
                read(&d.join(f).to_string_lossy())
                    .trim_start_matches("0x")
                    .to_lowercase()
            };
            pci_name(&ids, &hex("vendor"), &hex("device"))
        })
        .map(|n| gpu_name(&n))
        .collect()
}

/// "Intel Corporation Alder Lake-UP3 GT2 [Iris Xe Graphics]" for 8086:46a8, from pci.ids
/// (vendors at the line start, their devices indented by one tab).
fn pci_name(ids: &str, vendor: &str, device: &str) -> Option<String> {
    let mut vname = None;
    for l in ids.lines() {
        if l.starts_with('#') || l.is_empty() {
            continue;
        }
        if !l.starts_with('\t') {
            if vname.is_some() {
                break;
            }
            if let Some(rest) = l.strip_prefix(vendor).filter(|r| r.starts_with("  ")) {
                vname = Some(rest.trim().to_string());
            }
            continue;
        }
        if let (Some(v), Some(rest)) = (&vname, l.strip_prefix('\t')) {
            if rest.starts_with('\t') {
                continue;
            }
            if let Some(name) = rest.strip_prefix(device).filter(|r| r.starts_with("  ")) {
                return Some(format!("{v} {}", name.trim()));
            }
        }
    }
    vname
}

/// Sizes the way people say them: 16 GB, 349 GB, 1.8 TB.
fn size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{gb:.0} GB")
    }
}

/// A graphics chip as people know it: "Intel Corporation Alder Lake-UP3 GT2 [Iris Xe Graphics]
/// (rev 0c)" is "Intel Iris Xe Graphics".
fn gpu_name(pci: &str) -> String {
    let pci = pci.split(" (rev").next().unwrap_or(pci);
    let vendor = if pci.starts_with("Intel") {
        "Intel"
    } else if pci.starts_with("NVIDIA") {
        "NVIDIA"
    } else if pci.starts_with("Advanced Micro Devices") || pci.contains("AMD") {
        "AMD"
    } else {
        pci.split_whitespace().next().unwrap_or("")
    };
    match (pci.rfind('['), pci.rfind(']')) {
        (Some(a), Some(b)) if b > a => {
            // The last bracket is the product: AMD's read "[AMD/ATI] Navi 23 [Radeon RX 6600]".
            let name = pci[a + 1..b].trim();
            if name.starts_with(vendor) {
                name.to_string()
            } else {
                format!("{vendor} {name}")
            }
        }
        _ => pci.to_string(),
    }
}

pub fn read_all() -> About {
    let vendor = read("/sys/class/dmi/id/sys_vendor");
    let product = read("/sys/class/dmi/id/product_name");
    let os = read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".into());
    let cpuinfo = read("/proc/cpuinfo");
    let model = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().replace("(R)", "").replace("(TM)", ""))
        .unwrap_or_default();
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    let mem_kb: u64 = read("/proc/meminfo")
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    // In decimal gigabytes, like the disk (and the box it came in): 16044268 kB is 16 GB.
    let mem_gb = (mem_kb as f64 * 1024.0 / 1e9).round();
    let graphics = graphics();
    let disk = {
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // Safety: a valid NUL-terminated path and a zeroed struct the call fills.
        if unsafe { libc::statvfs(c"/".as_ptr(), &mut st) } == 0 {
            let total = st.f_blocks * st.f_frsize;
            let free = st.f_bavail * st.f_frsize;
            format!("{} free of {}", size(free), size(total))
        } else {
            String::new()
        }
    };
    let compositor = crate::hyprland::request_blocking("j/version")
        .ok()
        .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        .and_then(|v| Some(format!("Hyprland {}", v.get("version")?.as_str()?)))
        .unwrap_or_default();
    let up: f64 = read("/proc/uptime")
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let (d, h, m) = (
        (up / 86400.0) as u64,
        (up % 86400.0 / 3600.0) as u64,
        (up % 3600.0 / 60.0) as u64,
    );
    let part = |n: u64, one: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {one}s")),
    };
    let uptime = [part(d, "day"), part(h, "hour"), part(m, "minute")]
        .into_iter()
        .flatten()
        .take(2)
        .collect::<Vec<_>>()
        .join(", ");
    About {
        device: format!("{vendor} {product}").trim().to_string(),
        hostname: read("/proc/sys/kernel/hostname"),
        os,
        kernel: read("/proc/sys/kernel/osrelease"),
        processor: if threads > 0 {
            format!("{model} · {threads} threads")
        } else {
            model
        },
        memory: format!("{mem_gb:.0} GB"),
        graphics,
        disk,
        compositor,
        uptime: if uptime.is_empty() {
            "Just started".into()
        } else {
            uptime
        },
    }
}

/// Renames the computer (systemd-hostnamed: the static and the pretty name). polkit may ask.
pub async fn rename(name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    // A host name: letters, digits and hyphens, not at the ends.
    let host: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if host.is_empty() || host.len() > 63 {
        return Err("Use letters and numbers".into());
    }
    let c = zbus::Connection::system()
        .await
        .map_err(|e| e.to_string())?;
    let p = crate::dbus::proxy(
        &c,
        "org.freedesktop.hostname1",
        "/org/freedesktop/hostname1",
        "org.freedesktop.hostname1",
    )
    .await
    .map_err(|e| e.to_string())?;
    p.call::<_, _, ()>("SetStaticHostname", &(host.as_str(), true))
        .await
        .map_err(|e| {
            log::warn!("rename {host}: {e}");
            "Could not rename the computer".to_string()
        })?;
    let _ = p
        .call::<_, _, ()>("SetPrettyHostname", &(name.as_str(), true))
        .await;
    Ok(())
}

/// The user: login name, full name, whether an administrator (in wheel).
pub fn account() -> (String, String, bool) {
    let uid = unsafe { libc::getuid() };
    let line = std::fs::read_to_string("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .find(|l| l.split(':').nth(2) == Some(&uid.to_string()))
        .map(str::to_string)
        .unwrap_or_default();
    let f: Vec<&str> = line.split(':').collect();
    let login = f.first().copied().unwrap_or("").to_string();
    let full = f
        .get(4)
        .and_then(|g| g.split(',').next())
        .unwrap_or("")
        .to_string();
    let admin = std::fs::read_to_string("/etc/group")
        .unwrap_or_default()
        .lines()
        .any(|l| {
            let g: Vec<&str> = l.split(':').collect();
            g.first() == Some(&"wheel")
                && g.get(3).is_some_and(|m| m.split(',').any(|u| u == login))
        });
    (login, full, admin)
}

/// Sets the user's full name (chfn through polkit's pkexec).
pub async fn set_full_name(name: String) -> Result<(), String> {
    let name: String = name
        .chars()
        .filter(|c| !matches!(c, ':' | ',' | '=' | '\n'))
        .collect();
    let (login, _, _) = account();
    let ok = tokio::process::Command::new("pkexec")
        .args(["chfn", "-f", name.trim(), &login])
        .status()
        .await
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("Could not change the name".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn graphics_names_read_naturally() {
        assert_eq!(
            super::gpu_name("Intel Corporation Alder Lake-UP3 GT2 [Iris Xe Graphics] (rev 0c)"),
            "Intel Iris Xe Graphics"
        );
        assert_eq!(
            super::gpu_name("NVIDIA Corporation AD107M [GeForce RTX 4060 Max-Q / Mobile] (rev a1)"),
            "NVIDIA GeForce RTX 4060 Max-Q / Mobile"
        );
        assert_eq!(
            super::gpu_name(
                "Advanced Micro Devices, Inc. [AMD/ATI] Navi 23 [Radeon RX 6600] (rev c7)"
            ),
            "AMD Radeon RX 6600"
        );
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(super::size(349_000_000_000), "349 GB");
        assert_eq!(super::size(1_800_000_000_000), "1.8 TB");
    }
}

#[cfg(test)]
mod live {
    /// Graphics and compositor as read here (by hand: `--ignored`).
    #[test]
    #[ignore]
    fn reads_graphics_and_compositor() {
        let a = super::read_all();
        println!("{:?} | {}", a.graphics, a.compositor);
    }
}
