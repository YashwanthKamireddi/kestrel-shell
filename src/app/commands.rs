//! Commands from `kestrel <command>` and keybinds (IPC), and game mode.

use super::*;

impl App {
    pub(crate) fn ipc(&mut self, cmd: &str) -> Task<Message> {
        let mut words = cmd.split_whitespace();
        match (words.next(), words.next()) {
            // The default app for a kind of thing ("files", "browser", …), opening an optional
            // path: what Settings > Default apps chose, terminal apps in the default terminal.
            (Some("open-default"), Some(kind)) => {
                // Everything after the kind is the path (it may hold spaces).
                let arg = cmd
                    .trim()
                    .strip_prefix("open-default")
                    .map(str::trim)
                    .and_then(|r| r.strip_prefix(kind))
                    .map_or("", str::trim);
                let mime = match kind {
                    "files" => "inode/directory",
                    "browser" => "x-scheme-handler/https",
                    "mail" => "x-scheme-handler/mailto",
                    "text" => "text/plain",
                    other => other,
                };
                let arg = if arg.is_empty() && kind == "files" {
                    std::env::var("HOME").unwrap_or_default()
                } else {
                    arg.to_string()
                };
                match services::defaults::command_for(mime, &arg) {
                    Some(cmd) => {
                        Task::done(Message::Dispatch(services::compositor::Action::Exec(cmd)))
                    }
                    None => {
                        log::warn!("open-default {kind}: no default app");
                        Task::none()
                    }
                }
            }
            // Settings is a window of its own: opened, or brought forward if open. A toggle
            // closes it when it is the window in front (as every other toggle closes its panel).
            (Some("toggle"), Some("settings"))
                if self.settings_window.is_some() && self.hypr.window_class == SETTINGS_CLASS =>
            {
                self.close_settings()
            }
            (Some("toggle" | "open"), Some("settings")) => self.open_settings(),
            (Some("close"), Some("settings")) => self.close_settings(),
            // Settings on a page: `kestrel settings sound` (bindable in Settings > Shortcuts).
            (Some("settings"), Some(name)) => match panels::settings::Page::from_slug(name) {
                Some(p) => Task::done(Message::SettingsAt(p)),
                None => {
                    log::warn!("ipc: unknown Settings page {name}");
                    Task::none()
                }
            },
            (Some("settings"), None) => self.open_settings(),
            (Some("toggle"), Some(name)) => {
                let Some(kind) = PanelKind::from_name(name) else {
                    log::warn!("ipc: unknown panel {name}");
                    return Task::none();
                };
                let o = self
                    .outputs
                    .iter()
                    .find(|o| o.name == self.hypr.focused_monitor)
                    .or(self.outputs.first());
                let Some(o) = o else {
                    return Task::none();
                };
                // Without a button to hang from, panels drop from their home position.
                let x = match kind {
                    PanelKind::Launcher
                    | PanelKind::Calendar
                    | PanelKind::Media
                    | PanelKind::Wallpaper
                    | PanelKind::Keys
                    | PanelKind::Island
                    | PanelKind::Overview
                    | PanelKind::Clipboard
                    | PanelKind::Emoji
                    | PanelKind::Weather
                    | PanelKind::Files
                    | PanelKind::Auth
                    | PanelKind::Capture
                    | PanelKind::Monitor
                    | PanelKind::Notifications
                    | PanelKind::Pods => o.size.width / 2.0,
                    _ => o.size.width - 80.0,
                };
                // The music panel from its key hangs under the music bubble, as when it is
                // clicked: the bubble sits just left of the centred time pill (6 px between
                // pills, the pill as wide as it is now plus its 14 px padding a side).
                let x = if kind == PanelKind::Media && self.config.bar.contains("media") {
                    let pill = o.island_w.value + 28.0;
                    o.size.width / 2.0 - pill / 2.0 - 6.0 - crate::modules::island::BUBBLE / 2.0
                } else {
                    x
                };
                let id = o.id;
                // The tray opens on its list of apps.
                if kind == PanelKind::Tray {
                    let _ = panels::tray::open(&mut self.panels.tray, None);
                }
                // From the keyboard (a shortcut): the panel takes the keys.
                self.panel_keys = true;
                // The overview opens once its pictures are current (a frame or so).
                let open_now = self
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.open && p.kind == kind && p.output == id);
                if kind == PanelKind::Overview && !open_now {
                    let at = Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, tok::BAR));
                    let a =
                        panels::overview::prepare(&mut self.panels.overview, &self.hypr, id, at);
                    return panels::action::perform(self, a, |m| {
                        Message::Panel(panels::Message::Overview(m))
                    });
                }
                let shown = self.toggle(
                    kind,
                    id,
                    Rectangle::new(iced::Point::new(x, 0.0), Size::new(0.0, tok::BAR)),
                );
                // "sound" (or "mixer") is the control centre opened on its Sound page.
                if matches!(name, "sound" | "mixer") {
                    self.panels.control.page = panels::control::Page::Output;
                }
                // "bluetooth" and "wifi" open on their pages (through the page message, so
                // Wi-Fi scans and Bluetooth searches as when opened by hand).
                let page = match name {
                    "bluetooth" => Some(panels::control::Page::Bluetooth),
                    "wifi" | "network" => Some(panels::control::Page::Wifi),
                    _ => None,
                };
                if let Some(p) = page {
                    return Task::batch([
                        shown,
                        Task::done(Message::Panel(panels::Message::Control(
                            panels::control::Message::Page(p),
                        ))),
                    ]);
                }
                shown
            }
            (Some("close"), _) => self.close(),
            (Some("game-mode"), _) => self.change(Change::GameMode),
            #[cfg(feature = "devtools")]
            (Some("preview"), Some(which)) => {
                use panels::auth::Preview;
                if matches!(which, "volume" | "volume-boost") {
                    self.show_osd(Icon::Volume2, if which == "volume" { 0.6 } else { 1.3 });
                    return Task::none();
                }
                match panels::auth::preview(which) {
                    Some(Preview::Polkit(r)) => {
                        Task::done(Message::Polkit(services::polkit::Event::Ask(r)))
                    }
                    Some(Preview::Wifi(a)) => Task::done(Message::WifiAsk(a)),
                    Some(Preview::Bt(a)) => Task::done(Message::BtAsk(a)),
                    Some(Preview::Keyring(p)) => {
                        Task::done(Message::KeyringAsk(services::keyring::Ask::Prompt(p)))
                    }
                    None => Task::none(),
                }
            }
            #[cfg(feature = "memtrace")]
            (Some("memdump"), _) => {
                log::info!("memdump: {}", crate::memtrace::dump());
                Task::none()
            }
            (Some("osd"), Some(which @ ("caps" | "num"))) => {
                let is_caps = which == "caps";
                Task::perform(hyprland::locks(), move |(c, n)| {
                    Message::Locks(c, n, is_caps)
                })
            }
            (Some("volume"), Some(arg)) => {
                if let Some(d) = self.audio.sink().cloned() {
                    if arg == "mute" {
                        audio::command(audio::Command::SinkMute(d.name, !d.muted));
                    } else if let Some(step) = match arg {
                        // The volume keys: Settings > Sound's step, on whole steps.
                        "up" => Some(self.config.sound.step.clamp(1, 25) as f32),
                        "down" => Some(-(self.config.sound.step.clamp(1, 25) as f32)),
                        a => a.trim_end_matches('%').parse::<f32>().ok(),
                    } {
                        self.step_volume(step);
                        // The click is heard at the new volume (the output's own level).
                        if self.config.sound.feedback {
                            services::sounds::play(
                                &services::sounds::Sound::Named("audio-volume-change".into()),
                                1.0,
                            );
                        }
                    }
                }
                Task::none()
            }
            (Some("mic"), Some("mute")) => {
                if let Some(d) = self.audio.source().cloned() {
                    audio::command(audio::Command::SourceMute(d.name, !d.muted));
                    self.show_osd(
                        if d.muted { Icon::Mic } else { Icon::MicOff },
                        if d.muted { d.volume.min(1.0) } else { 0.0 },
                    );
                }
                Task::none()
            }
            (Some("brightness"), Some(arg)) => {
                let Ok(step) = arg.trim_end_matches('%').parse::<f32>() else {
                    return Task::none();
                };
                // The laptop screen off (docked): the keys dim the picture instead.
                if let Some(bus) = self.ddc_target() {
                    let cur = self.ddc_level.get(&bus).copied().unwrap_or(0.5);
                    let v = (cur + step / 100.0).clamp(0.0, 1.0);
                    self.ddc_level.insert(bus, v);
                    self.show_osd(Icon::Sun, v);
                    return Task::future(services::ddc::set(bus, v)).discard();
                }
                let laptop = self.outputs.iter().any(|o| o.name.starts_with("eDP"));
                if !laptop || power::brightness().is_none() {
                    let v = (self.dim + step / 100.0).clamp(toggles::MIN_GAMMA as f32 / 100.0, 1.0);
                    self.dim = v;
                    self.show_osd(Icon::Sun, v);
                    return Task::future(toggles::set_dim(v)).discard();
                }
                let cur = self.brightness.or_else(power::brightness).unwrap_or(0.5);
                let v = (cur + step / 100.0).clamp(0.01, 1.0);
                self.brightness = Some(v);
                self.show_osd(Icon::Sun, v);
                Task::future(power::set_brightness(v)).discard()
            }
            _ => {
                log::warn!("ipc: unknown command {cmd}");
                Task::none()
            }
        }
    }
}

impl App {
    /// The highest the volume goes by steps: Settings > Sound's maximum (past 100 % boosts
    /// quiet sound).
    pub(crate) fn volume_max(&self) -> f32 {
        self.config.sound.max.clamp(100, 150) as f32 / 100.0
    }

    /// The output's volume `step` percent up or down, on whole steps up to the maximum; a muted
    /// output is heard again (the volume keys and scrolling on the bar's volume).
    pub(crate) fn step_volume(&mut self, step: f32) {
        if let Some(d) = self.audio.sink().cloned() {
            let v = stepped(d.volume, step, self.volume_max());
            audio::command(audio::Command::SinkVolume(d.name.clone(), d.channels, v));
            if d.muted {
                audio::command(audio::Command::SinkMute(d.name, false));
            }
        }
    }
}

/// Whole notches in a scroll of `lines` (positive = up), with what is left of a notch kept
/// in `acc`: a mouse wheel's notch is one line, a touchpad sends many small fractions of one,
/// each of which was a whole step (a flick ran the volume from quiet to full). Turning back
/// drops what was left the other way.
pub(crate) fn notches(acc: &mut f32, lines: f32) -> i32 {
    if *acc * lines < 0.0 {
        *acc = 0.0;
    }
    *acc += lines;
    let n = acc.trunc();
    *acc -= n;
    n as i32
}

/// The volume one step of `step` percent from `cur`, on whole steps, from silent to `max`. A
/// step never moves the other way: up from a level above `max` (boosted, then the maximum
/// lowered) stays where it is instead of dropping to `max`.
pub(crate) fn stepped(cur: f32, step: f32, max: f32) -> f32 {
    let unit = step.abs().max(0.01) / 100.0;
    let v = (((cur + step / 100.0) / unit).round() * unit).clamp(0.0, max);
    if step > 0.0 { v.max(cur) } else { v.min(cur) }
}

/// Opens a file with its default app.
pub(crate) fn open_file(path: &str) -> Message {
    Message::Dispatch(crate::services::compositor::Action::Open(path.into()))
}

pub(crate) fn game_mode_flag() -> std::path::PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("kestrel-game-mode")
}

/// Records game mode in the runtime directory (cleared at logout), so a restarted shell agrees
/// with Hyprland about whether it is on.
pub fn remember_game_mode(on: bool) {
    let flag = game_mode_flag();
    let r = if on {
        std::fs::write(&flag, b"")
    } else {
        std::fs::remove_file(&flag).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })
    };
    if let Err(e) = r {
        log::warn!("game mode flag {}: {e}", flag.display());
    }
}

#[cfg(test)]
mod tests {
    use super::{notches, stepped};

    #[test]
    fn a_volume_step_never_goes_the_wrong_way() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // Whole steps, up to the maximum.
        assert!(near(stepped(0.5, 5.0, 1.5), 0.55));
        assert!(near(stepped(1.48, 5.0, 1.5), 1.5));
        assert!(near(stepped(0.01, -5.0, 1.5), 0.0));
        // Boosted past the bar's old 100 % cap: up goes on up.
        assert!(near(stepped(1.2, 5.0, 1.5), 1.25));
        // Above a maximum lowered since: up stays, down comes down.
        assert!(near(stepped(1.3, 2.0, 1.0), 1.3));
        assert!(near(stepped(1.3, -2.0, 1.0), 1.0));
    }

    #[test]
    fn small_scrolls_add_up_to_whole_notches() {
        let mut acc = 0.0;
        // A wheel: a step a notch.
        assert_eq!(notches(&mut acc, 1.0), 1);
        assert_eq!(notches(&mut acc, -2.0), -2);
        // A touchpad flick: 3 px a time (iced's pixels, 40 to a line) is no step until a
        // whole line has gone by.
        acc = 0.0;
        let steps: i32 = (0..20).map(|_| notches(&mut acc, 3.0 / 40.0)).sum();
        assert_eq!(steps, 1);
        // Turning back drops what was left the other way: 0.9 up, then 0.5 down is no step.
        acc = 0.0;
        assert_eq!(notches(&mut acc, 0.9), 0);
        assert_eq!(notches(&mut acc, -0.5), 0);
        assert_eq!(notches(&mut acc, -0.5), -1);
    }
}
