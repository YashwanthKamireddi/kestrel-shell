//! Opening, switching and closing panels and the Settings window.

use super::*;

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
                Task::future(services::hyprland::dispatch(
                    "hl.dsp.focus({ window = \"class:^(kestrel-settings)$\" })".into(),
                ))
                .discard(),
            ]);
        }
        let (id, task) = iced::new_window(iced::WindowSettings {
            title: "Settings".into(),
            app_id: "kestrel-settings".into(),
            size: (980, 660),
            min_size: (760, 480),
        });
        self.settings_window = Some(id);
        Task::batch([close, task, panels::settings::open(self)])
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

    /// Open `kind` under `button`, or move the open panel there.
    /// Opens `kind` under `at` (for panels that open themselves once their content is in).
    pub fn open_panel(
        &mut self,
        kind: PanelKind,
        output: OutputId,
        at: Rectangle,
    ) -> Task<Message> {
        self.show(kind, output, at)
    }

    pub(crate) fn show(
        &mut self,
        kind: PanelKind,
        output: OutputId,
        button: Rectangle,
    ) -> Task<Message> {
        if kind != PanelKind::Files
            && self
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Files)
        {
            panels::files::closed(self);
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
        // A file picker closed without choosing: the app waiting on it hears "cancelled".
        if self
            .panel
            .as_ref()
            .is_some_and(|p| p.open && p.kind == PanelKind::Files)
        {
            panels::files::closed(self);
        }
        // A password prompt closed without an answer: the app hears "cancelled".
        if self
            .panel
            .as_ref()
            .is_some_and(|p| p.open && p.kind == PanelKind::Auth)
        {
            panels::auth::closed(self);
        }
        // Gone on the click (design call 2026-11-13: any delay closing felt slow); its own surface
        // goes with it, and Hyprland animates that out.
        self.panel = None;
        self.drop_panel_surface()
    }
}
