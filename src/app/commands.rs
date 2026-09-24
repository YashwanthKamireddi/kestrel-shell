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
            // Settings is a window of its own: opened, or brought forward if open.
            (Some("toggle" | "open"), Some("settings")) => self.open_settings(),
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
                    | PanelKind::Settings
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
                        // As high as Settings > Sound allows (past 100 % boosts quiet sound).
                        let max = self.config.sound.max.clamp(100, 150) as f32 / 100.0;
                        let unit = step.abs() / 100.0;
                        let v = (((d.volume + step / 100.0) / unit).round() * unit).clamp(0.0, max);
                        audio::command(audio::Command::SinkVolume(d.name.clone(), d.channels, v));
                        if d.muted {
                            audio::command(audio::Command::SinkMute(d.name, false));
                        }
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
