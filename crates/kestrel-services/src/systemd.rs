//! The user's systemd (the session bus's `org.freedesktop.systemd1`), asked directly: no
//! `systemctl` processes.

/// Restarts a user unit (`kestrel.service`), as `systemctl --user restart` does.
pub async fn restart_user_unit(name: &str) -> zbus::Result<()> {
    let c = zbus::Connection::session().await?;
    let _: zbus::zvariant::OwnedObjectPath = crate::dbus::proxy(
        &c,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await?
    .call("RestartUnit", &(name, "replace"))
    .await?;
    Ok(())
}

#[cfg(test)]
mod live {
    /// Restarts the idle prompt process through systemd (by hand: `--ignored`).
    #[tokio::test]
    #[ignore]
    async fn restarts_a_user_unit() {
        super::restart_user_unit("kestrel-prompt.service")
            .await
            .unwrap();
    }
}
