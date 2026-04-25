//! Storage through udisks2 (the system's disk service): every mounted filesystem with how full
//! it is, and drives plugged in (USB sticks, SD cards, phones' storage) with Open, Eject and
//! mounting. Only filesystems are offered: partitions without one (EFI, swap) are left out.

use super::dbus;
use std::collections::HashMap;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const UD: &str = "org.freedesktop.UDisks2";

#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    /// udisks object path of the block device.
    pub path: String,
    /// "Windows", "root", "SanDisk Ultra".
    pub name: String,
    /// Where it is open ("/run/media/yash/USB"), if mounted.
    pub mount: Option<String>,
    pub size: u64,
    /// Bytes free while mounted.
    pub free: Option<u64>,
    /// A drive you plug in (USB, SD card).
    pub removable: bool,
    /// The drive's object path (for ejecting).
    pub drive: String,
}

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn prop<T: TryFrom<OwnedValue>>(m: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    m.get(k).and_then(|v| T::try_from(v.clone()).ok())
}

/// A NUL-ended byte string (udisks sends paths that way).
fn bytes(v: Vec<u8>) -> String {
    String::from_utf8_lossy(v.split(|b| *b == 0).next().unwrap_or(&[])).into_owned()
}

fn free_at(mount: &str) -> Option<u64> {
    let c = std::ffi::CString::new(mount).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // Safety: a valid C string and a zeroed struct the call fills.
    (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then(|| st.f_bavail * st.f_frsize)
}

pub async fn list() -> Vec<Volume> {
    let Ok(c) = Connection::system().await else {
        return Vec::new();
    };
    let objects: Objects = match dbus::proxy(
        &c,
        UD,
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    {
        Ok(p) => p.call("GetManagedObjects", &()).await.unwrap_or_default(),
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for (path, ifaces) in &objects {
        let (Some(block), Some(fs)) = (
            ifaces.get("org.freedesktop.UDisks2.Block"),
            ifaces.get("org.freedesktop.UDisks2.Filesystem"),
        ) else {
            continue;
        };
        if prop::<bool>(block, "HintIgnore").unwrap_or(false) {
            continue;
        }
        let drive: OwnedObjectPath =
            prop(block, "Drive").unwrap_or_else(|| OwnedObjectPath::try_from("/").unwrap());
        let d = objects
            .get(&drive)
            .and_then(|i| i.get("org.freedesktop.UDisks2.Drive"));
        let removable = d.is_some_and(|d| {
            prop::<bool>(d, "Removable").unwrap_or(false)
                || prop::<String>(d, "ConnectionBus").is_some_and(|b| b == "usb" || b == "sdio")
        });
        let mounts: Vec<Vec<u8>> = prop(fs, "MountPoints").unwrap_or_default();
        let mount = mounts.into_iter().map(bytes).find(|m| !m.is_empty());
        // System partitions not mounted (EFI) are of no use here.
        if mount.is_none() && !removable {
            continue;
        }
        let label: String = prop(block, "IdLabel").unwrap_or_default();
        let name = if mount.as_deref() == Some("/") {
            "System".into()
        } else if !label.is_empty() {
            label
        } else if let Some(d) = d {
            let v: String = prop(d, "Vendor").unwrap_or_default();
            let m: String = prop(d, "Model").unwrap_or_default();
            format!("{v} {m}").trim().to_string()
        } else {
            bytes(prop(block, "PreferredDevice").unwrap_or_default())
        };
        out.push(Volume {
            path: path.to_string(),
            free: mount.as_deref().and_then(free_at),
            name,
            mount,
            size: prop(block, "Size").unwrap_or(0),
            removable,
            drive: drive.to_string(),
        });
    }
    out.sort_by(|a, b| {
        a.removable
            .cmp(&b.removable)
            .then((a.mount.as_deref() != Some("/")).cmp(&(b.mount.as_deref() != Some("/"))))
            .then(a.name.cmp(&b.name))
    });
    out
}

fn no_opts() -> HashMap<&'static str, Value<'static>> {
    HashMap::new()
}

/// Mounts a volume; Ok(where it is now).
pub async fn mount(path: String) -> Result<String, String> {
    let c = Connection::system().await.map_err(|e| e.to_string())?;
    let p = dbus::proxy(&c, UD, &path, "org.freedesktop.UDisks2.Filesystem")
        .await
        .map_err(|e| e.to_string())?;
    p.call::<_, _, String>("Mount", &(no_opts(),))
        .await
        .map_err(|e| {
            log::warn!("mount {path}: {e}");
            "Could not open the drive".to_string()
        })
}

/// Unmounts a volume and, for a drive you plug in, powers it off so it can be pulled out.
pub async fn eject(path: String, drive: String) -> Result<(), String> {
    let c = Connection::system().await.map_err(|e| e.to_string())?;
    let p = dbus::proxy(&c, UD, &path, "org.freedesktop.UDisks2.Filesystem")
        .await
        .map_err(|e| e.to_string())?;
    let r: zbus::Result<()> = p.call("Unmount", &(no_opts(),)).await;
    if let Err(e) = r
        && !e.to_string().contains("NotMounted")
    {
        log::warn!("unmount {path}: {e}");
        return Err(if e.to_string().contains("Busy") {
            "In use: close what is open on it first".into()
        } else {
            "Could not eject".into()
        });
    }
    if drive != "/"
        && let Ok(d) = dbus::proxy(&c, UD, &drive, "org.freedesktop.UDisks2.Drive").await
    {
        let _: zbus::Result<()> = d.call("PowerOff", &(no_opts(),)).await;
    }
    Ok(())
}

/// What fills a folder: its biggest folders and files (one level down), largest first, with
/// the folder's total. Measured with `du` (on this filesystem only); slow on a cold cache, so
/// run it in the background.
pub async fn usage(dir: std::path::PathBuf) -> (u64, Vec<(std::path::PathBuf, u64)>) {
    let out = tokio::process::Command::new("du")
        .args(["-b", "-x", "--max-depth=1"])
        .arg(&dir)
        .stderr(std::process::Stdio::null())
        .output()
        .await;
    let Ok(out) = out else {
        return (0, Vec::new());
    };
    let mut total = 0;
    let mut items: Vec<(std::path::PathBuf, u64)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (n, p) = l.split_once('\t')?;
            Some((std::path::PathBuf::from(p), n.parse().ok()?))
        })
        .filter(|(p, n)| {
            if *p == dir {
                total = *n;
                false
            } else {
                true
            }
        })
        .collect();
    items.sort_by_key(|x| std::cmp::Reverse(x.1));
    items.truncate(10);
    (total, items)
}
