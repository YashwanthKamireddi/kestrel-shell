//! Session actions through systemd-logind on the system bus.

use zbus::Connection;

const DEST: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Lock,
    Suspend,
    Reboot,
    PowerOff,
}

pub async fn run(action: Action) {
    if let Err(e) = call(action).await {
        log::warn!("logind {action:?}: {e}");
    }
}

async fn call(action: Action) -> zbus::Result<()> {
    let c = Connection::system().await?;
    match action {
        Action::Lock => {
            // Locks this session; hypridle (or any locker listening to logind) shows the lock.
            c.call_method(
                Some(DEST),
                "/org/freedesktop/login1/session/auto",
                Some("org.freedesktop.login1.Session"),
                "Lock",
                &(),
            )
            .await?;
        }
        Action::Suspend => {
            c.call_method(Some(DEST), PATH, Some(MANAGER), "Suspend", &(true,))
                .await?;
        }
        Action::Reboot => {
            c.call_method(Some(DEST), PATH, Some(MANAGER), "Reboot", &(true,))
                .await?;
        }
        Action::PowerOff => {
            c.call_method(Some(DEST), PATH, Some(MANAGER), "PowerOff", &(true,))
                .await?;
        }
    }
    Ok(())
}
