//! Readings from the system turned into what the shell shows: time and units, colours
//! (recolour), night light and quiet hours schedules, privacy history, audio changes, DDC.

use super::*;

impl App {
    /// The external monitor whose brightness the slider and keys set: the focused one, when it
    /// answers DDC/CI and the laptop screen is off.
    pub fn ddc_target(&self) -> Option<u32> {
        if self.outputs.iter().any(|o| o.name.starts_with("eDP")) {
            return None;
        }
        self.ddc
            .iter()
            .find(|d| d.connector == self.hypr.focused_monitor)
            .or(self.ddc.first())
            .map(|d| d.bus)
    }

    /// The time as the user wants it read ("14:05" or "2:05 PM").
    pub fn time_text(&self) -> String {
        match self.config.clock.format {
            config::ClockFormat::H24 if self.config.clock.seconds => {
                self.now.format("%H:%M:%S").to_string()
            }
            config::ClockFormat::H12 if self.config.clock.seconds => {
                self.now.format("%-I:%M:%S %p").to_string()
            }
            config::ClockFormat::H24 => self.now.format("%H:%M").to_string(),
            config::ClockFormat::H12 => self.now.format("%-I:%M %p").to_string(),
        }
    }

    /// A temperature (kept in °C) in the user's units, "21°".
    pub fn temp(&self, celsius: f32) -> String {
        self.config.weather_units.show(celsius)
    }

    /// What `bus` last reported as its artwork.
    pub(crate) fn pending_art(&self, bus: &str) -> Option<Option<String>> {
        self.reported_art.get(bus).cloned()
    }

    /// After every change: the in-use capsule follows what is in use (fading out when nothing
    /// is), and apps that stop using the camera or microphone are remembered for 10 minutes.
    pub(crate) fn track_privacy(&mut self, cam_before: &[String], mic_before: &[String]) {
        let now = crate::modules::privacy::glyphs(self);
        if now.is_empty() {
            if self.privacy_fade.target > 0.0 {
                self.privacy_fade.to(0.0, tok::SPRING_CLOSE);
                self.privacy_w.to(0.0, tok::SPRING_CLOSE);
                self.clock.reset();
            }
        } else if now != self.privacy_glyphs || self.privacy_fade.target < 1.0 {
            let w = crate::modules::privacy::capsule_width(now.len());
            // Appearing from nothing: it grows and fades in (as it leaves); an icon joining or
            // leaving: the width springs there.
            if self.privacy_fade.value < 0.02 {
                self.privacy_w.snap(0.0);
            }
            self.privacy_w.to(w, tok::SPRING_SNAP);
            self.privacy_fade.to(1.0, tok::SPRING_SHOW);
            self.clock.reset();
            self.privacy_glyphs = now;
        }
        let ended = |before: &[String], after: &[String], i: Icon, recent: &mut Vec<_>| {
            for a in before.iter().filter(|a| !after.contains(a)) {
                recent.retain(|(d, n, _): &(Icon, String, Instant)| !(*d == i && n == a));
                recent.insert(0, (i, a.clone(), Instant::now()));
            }
        };
        let cam = self.camera_apps.clone();
        let mic = self.audio.recording.clone();
        ended(cam_before, &cam, Icon::Video, &mut self.privacy_recent);
        ended(mic_before, &mic, Icon::Mic, &mut self.privacy_recent);
        // In use again: no longer "recent".
        self.privacy_recent.retain(|(d, n, t)| {
            t.elapsed() < Duration::from_secs(600)
                && !(*d == Icon::Video && cam.contains(n))
                && !(*d == Icon::Mic && mic.contains(n))
        });
        self.privacy_recent.truncate(8);
    }

    /// Apps that follow the desktop (GTK 4, Qt 6, Chromium, Firefox) read this through the
    /// portal: dark, kestrel's accent while it themes apps, and `[motion] reduce`.
    pub(crate) fn tell_appearance(&self) {
        let a = self.scheme.accent;
        services::portal::set_appearance(services::portal::Appearance {
            accent: self.config.theme_apps.then_some((
                f64::from(a.r),
                f64::from(a.g),
                f64::from(a.b),
            )),
            reduced_motion: self.config.motion.reduce,
            ..Default::default()
        });
    }

    /// The colours from the wallpaper's seed, or from the accent chosen in Settings; then the
    /// apps' colours too when kestrel themes them.
    pub fn recolour(&mut self) -> Task<Message> {
        let seed = match self
            .config
            .accent
            .as_deref()
            .and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok())
        {
            // A chosen colour counts as a wallpaper made of it: Material builds the accent.
            Some(rgb) => crate::theme::Seed {
                source: Some(rgb),
                chroma: 0.2,
                coverage: 1.0,
                ..self.seed
            },
            None => self.seed,
        };
        self.scheme = Scheme::dark(seed);
        // Kept for the prompt process, so its window has the shell's colours.
        crate::state::update(|s| {
            s.look = crate::state::Look {
                hue: seed.hue,
                chroma: seed.chroma,
                coverage: seed.coverage,
                source: seed.source,
            };
        });
        self.tell_appearance();
        if !self.config.theme_apps {
            return Task::none();
        }
        let scheme = self.scheme;
        let path = self
            .config
            .wallpaper
            .as_deref()
            .map(|p| config::expand(p).to_string_lossy().into_owned());
        Task::future(async move {
            let _ = tokio::task::spawn_blocking(move || crate::apptheme::apply(scheme, path)).await;
            crate::apptheme::notify_qt().await;
        })
        .discard()
    }

    /// The night light schedule: on from sunset to sunrise (today's times from the weather).
    /// Night light by the sun (if scheduled): the change to make, if any.
    pub fn night_schedule(&mut self) -> Task<Message> {
        if !self.config.night_light_schedule {
            self.night_auto = None;
            return Task::none();
        }
        let Some(w) = &self.weather else {
            return Task::none();
        };
        let parse = |t: &str| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok();
        let (Some(rise), Some(set)) = (parse(&w.sunrise), parse(&w.sunset)) else {
            return Task::none();
        };
        let now = self.now.time();
        let want = now >= set || now < rise;
        if self.night_auto != Some(want) {
            self.night_auto = Some(want);
            if self.night_light != want {
                self.night_light = want;
                crate::state::update(|s| s.toggles.night_light = want);
                let k = self.config.night_light_temperature;
                return Task::future(services::toggles::set_night_light(want, k)).discard();
            }
        }
        Task::none()
    }

    /// Quiet hours: do not disturb from `quiet_from` to `quiet_to` (across midnight too).
    pub fn quiet_schedule(&mut self) {
        let parse = |t: &Option<String>| {
            t.as_deref()
                .and_then(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok())
        };
        let (Some(from), Some(to)) = (parse(&self.config.quiet_from), parse(&self.config.quiet_to))
        else {
            self.quiet_auto = None;
            return;
        };
        let now = self.now.time();
        let want = if from <= to {
            now >= from && now < to
        } else {
            now >= from || now < to
        };
        if self.quiet_auto != Some(want) {
            self.quiet_auto = Some(want);
            if self.dnd != want {
                self.dnd = want;
                crate::state::update(|s| s.toggles.dnd = want);
            }
        }
    }

    pub(crate) fn audio_changed(&mut self, mut s: audio::State) {
        // Switching a Bluetooth card's mode removes its output and makes a new one: for a
        // moment the speaker is the only output, and the page drew exactly that (the device
        // list, the modes, balance and volume all jumping to the speaker's and back, under the user's
        // pointer). While the switch the user asked for is under way, the card's output and the new
        // mode stay shown; everything else updates. Done once the card's output is back and in
        // use, or after 4 s whatever happened.
        if let Some((card, mode, at)) = self.mode_switch.clone() {
            let back = s
                .sinks
                .iter()
                .any(|d| d.card == Some(card) && d.name == s.default_sink);
            if back || at.elapsed() > Duration::from_secs(4) {
                self.mode_switch = None;
            } else {
                s.sinks.clone_from(&self.audio.sinks);
                s.default_sink.clone_from(&self.audio.default_sink);
                if let Some(k) = s.cards.iter_mut().find(|k| k.index == card) {
                    k.active = mode;
                }
                self.audio = s;
                return;
            }
        }
        let old = self.audio.sink().map(|d| (d.volume, d.muted));
        let new = s.sink().map(|d| (d.volume, d.muted));
        let same_device = self.audio.default_sink == s.default_sink;
        self.audio = s;
        if same_device
            && old.is_some()
            && let (Some(o), Some(n)) = (old, new)
            && ((o.0 - n.0).abs() > 0.004 || o.1 != n.1)
        {
            let d = self.audio.sink().cloned();
            let v = if n.1 { 0.0 } else { n.0 };
            // The real level (above 100 % with boost); the bar fills to its end at most.
            self.show_osd(panels::control::volume_icon(d.as_ref()), v);
        }
    }
}
