//! Screen checks (`make check`): every panel is built, laid out and drawn with the real
//! renderer, without a screen, in its plain state and in a busy one. A panel fails when its
//! view, layout or drawing panics, when it is empty, or when it takes all the height it is
//! offered (a Fill inside a panel sized to its content: the notification list that never
//! opened, 2026-11-16). No pictures are compared, so changing the look never breaks them.

#[cfg(test)]
mod tests {
    use crate::app::{App, Message};
    use crate::panels::{self, PanelKind};
    use iced::advanced::{Layout, layout, renderer, widget};
    use iced::{Element, Pixels, Rectangle, Size, Theme, mouse};

    /// The height offered: no panel may need it all.
    const ROOM: f32 = 4000.0;

    /// Lays `el` out in a column `width` wide and draws it; its size.
    fn lay_out_and_draw(el: Element<'_, Message>, width: f32) -> Size {
        let mut el = el;
        // The CPU renderer (the shell's fallback): the checks run with or without a GPU.
        let mut r = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
            crate::theme::FONT,
            Pixels(14.0),
        ));
        let mut tree = widget::Tree::new(&el);
        let limits = layout::Limits::new(Size::ZERO, Size::new(width, ROOM));
        let node = el.as_widget_mut().layout(&mut tree, &r, &limits);
        let size = node.size();
        el.as_widget().draw(
            &tree,
            &mut r,
            &Theme::Dark,
            &renderer::Style {
                text_color: iced::Color::WHITE,
            },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::new(iced::Point::ORIGIN, Size::new(width, ROOM)),
        );
        size
    }

    const ALL: [PanelKind; 21] = [
        PanelKind::Control,
        PanelKind::Media,
        PanelKind::Calendar,
        PanelKind::Power,
        PanelKind::Tray,
        PanelKind::Launcher,
        PanelKind::Wallpaper,
        PanelKind::Keys,
        PanelKind::Island,
        PanelKind::Overview,
        PanelKind::Clipboard,
        PanelKind::Emoji,
        PanelKind::Settings,
        PanelKind::Weather,
        PanelKind::Pods,
        PanelKind::Files,
        PanelKind::Auth,
        PanelKind::Capture,
        PanelKind::Monitor,
        PanelKind::Privacy,
        PanelKind::Notifications,
    ];

    /// Shown only while something asks (a password, a file): with nothing asked they are
    /// closed, never drawn empty.
    fn only_when_asked(kind: PanelKind) -> bool {
        matches!(kind, PanelKind::Auth | PanelKind::Files)
    }

    /// Every panel of `app`: none panics, none is empty, none fills the room (Settings is a
    /// window of its own, which it fills).
    fn check_all(app: &App, state: &str) {
        let mut bad = Vec::new();
        for kind in ALL {
            let width = crate::theme::tok::PANEL_XL;
            let size = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                lay_out_and_draw(panels::view(app, kind, app.scheme), width)
            }));
            match size {
                Err(_) => bad.push(format!("{kind:?} ({state}): panicked")),
                Ok(s) if s.height >= ROOM && kind != PanelKind::Settings => {
                    bad.push(format!("{kind:?} ({state}): takes all the height offered"));
                }
                Ok(s) if (s.height <= 0.0 || s.width <= 0.0) && !only_when_asked(kind) => {
                    bad.push(format!("{kind:?} ({state}): empty {s:?}"));
                }
                Ok(_) => {}
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    fn notification(
        id: u32,
        app: &str,
        body: &str,
    ) -> crate::services::notifications::Notification {
        crate::services::notifications::Notification {
            id,
            app: app.into(),
            app_icon: String::new(),
            summary: format!("Message {id}"),
            body: body.into(),
            actions: vec![
                ("default".into(), "Open".into()),
                ("reply".into(), "Reply".into()),
            ],
            image: None,
            urgency: 1,
            timeout: -1,
            time: std::time::SystemTime::now(),
            sound: Default::default(),
            replaced: false,
        }
    }

    #[test]
    fn every_panel_lays_out_and_draws() {
        let (mut app, _) = crate::app::boot();
        check_all(&app, "as started");

        // Busy: many notifications from several apps (a list longer than the panel), a query
        // in the launcher, a long clipboard history.
        let long = "A much longer message that wraps across several lines of the card, with \
                    enough words to need the second and third line of its text.";
        app.notifications = (1..=30)
            .map(|i| notification(i, ["Discord", "Mail", "Signal"][i as usize % 3], long))
            .collect();
        let _ = panels::update(
            &mut app,
            panels::Message::Launcher(panels::launcher::Message::Query("a".into())),
        );
        let _ = panels::update(
            &mut app,
            panels::Message::Clipboard(panels::clipboard::Message::Loaded(
                panels::clipboard::sample_clips(200),
            )),
        );
        check_all(&app, "busy");
    }
}
