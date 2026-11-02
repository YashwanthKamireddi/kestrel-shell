//! Per-frame work: which springs are moving, stepping them, and the music visualiser.

use super::*;

impl App {
    pub(crate) fn animating(&self) -> bool {
        self.panel
            .as_ref()
            .is_some_and(|p| !(p.h.settled() && p.t.settled() && p.x.settled() && p.w.settled()))
            || !self.toast_sheet.h.settled()
            || !self.toast_w.settled()
            || !self.osd_sheet.h.settled()
            || self
                .outputs
                .iter()
                .any(|o| !o.pill_x.settled() || !o.pill_r.settled() || !o.island_w.settled())
            || !self.panels.launcher.glide.settled()
            || !self.panels.clipboard.glide.settled()
            || !self.privacy_fade.settled()
            || !self.privacy_w.settled()
    }

    pub(crate) fn tick(&mut self, now: Instant) -> Task<Message> {
        let dt = self.clock.tick(now);
        if self.vis_fast() {
            self.vis_step(now);
        }
        for o in &mut self.outputs {
            o.pill_x.step(dt);
            o.pill_r.step(dt);
            o.island_w.step(dt);
        }
        self.panels.launcher.glide.step(dt);
        self.panels.clipboard.glide.step(dt);
        if let Some(p) = &mut self.panel {
            p.x.step(dt);
            p.w.step(dt);
            p.h.step(dt);
            p.t.step(dt);
            if !p.open && p.t.settled() {
                self.panel = None;
            }
        }
        self.toast_sheet.h.step(dt);
        self.toast_w.step(dt);
        // Shrunk back into the pill: gone.
        if self.toasts.is_empty()
            && self.toast_sheet.h.target > 0.0
            && self.toast_w.target < TOAST_WIDTH - 1.0
            && self.toast_w.settled()
            && self.toast_sheet.h.settled()
        {
            self.toast_sheet.h.snap(0.0);
            self.toast_w.snap(0.0);
            self.toast_last = None;
        }
        self.osd_sheet.h.step(dt);
        self.privacy_fade.step(dt);
        self.privacy_w.step(dt);
        // The drop-down OSD is done once its sheet has closed. Shown in the island there is no
        // sheet (it stays closed), so that would clear the OSD on the next frame: there it
        // ends by its own time instead (`expire`).
        if !crate::modules::island::hosts_osd(self)
            && self.osd_sheet.h.target == 0.0
            && self.osd_sheet.h.settled()
        {
            self.osd = None;
        }
        Task::none()
    }

    /// Whether the visualiser bars move this frame: sound playing (or settling), or falling to
    /// rest after a pause. They then step with the screen's frames (`tick`), not a timer that
    /// drifts against vsync.
    pub(crate) fn vis_fast(&self) -> bool {
        if crate::motion::still() {
            return false;
        }
        let playing = self.media.main().is_some_and(|p| p.playing);
        let moving = self.levels.iter().any(|b| *b > 0.09 || *b < 0.07);
        (playing
            && self.config.bar.visualiser
            && (self.envelope > 0.02 || self.loudness > 0.02 || moving))
            || (self.level_last.is_some() && !playing)
    }

    /// One step of the visualiser bars toward the newest sound reading.
    pub(crate) fn vis_step(&mut self, now: Instant) {
        let dt = self
            .level_last
            .map_or(1.0 / 60.0, |t| {
                now.saturating_duration_since(t).as_secs_f32()
            })
            .min(0.1);
        self.level_last = Some(now);
        self.level_phase += dt;
        // The newest reading (kept by the level stream, no message of its own).
        let bands = services::level::latest();
        // Rises fast, falls slower (like cava): the old per-reading 0.6 / 0.25 at 30 a
        // second, as rates over the frame's time.
        for (target, b) in self.level_targets.iter_mut().zip(bands) {
            let rate = if b > *target {
                tok::METER_RISE
            } else {
                tok::METER_FALL
            };
            *target += (b - *target) * (1.0 - (-dt * rate).exp());
        }
        self.envelope = bands.iter().copied().fold(0.0, f32::max);
        // The bars glide to their bands at the screen's frame rate.
        let playing = self.media.main().is_some_and(|p| p.playing);
        for (bar, target) in self.levels.iter_mut().zip(self.level_targets) {
            let to = if playing { 0.08 + target * 0.92 } else { 0.0 };
            *bar += (to - *bar) * (1.0 - (-dt * tok::METER_GLIDE).exp());
        }
        self.loudness = self.envelope;
        if !playing && self.levels.iter().all(|b| *b < 0.005) {
            self.levels = [0.0; 4];
            self.level_last = None;
        }
    }
}
