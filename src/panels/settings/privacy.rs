//! Settings › privacy: its page.

use super::*;

pub(super) async fn gsettings_privacy(key: &'static str, value: String) {
    let _ = tokio::process::Command::new("gsettings")
        .args(["set", "org.gnome.desktop.privacy", key, &value])
        .status()
        .await;
}

pub(super) fn privacy_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let st = &app.panels.settings;
    let Some((lock, remember, days)) = st.privacy else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(on) = lock {
        out.extend(group(
            s,
            Some("Screen lock"),
            vec![row_of(
                s,
                "Lock when the laptop sleeps",
                None,
                ui::switch(s, on, msg(Message::LockOnSleep(!on))),
            )],
        ));
    }
    let clear = |which: u8, what: &'static str| {
        let armed = st.clear_armed == Some(which);
        control(label(
            if armed { "Clear?" } else { what },
            text::SMALL,
            if armed { s.bad } else { s.ink },
        ))
        .pill(s, crate::ui::control::Pill::Plain)
        .on_press(msg(Message::Clear(which)))
        .into()
    };
    let mut recent = vec![row_of(
        s,
        "Remember files used",
        None,
        ui::switch(s, remember, msg(Message::RememberRecent(!remember))),
    )];
    if remember {
        let opts: Vec<(&str, bool, AppMessage)> = [
            ("7 days", 7),
            ("30 days", 30),
            ("A year", 365),
            ("Always", -1),
        ]
        .into_iter()
        .map(|(n, d)| (n, days == d, msg(Message::RecentDays(d))))
        .collect();
        recent.push(row_of(s, "Keep them for", None, segments(s, &opts)));
    }
    recent.push(row_of(s, "Files used", None, clear(0, "Clear")));
    out.extend(group(s, Some("Recent files"), recent));
    out.extend(group(
        s,
        Some("Clipboard"),
        vec![row_of(s, "Clipboard history", None, clear(1, "Clear"))],
    ));
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}
