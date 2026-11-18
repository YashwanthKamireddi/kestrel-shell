//! Date and time through systemd's timedated on the system bus: the time zone and whether the
//! clock sets itself from the network (NTP). Changing either asks polkit (a local rule may allow
//! it without a prompt; otherwise kestrel's own password prompt appears).

use super::dbus;
use zbus::Connection;

const TD: &str = "org.freedesktop.timedate1";
const PATH: &str = "/org/freedesktop/timedate1";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub timezone: String,
    /// The clock sets itself from the network.
    pub ntp: bool,
    /// Setting it from the network is possible (a time service is installed).
    pub can_ntp: bool,
    /// Every time zone, for choosing.
    pub zones: Vec<String>,
}

pub async fn read() -> State {
    let Ok(c) = Connection::system().await else {
        return State::default();
    };
    let zones: Vec<String> = match dbus::proxy(&c, TD, PATH, TD).await {
        Ok(p) => p.call("ListTimezones", &()).await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    State {
        timezone: dbus::get(&c, TD, PATH, TD, "Timezone")
            .await
            .unwrap_or_default(),
        ntp: dbus::get(&c, TD, PATH, TD, "NTP").await.unwrap_or(false),
        can_ntp: dbus::get(&c, TD, PATH, TD, "CanNTP").await.unwrap_or(false),
        zones,
    }
}

/// Sets the time zone ("Europe/Paris"); Err with a plain reason.
pub async fn set_timezone(zone: String) -> Result<(), String> {
    let c = Connection::system().await.map_err(|e| e.to_string())?;
    let p = dbus::proxy(&c, TD, PATH, TD)
        .await
        .map_err(|e| e.to_string())?;
    // interactive: polkit may ask for a password (through kestrel's agent).
    p.call::<_, _, ()>("SetTimezone", &(zone.as_str(), true))
        .await
        .map_err(|e| {
            log::warn!("time zone {zone}: {e}");
            "Could not change the time zone".to_string()
        })
}

/// Sets the clock from the network (or not).
pub async fn set_ntp(on: bool) -> Result<(), String> {
    let c = Connection::system().await.map_err(|e| e.to_string())?;
    let p = dbus::proxy(&c, TD, PATH, TD)
        .await
        .map_err(|e| e.to_string())?;
    p.call::<_, _, ()>("SetNTP", &(on, true))
        .await
        .map_err(|e| {
            log::warn!("automatic time {on}: {e}");
            "Could not change automatic time".to_string()
        })
}
