//! Real brightness for external monitors over DDC/CI, spoken directly on the monitor's i2c bus
//! (no `ddcutil` processes). Only monitors that answer are used; one with DDC/CI switched off
//! in its own menu does not answer, and the shell dims the picture instead
//! (`toggles::set_dim`). Setting is slow (~50 ms per write, the protocol's own pause), so while
//! a slider moves only the newest value is sent once the previous write finishes.
//!
//! The protocol is VESA DDC/CI as ddcutil speaks it (its base/ddc_packets.c, base/parms.h):
//! requests to i2c address 0x37 as `0x51, 0x80|length, data…, checksum` (XOR from 0x6E), a
//! Get VCP Feature reply read 40 ms after its request as `0x6E, 0x80|length, 0x02, result,
//! code, type, max, current, checksum` (XOR from 0x50), a pause of 50 ms after a set. A
//! monitor's bus is the kernel's own link from its DRM connector (`/sys/class/drm/*/ddc`).

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::sync::Mutex;
use std::time::Duration;

/// A monitor that answers: its connector ("HDMI-A-1") and i2c bus number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub connector: String,
    pub bus: u32,
}

/// The monitor's own address on the bus (7-bit; 0x6E on the wire).
const MONITOR: libc::c_ulong = 0x37;
/// linux/i2c-dev.h: talk to this address from now on.
const I2C_SLAVE: libc::c_ulong = 0x0703;
/// The brightness control.
const BRIGHTNESS: u8 = 0x10;
const GET_TO_REPLY: Duration = Duration::from_millis(40);
const AFTER_SET: Duration = Duration::from_millis(50);
/// The longest packet a monitor sends.
const MAX_PACKET: usize = 39;

/// A request packet: source 0x51, length, the data, the checksum (the destination 0x6E, then
/// every byte, XORed).
fn request(data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(data.len() + 3);
    p.push(0x51);
    p.push(0x80 | data.len() as u8);
    p.extend_from_slice(data);
    let sum = p.iter().fold(0x6e, |a, b| a ^ b);
    p.push(sum);
    p
}

/// A Get VCP Feature reply: (current, maximum), or None if it is not a valid answer for `code`
/// (a null message: the monitor is busy or does not do DDC/CI).
fn parse_reply(r: &[u8], code: u8) -> Option<(u16, u16)> {
    let len = usize::from(*r.get(1)? & 0x7f);
    let packet = r.get(..2 + len + 1)?;
    let sum = packet[..2 + len].iter().fold(0x50, |a, b| a ^ b);
    if r[0] != 0x6e || r[1] & 0x80 == 0 || len != 8 || packet[2 + len] != sum {
        return None;
    }
    let d = &packet[2..2 + len];
    // Feature reply (0x02), no error, about the asked code.
    if d[0] != 0x02 || d[1] != 0x00 || d[2] != code {
        return None;
    }
    let max = u16::from_be_bytes([d[4], d[5]]);
    let cur = u16::from_be_bytes([d[6], d[7]]);
    Some((cur, max))
}

fn open(bus: u32) -> std::io::Result<File> {
    let f = File::options()
        .read(true)
        .write(true)
        .open(format!("/dev/i2c-{bus}"))?;
    // Safety: the i2c-dev ioctl that sets the address of later reads and writes on this fd.
    if unsafe { libc::ioctl(f.as_raw_fd(), I2C_SLAVE, MONITOR) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(f)
}

/// A feature's (current, maximum), asked up to `tries` times (DDC/CI answers are unreliable).
fn get_vcp(bus: u32, code: u8, tries: u32) -> Option<(u16, u16)> {
    let mut f = open(bus).ok()?;
    let req = request(&[0x01, code]);
    for _ in 0..tries {
        if f.write_all(&req).is_err() {
            std::thread::sleep(AFTER_SET);
            continue;
        }
        std::thread::sleep(GET_TO_REPLY);
        let mut r = [0u8; MAX_PACKET];
        if f.read(&mut r).is_ok()
            && let Some(v) = parse_reply(&r, code)
        {
            return Some(v);
        }
        std::thread::sleep(AFTER_SET);
    }
    None
}

/// Sets a feature (the monitor gives no answer to a set).
fn set_vcp(bus: u32, code: u8, value: u16) -> std::io::Result<()> {
    let mut f = open(bus)?;
    let [hi, lo] = value.to_be_bytes();
    let req = request(&[0x03, code, hi, lo]);
    let mut last = Ok(());
    for _ in 0..4 {
        last = f.write_all(&req);
        std::thread::sleep(AFTER_SET);
        if last.is_ok() {
            break;
        }
    }
    last
}

/// External monitors on a connector with a DDC bus (the built-in panel has a backlight).
fn external_buses() -> Vec<(String, u32)> {
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut v: Vec<(String, u32)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // "card1-HDMI-A-1" → "HDMI-A-1"
            let connector = name.split_once('-')?.1.to_string();
            if ["eDP", "LVDS", "DSI"]
                .iter()
                .any(|i| connector.starts_with(i))
            {
                return None;
            }
            let status = std::fs::read_to_string(e.path().join("status")).ok()?;
            if status.trim() != "connected" {
                return None;
            }
            let bus = std::fs::read_link(e.path().join("ddc")).ok()?;
            let bus = bus
                .file_name()?
                .to_str()?
                .strip_prefix("i2c-")?
                .parse()
                .ok()?;
            Some((connector, bus))
        })
        .collect();
    v.sort();
    v
}

/// The monitors that answer DDC/CI.
pub async fn displays() -> Vec<Display> {
    tokio::task::spawn_blocking(|| {
        external_buses()
            .into_iter()
            .filter(|(_, bus)| get_vcp(*bus, BRIGHTNESS, 2).is_some())
            .map(|(connector, bus)| Display { connector, bus })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// The monitor's brightness, 0 to 1.
pub async fn get(bus: u32) -> Option<f32> {
    let (cur, max) = tokio::task::spawn_blocking(move || get_vcp(bus, BRIGHTNESS, 10))
        .await
        .ok()??;
    (max > 0).then(|| f32::from(cur) / f32::from(max))
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
        let v = u16::try_from(want).unwrap_or(100);
        let r = tokio::task::spawn_blocking(move || set_vcp(bus, BRIGHTNESS, v)).await;
        if let Ok(Err(e)) = r {
            log::warn!("ddc: bus {bus}: {e}");
        }
        sent = Some(want);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_are_as_ddcutil_builds_them() {
        // ddcutil unit_tests/base/test_ddc_packets.c: Get VCP 0x10, and Set VCP 0x12 to 0x0134
        // (the destination 0x6E is the i2c address, not sent as a byte).
        assert_eq!(
            request(&[0x01, 0x10]),
            vec![0x51, 0x82, 0x01, 0x10, 0x6e ^ 0x51 ^ 0x82 ^ 0x01 ^ 0x10]
        );
        assert_eq!(
            request(&[0x03, 0x12, 0x01, 0x34]),
            vec![
                0x51,
                0x84,
                0x03,
                0x12,
                0x01,
                0x34,
                0x6e ^ 0x51 ^ 0x84 ^ 0x03 ^ 0x12 ^ 0x01 ^ 0x34
            ]
        );
    }

    #[test]
    fn replies_are_checked() {
        let mut r = vec![0x6e, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x32];
        r.push(r.iter().fold(0x50, |a, b| a ^ b));
        r.resize(MAX_PACKET, 0xff);
        assert_eq!(parse_reply(&r, 0x10), Some((50, 100)));
        // Another code, an error result, a bad checksum, a null message.
        assert_eq!(parse_reply(&r, 0x12), None);
        let mut bad = r.clone();
        bad[3] = 0x01;
        assert_eq!(parse_reply(&bad, 0x10), None);
        let mut bad = r.clone();
        bad[10] ^= 1;
        assert_eq!(parse_reply(&bad, 0x10), None);
        // ddcutil's spec case 6.4: the null message's checksum is 0xBE.
        assert_eq!(parse_reply(&[0x6e, 0x80, 0xbe], 0x10), None);
    }
}

#[cfg(test)]
mod live {
    /// The monitors that answer here, and how long finding out takes (by hand: `--ignored`).
    #[tokio::test]
    #[ignore]
    async fn finds_the_answering_monitors() {
        println!("external: {:?}", super::external_buses());
        let t = std::time::Instant::now();
        println!(
            "answering: {:?} in {:?}",
            super::displays().await,
            t.elapsed()
        );
    }
}
