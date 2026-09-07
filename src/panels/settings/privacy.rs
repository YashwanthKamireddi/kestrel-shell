//! Settings › privacy: its page.

use super::*;

pub(super) async fn gsettings_privacy(key: &'static str, value: crate::services::gsettings::Set) {
    if let Err(e) = crate::services::gsettings::set("org.gnome.desktop.privacy", key, value).await {
        log::warn!("{e}");
    }
}

pub(super) fn privacy_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let Some((lock, remember, days)) = st.privacy else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // Camera and microphone: what uses them now and what did in the last 10 minutes.
    {
        let mut rows: Vec<Option<Element<'_, Message>>> = Vec::new();
        let now = |device: &'static str, apps: &[String]| {
            if apps.is_empty() {
                None
            } else {
                value_row(s, device, format!("{} (now)", apps.join(", ")))
            }
        };
        rows.push(now("Camera", cx.camera_apps));
        rows.push(now("Microphone", &cx.audio.recording));
        for (d, n, t) in cx.privacy_recent {
            let what = if *d == crate::ui::Icon::Video {
                "Camera"
            } else {
                "Microphone"
            };
            rows.push(value_row(
                s,
                what,
                format!("{n} · {}", crate::panels::privacy::ago(*t)),
            ));
        }
        if rows.iter().all(Option::is_none) {
            rows = vec![value_row(
                s,
                "Camera and microphone",
                "Not used in the last 10 minutes".into(),
            )];
        }
        out.extend(group(s, Some("Camera and microphone"), rows));
    }
    if let Some(on) = lock {
        out.extend(group(
            s,
            Some("Screen lock"),
            vec![row_of(
                s,
                "Lock when the laptop sleeps",
                None,
                ui::switch(s, on, Message::LockOnSleep(!on)),
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
        .on_press(Message::Clear(which))
        .into()
    };
    let mut recent = vec![row_of(
        s,
        "Remember files used",
        None,
        ui::switch(s, remember, Message::RememberRecent(!remember)),
    )];
    if remember {
        let opts: Vec<(&str, bool, Message)> = [
            ("7 days", 7),
            ("30 days", 30),
            ("A year", 365),
            ("Always", -1),
        ]
        .into_iter()
        .map(|(n, d)| (n, days == d, Message::RecentDays(d)))
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
