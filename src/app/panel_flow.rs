//! Opening, switching and closing panels and the Settings window.

use super::*;

/// The Settings window's app id (Hyprland's class for it).
pub const SETTINGS_CLASS: &str = "kestrel-settings";

impl App {
    /// Opens Settings as an ordinary window, or brings the open one forward.
    pub fn open_settings(&mut self) -> Task<Message> {
        // A panel open over it goes (Settings is not a panel any more).
        let close = if self.panel.as_ref().is_some_and(|p| p.open) {
            self.close()
        } else {
            Task::none()
        };
        if self.settings_window.is_some() {
            return Task::batch([
                close,
                Task::future(services::hyprland::dispatch(format!(
                    "hl.dsp.focus({{ window = \"class:^({SETTINGS_CLASS})$\" }})"
                )))
                .discard(),
            ]);
        }
        let (id, task) = iced::new_window(iced::WindowSettings {
            title: "Settings".into(),
            app_id: SETTINGS_CLASS.into(),
            size: (980, 660),
            min_size: (760, 480),
        });
        self.settings_window = Some(id);
        Task::batch([close, task, panels::open_settings(self)])
    }

    /// Closes the Settings window: a toggle while it is in front, `kestrel close settings`, or
    /// one of its buttons that hands over to a terminal. (Its ✕ closes it in the compositor;
    /// `WindowClosed` then forgets it.)
    pub fn close_settings(&mut self) -> Task<Message> {
        match self.settings_window {
            Some(id) => {
                self.settings_gone();
                iced::destroy_layer_surface(id)
            }
            None => Task::none(),
        }
    }

    /// The Settings window is gone, however it went (closed here or by the compositor): it is
    /// forgotten and what was half done in it ends (`settings::closed`).
    pub(crate) fn settings_gone(&mut self) {
        self.settings_window = None;
        panels::settings::closed(&mut self.panels.settings);
    }

    pub(crate) fn toggle(
        &mut self,
        kind: PanelKind,
        output: OutputId,
        button: Rectangle,
    ) -> Task<Message> {
        // No notifications: the island says so for a moment and goes back (as a notification
        // does), instead of opening an empty list.
        if kind == PanelKind::Notifications
            && self.notifications.is_empty()
            && !self
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == kind)
        {
            self.show_event(Icon::Bell, "No notifications".into(), String::new());
            return Task::none();
        }
        if let Some(p) = &mut self.panel
            && p.output == output
            && p.kind == kind
            && p.open
        {
            return self.close();
        }
        self.show(kind, output, button)
    }

    /// Asked to open `kind` under `button`, or move the open panel there: at once, or, for a
    /// panel that waits for what it shows (`panels::prepare`), once that is in.
    pub(crate) fn show(
        &mut self,
        kind: PanelKind,
        output: OutputId,
        button: Rectangle,
    ) -> Task<Message> {
        match panels::prepare(self, kind, output, button) {
            Some(waiting) => waiting,
            None => self.open_panel(kind, output, button),
        }
    }

    /// Opens `kind` under `button` now (a waiting panel's `Action::Open` comes here), or moves
    /// the open panel there.
    pub fn open_panel(
        &mut self,
        kind: PanelKind,
        output: OutputId,
        button: Rectangle,
    ) -> Task<Message> {
        // Another panel in a question's place: its question is answered "cancelled".
        if let Some(old) = self
            .panel
            .as_ref()
            .filter(|p| p.open && p.kind != kind)
            .map(|p| p.kind)
        {
            panels::closed(self, old);
        }
        let Some(o) = self.output(output) else {
            return Task::none();
        };
        let screen_w = o.size.width;
        let w = kind.width_for(self);
        let x = place(button.center_x(), w, screen_w, self.panel_edge());
        let natural = kind
            .height_for(self)
            .or_else(|| self.natural.get(&kind).map(|s| s.height));
        match &mut self.panel {
            // The same panel, reopened while it was closing: it grows back from where it is.
            Some(p) if p.output == output && p.kind == kind => {
                p.open = true;
                p.x.snap(x);
                p.w.snap(w);
                p.t.snap(1.0);
            }
            // Anything else, a switch included: the panel appears in its own place under the
            // bar and reveals downward. Nothing slides or stretches across the bar; the old
            // panel is simply replaced.
            _ => {
                // A panel is simply there, at once: no fade in, no transition (design call
                // 2026-11-14: under load any animation crawled and read as lag).
                let t = Spring::new(1.0);
                self.panel = Some(Panel {
                    kind,
                    output,
                    open: true,
                    x: Spring::new(x),
                    w: Spring::new(w),
                    // Its last known height straight away; corrected when measured.
                    h: Spring::new(natural.unwrap_or(0.0)),
                    t,
                });
            }
        }
        if kind == PanelKind::Notifications {
            self.unread = false;
            self.unseen = 0;
            self.toasts.clear();
            self.settle_toasts();
        }
        self.clock.reset();
        let surface = self.sync_panel_surface();
        Task::batch([surface, panels::opened(self, kind)])
    }

    pub(crate) fn close(&mut self) -> Task<Message> {
        // A question closed without an answer (a file, "Open with", a password): whoever
        // waits on it hears "cancelled".
        if let Some(kind) = self.panel.as_ref().filter(|p| p.open).map(|p| p.kind) {
            panels::closed(self, kind);
        }
        // One asked for but still waiting for its content stays closed too.
        panels::cancel(self);
        // Gone on the click (design call 2026-11-13: any delay closing felt slow); its own surface
        // goes with it, and Hyprland animates that out.
        self.panel = None;
        self.drop_panel_surface()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::settings;

    /// Records new keys for one of the user's shortcuts in Settings › Shortcuts.
    fn rebinding(app: &mut App) {
        let _ = app.open_settings();
        for m in [
            settings::Message::Page(settings::Page::Shortcuts),
            settings::Message::Rebind("SUPER + T".into()),
        ] {
            let _ = panels::update(app, panels::Message::Settings(m));
        }
        assert!(settings::recording(&app.panels.settings));
    }

    #[test]
    fn closing_settings_stops_recording_a_shortcut() {
        // Closed by a toggle or `kestrel close settings`.
        let (mut app, _) = crate::app::boot();
        rebinding(&mut app);
        let _ = app.close_settings();
        assert!(app.settings_window.is_none());
        assert!(!settings::recording(&app.panels.settings));

        // Closed by its ✕ (the compositor).
        rebinding(&mut app);
        let id = app.settings_window.map(iced::window::Id::from);
        let _ = crate::app::update(&mut app, Message::WindowClosed(id.unwrap()));
        assert!(app.settings_window.is_none());
        assert!(!settings::recording(&app.panels.settings));
    }

    #[test]
    fn settings_closes_itself_not_the_panel() {
        // Storage › a drive's Open hands over to the file manager and closes Settings.
        let (mut app, _) = crate::app::boot();
        let _ = app.open_settings();
        let _ = panels::update(
            &mut app,
            panels::Message::Settings(settings::Message::VolumeOpen(
                "/dev/null".into(),
                Some("/tmp".into()),
            )),
        );
        assert!(app.settings_window.is_none());
    }

    #[test]
    fn control_centre_gear_keeps_settings_open() {
        // Settings is the window in front; the gear in the control centre brings it forward.
        let (mut app, _) = crate::app::boot();
        let _ = app.open_settings();
        app.hypr.window_class = SETTINGS_CLASS.into();
        let _ = panels::update(
            &mut app,
            panels::Message::Control(panels::control::Message::Toggle("settings")),
        );
        assert!(app.settings_window.is_some());
    }

    #[test]
    fn keys_typed_in_settings_are_its_own() {
        // The open panel's keys (and Esc) ignore what is typed in Settings' window.
        let (mut app, _) = crate::app::boot();
        let other = iced::window::Id::from(SurfaceId::unique());
        assert!(!panels::in_settings(&app, other));
        let _ = app.open_settings();
        let settings = app.settings_window.map(iced::window::Id::from).unwrap();
        assert!(panels::in_settings(&app, settings));
        assert!(!panels::in_settings(&app, other));
        // Esc there leaves Settings and everything else as it was.
        let _ = crate::app::update(&mut app, Message::Escape(settings));
        assert!(app.settings_window.is_some());
    }
}
