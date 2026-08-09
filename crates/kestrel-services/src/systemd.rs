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

async fn manager(c: &zbus::Connection) -> zbus::Result<zbus::Proxy<'static>> {
    crate::dbus::proxy(
        c,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
}

/// Runs `argv` as a user service of its own (`name`, e.g. `kestrel-hyprsunset.service`), as
/// `systemd-run --user` does: systemd starts it again if it fails, it stops with the graphical
/// session, and it is gone once it ends. Not part of kestrel's service, so a shell restart or
/// crash leaves it running.
pub async fn run_service(name: &str, description: &str, argv: &[&str]) -> zbus::Result<()> {
    use zbus::zvariant::Value;
    let c = zbus::Connection::session().await?;
    let exec: Vec<(String, Vec<String>, bool)> = vec![(
        argv.first().copied().unwrap_or_default().to_string(),
        argv.iter().map(|a| (*a).to_string()).collect(),
        false,
    )];
    let props: Vec<(&str, Value<'_>)> = vec![
        ("Description", Value::from(description)),
        ("ExecStart", Value::new(exec)),
        ("Restart", Value::from("on-failure")),
        ("RestartUSec", Value::from(1_000_000u64)),
        ("PartOf", Value::new(vec!["graphical-session.target"])),
        ("After", Value::new(vec!["graphical-session.target"])),
        // Removed once it ends, failed or not, so the name is free to run it again.
        ("CollectMode", Value::from("inactive-or-failed")),
    ];
    let aux: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();
    let _: zbus::zvariant::OwnedObjectPath = manager(&c)
        .await?
        .call("StartTransientUnit", &(name, "replace", props, aux))
        .await?;
    Ok(())
}

/// Stops a user unit (nothing if it is not running).
pub async fn stop_user_unit(name: &str) -> zbus::Result<()> {
    let c = zbus::Connection::session().await?;
    let r: zbus::Result<zbus::zvariant::OwnedObjectPath> = manager(&c)
        .await?
        .call("StopUnit", &(name, "replace"))
        .await;
    match r {
        Ok(_) => Ok(()),
        // Not loaded: already stopped.
        Err(zbus::Error::MethodError(n, _, _)) if n.as_str().ends_with("NoSuchUnit") => Ok(()),
        Err(e) => Err(e),
    }
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

#[cfg(test)]
mod live_run {
    /// Starts `sleep 30` as its own unit (by hand: `--ignored`; stop_unit test ends it).
    #[tokio::test]
    #[ignore]
    async fn runs_a_service() {
        super::run_service(
            "kestrel-test-sleep.service",
            "kestrel test",
            &["/usr/bin/sleep", "30"],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn stops_a_service() {
        super::stop_user_unit("kestrel-test-sleep.service")
            .await
            .unwrap();
        super::stop_user_unit("kestrel-test-sleep.service")
            .await
            .unwrap();
    }
}
