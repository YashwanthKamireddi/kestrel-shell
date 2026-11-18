//! What the shell says: notifications arriving, kestrel's own entries, the island's card,
//! levels and events (the OSD), flashes (copied, recorded) and their expiry.

use super::*;

impl App {
    /// Something kestrel itself did (a recording saved, an image copied), kept in the
    /// notification list so it can be found and opened later. The island already said it, so
    /// no banner and no unread count. Ids count down from the top, apart from the apps' ones.
    pub fn remember(
        &mut self,
        app_icon: &str,
        summary: String,
        body: String,
        image: Option<notifications::Image>,
        actions: Vec<(String, String)>,
    ) {
        let id = own_id();
        self.notifications.push(notifications::Notification {
            id,
            app: "kestrel".into(),
            app_icon: app_icon.into(),
            summary,
            body,
            actions,
            image,
            urgency: 1,
            timeout: 0,
            time: std::time::SystemTime::now(),
            sound: services::sounds::Sound::Silent,
            replaced: false,
        });
        if self.notifications.len() > 50 {
            self.notifications.remove(0);
        }
    }

    pub(crate) fn notify(&mut self, e: notifications::Event) -> Task<Message> {
        match e {
            notifications::Event::Added(n) => {
                // Remembered for Settings, then the app's rule: muted ones are dropped.
                if !n.app.is_empty() && !crate::state::get(|s| s.notifiers.contains(&n.app)) {
                    let app = n.app.clone();
                    crate::state::update(|s| {
                        s.notifiers.insert(app);
                    });
                }
                let rules = &self.config.notifications;
                if rules.muted.contains(&n.app) {
                    return Task::none();
                }
                let quiet = rules.quiet.contains(&n.app);
                let silent = !rules.sound || rules.silent.contains(&n.app) || n.replaced;
                let urgent_in_dnd = rules.urgent_in_dnd;
                let banner_secs = rules.banner.clamp(2, 30);
                let sound = n.sound.clone();
                crate::lua::event(
                    "notification",
                    vec![
                        ("app", crate::lua::Val::Str(n.app.clone())),
                        ("summary", crate::lua::Val::Str(n.summary.clone())),
                        ("body", crate::lua::Val::Str(n.body.clone())),
                    ],
                );
                let id = n.id;
                let urgent = n.urgency >= 2;
                let timeout = n.timeout;
                // An update to one already listed (a download's progress, a changed message)
                // changes it where it is (Noctalia); anything else goes on top.
                let replaced = n.replaced;
                match self.notifications.iter_mut().find(|x| x.id == id) {
                    Some(old) if replaced => *old = *n,
                    _ => {
                        self.notifications.retain(|x| x.id != id);
                        self.notifications.push(*n);
                    }
                }
                if self.notifications.len() > 50 {
                    self.notifications.remove(0);
                }
                let centre_open = self
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.open && p.kind == PanelKind::Notifications);
                if !centre_open {
                    self.unread = true;
                    self.unseen += 1;
                }
                // A fullscreen app (a video, a game) is not interrupted by banners; they wait in
                // the list. Urgent ones still show.
                let fullscreen = self.hypr.fullscreen.contains(&self.hypr.focused_monitor);
                let through = !self.dnd || (urgent && urgent_in_dnd);
                let heard = through && (!quiet && !fullscreen || urgent);
                // The sound goes with what would show (with the centre open too: the list
                // takes it there); never for quiet or muted apps, in Do not disturb, or over a
                // fullscreen app.
                if heard && !silent {
                    services::sounds::play(&sound, self.config.sound.alert_volume());
                }
                if heard && !centre_open {
                    let secs = if timeout > 0 {
                        (timeout as u64 / 1000).clamp(2, 30)
                    } else {
                        banner_secs
                    };
                    self.toasts.retain(|t| t.id != id);
                    let secs = Duration::from_secs(if urgent { 3600 } else { secs });
                    self.toast_last = Some(id);
                    self.toasts.push(Toast {
                        id,
                        until: Instant::now() + secs,
                        secs,
                    });
                    if self.toasts.len() > 3 {
                        self.toasts.remove(0);
                    }
                    self.open_sheet_toasts();
                }
            }
            notifications::Event::Closed(id) => {
                self.notifications.retain(|n| n.id != id);
                self.toasts.retain(|t| t.id != id);
            }
        }
        self.notes_changed();
        self.settle_toasts();
        Task::none()
    }

    /// After every change to the list, however it came (an app, a panel, the 50 limit): the
    /// panel forgets what was about ones now gone.
    fn notes_changed(&mut self) {
        self.panels.notifications.keep(&self.notifications);
    }

    /// A change to the list asked by a panel or a banner (`panels::action::Note`).
    pub(crate) fn note(&mut self, n: crate::panels::action::Note) -> Task<Message> {
        use crate::panels::action::Note;
        let task = match n {
            Note::Dismiss(id) => self.dismiss_notes(|n| n.id == id),
            Note::DismissApp(app) => self.dismiss_notes(|n| n.app == app),
            Note::ClearAll => self.dismiss_notes(|_| true),
            // Kestrel's own entries: open a file or folder, copy it, or save a copied picture.
            Note::Invoke(id, key) if key.starts_with("kestrel-") => {
                self.toasts.retain(|t| t.id != id);
                self.own_action(id, &key)
            }
            Note::Invoke(id, key) => {
                self.notifications.retain(|n| n.id != id);
                self.toasts.retain(|t| t.id != id);
                Task::future(notifications::invoke(id, key)).discard()
            }
            Note::HideBanner(id) => {
                self.toasts.retain(|t| t.id != id);
                Task::none()
            }
            Note::HoldBanner(id, on) => {
                if on {
                    self.toast_hover = Some(id);
                } else if self.toast_hover == Some(id) {
                    self.toast_hover = None;
                    // Its time starts again in full once the pointer leaves (DMS restarts it).
                    if let Some(t) = self.toasts.iter_mut().find(|t| t.id == id) {
                        t.until = Instant::now() + t.secs;
                    }
                }
                Task::none()
            }
        };
        self.notes_changed();
        // The last one gone while the list is open: the list closes at once, in this same
        // update (closing a frame later drew it empty first: a flicker).
        let open = self
            .panel
            .as_ref()
            .is_some_and(|p| p.open && p.kind == PanelKind::Notifications);
        if self.notifications.is_empty() && open {
            return Task::batch([task, self.close()]);
        }
        task
    }

    /// Takes away the notifications `which` picks, and their banners; their apps are told.
    fn dismiss_notes(
        &mut self,
        which: impl Fn(&notifications::Notification) -> bool,
    ) -> Task<Message> {
        let ids: Vec<u32> = self
            .notifications
            .iter()
            .filter(|n| which(n))
            .map(|n| n.id)
            .collect();
        self.notifications.retain(|n| !which(n));
        self.toasts.retain(|t| !ids.contains(&t.id));
        Task::batch(ids.into_iter().map(|id| {
            Task::future(notifications::closed(id, notifications::Reason::Dismissed)).discard()
        }))
    }

    /// A button on kestrel's own notification (a screenshot's, a recording's).
    fn own_action(&mut self, id: u32, key: &str) -> Task<Message> {
        if let Some(path) = key.strip_prefix("kestrel-open:") {
            return Task::done(Message::Dispatch(services::compositor::Action::Open(
                path.into(),
            )));
        }
        if let Some(path) = key.strip_prefix("kestrel-copy:") {
            copy_file(path);
        } else if let Some(path) = key.strip_prefix("kestrel-save:")
            && let Some(saved) = save_picture(path)
            // The entry now points at the saved file.
            && let Some(n) = self.notifications.iter_mut().find(|n| n.id == id)
        {
            n.body = format!(
                "Saved in {}",
                saved
                    .parent()
                    .map_or(String::new(), |p| p.display().to_string())
            );
            n.actions = vec![(format!("kestrel-open:{}", saved.display()), "Open".into())];
        }
        Task::none()
    }

    /// Low battery: warned once as UPower's level rises on battery (low, then critical, urgent),
    /// with what is left; the warning goes when the charger is plugged in.
    pub(crate) fn battery_warning(&mut self, s: &power::State) {
        if !s.has_battery {
            return;
        }
        if !s.on_battery {
            if let Some(id) = self.battery_note.take() {
                self.notify(notifications::Event::Closed(id));
            }
            return;
        }
        let before = if self.power.on_battery {
            self.power.warning
        } else {
            1
        };
        if s.warning <= before || s.warning < 3 {
            return;
        }
        let left = if s.time_left > 0 {
            format!(
                "{:.0}% left, about {}",
                s.percent,
                panels::control::duration(s.time_left)
            )
        } else {
            format!("{:.0}% left", s.percent)
        };
        let (summary, body, urgency) = match s.warning {
            3 => (
                "Battery low",
                format!("{left}. Plug in the charger soon."),
                1,
            ),
            _ => (
                "Battery almost empty",
                format!("{left}. Plug in the charger now: the laptop will stop by itself."),
                2,
            ),
        };
        let mut n = own_notification(summary, &body);
        n.urgency = urgency;
        n.app_icon = "battery-caution".into();
        n.sound = services::sounds::Sound::Named("dialog-warning".into());
        // One warning at a time: the critical one replaces the low one.
        if let Some(old) = self.battery_note.take() {
            self.notify(notifications::Event::Closed(old));
        }
        self.battery_note = Some(n.id);
        let _ = self.notify(notifications::Event::Added(n));
    }

    pub(crate) fn open_sheet_toasts(&mut self) {
        // The island grows into it (a dynamic island): from the pill's own size to the
        // banner's, height and width together. Already grown (a second one), it stays.
        let grown = self.toast_w.target >= TOAST_WIDTH - 1.0;
        // From the pill only when it is the pill: one arriving while the island is still
        // shrinking back grows again from where it is (it jumped to the pill's size first).
        if !grown && self.toast_w.value < 1.0 {
            let pill_w = self
                .focused_output()
                .and_then(|o| self.output(o))
                .map_or(120.0, |o| o.island_w.value + 2.0 * crate::theme::tok::L);
            self.toast_w.snap(pill_w);
            self.toast_sheet.h.snap(tok::BAR - 6.0);
        }
        self.toast_w.to(TOAST_WIDTH, Spring::OPEN);
        if self.toast_sheet.natural > 0.0 {
            self.toast_sheet
                .h
                .to(self.toast_sheet.natural, Spring::OPEN);
        }
        self.clock.reset();
    }

    /// Toasts gone → close the sheet.
    pub(crate) fn settle_toasts(&mut self) {
        if self.toasts.is_empty() && self.toast_w.target >= TOAST_WIDTH - 1.0 {
            // Back into the pill, smoothly (the island shrinking, as on the iPhone); once it is
            // the pill's size again it is gone (`tick`).
            let pill_w = self
                .focused_output()
                .and_then(|o| self.output(o))
                .map_or(120.0, |o| o.island_w.value + 2.0 * crate::theme::tok::L);
            self.toast_w.to(pill_w, Spring::CLOSE);
            self.toast_sheet.h.to(tok::BAR - 6.0, Spring::CLOSE);
            self.clock.reset();
        }
    }

    pub fn show_osd(&mut self, icon: Icon, value: f32) {
        // The control centre already shows the level being changed.
        if self.panel.as_ref().is_some_and(|p| p.open) {
            return;
        }
        self.osd = Some(Osd {
            icon,
            value,
            text: None,
            sub: None,
            until: Instant::now() + Duration::from_millis(OSD_MS),
            // A click opens the control centre, where the level is set.
            action: Some(Message::Ipc("toggle control".into())),
        });
        if self.osd_sheet.natural > 0.0 {
            self.osd_sheet.h.snap(self.osd_sheet.natural);
        }
        self.clock.reset();
    }

    /// An event rather than a level: a title and a line under it (a device connecting, the
    /// charger). Shown longer than a level change, since it has more to read.
    pub fn show_event(&mut self, icon: Icon, title: String, sub: String) {
        self.osd = Some(Osd {
            icon,
            value: 0.0,
            text: Some(title),
            // No second line when there is nothing to say on it.
            sub: (!sub.is_empty()).then_some(sub),
            until: Instant::now() + Duration::from_millis(OSD_MS * 2),
            action: Some(Message::Ipc("toggle control".into())),
        });
        if self.osd_sheet.natural > 0.0 {
            self.osd_sheet.h.snap(self.osd_sheet.natural);
        }
        self.clock.reset();
    }

    /// Takes the OSD away now (before a screenshot it would be in).
    pub fn hide_osd(&mut self) {
        self.osd = None;
        self.osd_sheet.h.snap(0.0);
        self.clock.reset();
    }

    pub(crate) fn expire(&mut self, now: Instant) {
        if self.flash.as_ref().is_some_and(|f| f.until <= now) {
            self.flash = None;
        }
        let before = self.toasts.len();
        // The one under the pointer stays (DMS stops its time while hovered).
        let held = self.toast_hover;
        self.toasts.retain(|t| t.until > now || held == Some(t.id));
        if self.toasts.len() != before {
            self.settle_toasts();
        }
        // Shown in the island: nothing to animate away, just drop it.
        if crate::modules::island::hosts_osd(self)
            && self.osd.as_ref().is_some_and(|o| o.until <= now)
        {
            self.osd = None;
        }
        if self.osd.as_ref().is_some_and(|o| o.until <= now) && self.osd_sheet.h.target > 0.0 {
            self.osd_sheet.h.snap(0.0);
            self.clock.reset();
        }
    }

    /// Something was copied: say so in the island.
    pub(crate) fn copied(&mut self, c: services::clipboard::Copied) {
        use services::clipboard::Copied;
        let mut action = Some(Message::Ipc("toggle clipboard".into()));
        let (icon, title, detail, thumb) = match c {
            // Just "Copied" (his call 2026-11-13): the copied text itself was noise.
            Copied::Text(_) => (Icon::Clipboard, "Copied".to_string(), String::new(), None),
            // Words only: what it was, no picture or size.
            Copied::Image {
                width,
                height,
                file,
                ..
            } => {
                // A capture the size of a whole screen is a screenshot.
                let full = self
                    .outputs
                    .iter()
                    .any(|o| (o.size.width as u32, o.size.height as u32) == (width, height));
                // A screenshot (the screen was captured just before) is kept in the list with
                // its picture: a click opens it, "Save" puts it in Pictures/Screenshots. Other
                // copied pictures are not (every copied image there was clutter).
                let shot = self
                    .hypr
                    .captured_at
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d < Duration::from_secs(4));
                if (full || shot) && self.config.sound.screenshot {
                    services::sounds::play(
                        &services::sounds::Sound::Named("screen-capture".into()),
                        self.config.sound.alert_volume(),
                    );
                }
                if shot && let Some(f) = file {
                    // A click on "Screenshot copied" opens it.
                    action = Some(open_file(&f));
                    self.remember(
                        "camera",
                        "Screenshot".into(),
                        String::new(),
                        Some(notifications::Image::Path(f.clone())),
                        vec![
                            (format!("kestrel-open:{f}"), "Open".into()),
                            (format!("kestrel-copy:{f}"), "Copy".into()),
                            (format!("kestrel-save:{f}"), "Save".into()),
                        ],
                    );
                }
                (
                    if full || shot {
                        Icon::Camera
                    } else {
                        Icon::Image
                    },
                    if full || shot {
                        "Screenshot copied"
                    } else {
                        "Image copied"
                    }
                    .to_string(),
                    String::new(),
                    None,
                )
            }
            Copied::Other => (Icon::Clipboard, "Copied".to_string(), String::new(), None),
        };
        self.flash = Some(Flash {
            icon,
            title,
            detail,
            thumb,
            until: Instant::now() + Duration::from_millis(FLASH_MS),
            action,
        });
        self.clock.reset();
    }
}

/// A notification from kestrel itself (ids from the top of the range, clear of the server's).
pub(crate) fn own_notification(summary: &str, body: &str) -> Box<notifications::Notification> {
    Box::new(notifications::Notification {
        id: own_id(),
        app: "kestrel".into(),
        app_icon: String::new(),
        summary: summary.into(),
        body: body.into(),
        actions: Vec::new(),
        image: None,
        urgency: 1,
        timeout: -1,
        time: std::time::SystemTime::now(),
        sound: services::sounds::Sound::Default,
        replaced: false,
    })
}

/// Ids for kestrel's own notifications: one counter from the top of the range (clear of the
/// server's, which count up), shared by every kind so two never get the same id.
pub(crate) fn own_id() -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
    NEXT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
}

/// The icon for a BlueZ device class name.
pub(crate) fn device_icon(bluez: &str) -> Icon {
    match bluez {
        "audio-headset" | "audio-headphones" => Icon::Headphones,
        "audio-card" | "audio-speakers" => Icon::Speaker,
        "input-mouse" | "input-tablet" => Icon::Mouse,
        "input-keyboard" => Icon::Keyboard,
        "input-gaming" => Icon::Gamepad2,
        "phone" => Icon::Smartphone,
        _ if bluez.contains("watch") => Icon::Watch,
        _ => Icon::Bluetooth,
    }
}

/// Puts a file on the clipboard: a picture as the picture itself (pastes into any chat or
/// editor), anything else (a recording) as the file (pastes into a chat, a folder, an upload).
fn copy_file(path: &str) {
    let p = std::path::Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase);
    let picture = match ext.as_deref() {
        Some("png") => Some("image/png"),
        Some("webp") => Some("image/webp"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        _ => None,
    };
    let (mime, data) = match picture {
        Some(mime) => match std::fs::read(p) {
            Ok(d) => (mime, d),
            Err(_) => return,
        },
        None => (
            "text/uri-list",
            format!("{}\r\n", services::filechooser::uri(p)).into_bytes(),
        ),
    };
    services::clipboard::copy(&[mime], data);
}

/// Copies a kept picture into Pictures/Screenshots under a dated name.
fn save_picture(from: &str) -> Option<std::path::PathBuf> {
    let dir = services::xdg::user_dir("PICTURES").join("Screenshots");
    std::fs::create_dir_all(&dir).ok()?;
    let ext = std::path::Path::new(from)
        .extension()
        .map_or("png".into(), |e| e.to_string_lossy().into_owned());
    let to = dir.join(format!(
        "Screenshot_{}.{ext}",
        chrono::Local::now().format("%Y-%m-%d_%H.%M.%S")
    ));
    std::fs::copy(from, &to).ok()?;
    Some(to)
}
