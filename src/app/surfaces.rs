//! Surfaces on each output: making them for an output, what each takes (input, keyboard,
//! exclusive zones), the open panel's own surface, the frame and the workspace pill.

use super::*;

impl App {
    pub(crate) fn add_output(&mut self, info: iced::OutputInfo) -> Task<Message> {
        if self.outputs.iter().any(|o| o.id == info.id) {
            return Task::none();
        }
        let (w, h) = info.logical_size.unwrap_or((1920, 1080));
        let size = Size::new(w as f32, h as f32);
        // Strips are sized for the configured frame and stay while it is off (game mode,
        // pills); they draw nothing then.
        let f = self.config.frame;
        let t = f.thickness;
        let out = Some(info.id);
        let mut tasks: Vec<Task<Message>> = Vec::new();
        let mut surfaces = Vec::new();
        let oid = info.id;
        let mut make = |role: Role,
                        s: LayerShellSettings,
                        roles: &mut HashMap<SurfaceId, (OutputId, Role)>,
                        tasks: &mut Vec<Task<Message>>| {
            let (id, task) = new_layer_surface(s);
            roles.insert(id, (oid, role));
            surfaces.push(id);
            tasks.push(task);
            id
        };
        let base = |namespace: &str| LayerShellSettings {
            namespace: namespace.into(),
            output: out,
            exclusive_zone: -1,
            ..LayerShellSettings::default()
        };
        make(
            Role::Wallpaper,
            LayerShellSettings {
                anchor: Anchor::all(),
                layer: Layer::Background,
                ..base("kestrel-wallpaper")
            },
            &mut self.roles,
            &mut tasks,
        );
        // Space reservations: invisible, so drawing never depends on how the compositor
        // orders exclusive zones. Always present; `reserve_zones` sets how much each holds.
        let zones = self.reserve_zones();
        let mut reserves = Vec::new();
        for ((anchor, sz), zone) in [
            (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, 1)),
            (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (1, 0)),
            (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (1, 0)),
            (Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT, (0, 1)),
        ]
        .into_iter()
        .zip(zones)
        {
            let id = make(
                Role::Reserve,
                LayerShellSettings {
                    anchor,
                    exclusive_zone: zone,
                    size: Some(sz),
                    ..base("kestrel-reserve")
                },
                &mut self.roles,
                &mut tasks,
            );
            tasks.push(set_input_region(id, Some(vec![])));
            reserves.push(id);
        }
        for (edge, anchor, sz) in [
            (
                Edge::Left,
                Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                (t as u32, 0),
            ),
            (
                Edge::Right,
                Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
                (t as u32, 0),
            ),
            (
                Edge::Bottom,
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                (0, (t + f.radius) as u32),
            ),
        ] {
            if t <= 0.0 {
                break;
            }
            let id = make(
                Role::Frame(edge),
                LayerShellSettings {
                    anchor,
                    size: Some(sz),
                    ..base("kestrel-frame")
                },
                &mut self.roles,
                &mut tasks,
            );
            tasks.push(set_input_region(id, Some(vec![])));
        }
        // The bar: a strip across the top, tall enough for the frame's corners under it. Never
        // resized (a resize shows the old frame stretched for a moment). Being small, its own
        // motion repaints a strip instead of the whole screen.
        let bar = make(
            Role::Bar,
            LayerShellSettings {
                anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                size: Some((0, BAR_SURFACE as u32)),
                ..base("kestrel")
            },
            &mut self.roles,
            &mut tasks,
        );
        tasks.push(set_input_region(bar, Some(vec![bar_rect(size.width)])));
        // Panels, toasts and pop-ups: the whole output, always (never resized either), above
        // the bar. Input only where something is; idle, it draws and repaints nothing.
        let overlay = make(
            Role::Overlay,
            LayerShellSettings {
                anchor: Anchor::all(),
                size: Some((0, 0)),
                // Above fullscreen apps: a panel opened over one (the overview over a fullscreen
                // window) must show. Banners hold back while a screen is fullscreen instead.
                layer: Layer::Overlay,
                ..base("kestrel-overlay")
            },
            &mut self.roles,
            &mut tasks,
        );
        tasks.push(set_input_region(overlay, Some(vec![])));
        self.outputs.push(Output {
            id: info.id,
            name: info.name.clone(),
            info,
            size,
            overlay,
            surfaces,
            reserves,
            plan: None,
            pill_x: Spring::new(0.0),
            pill_r: Spring::new(0.0),
            island_w: Spring::new(0.0),
        });
        Task::batch(tasks)
    }

    /// The screen frame in effect: none in game mode (a game gets every pixel) and none with
    /// floating pills (nothing is drawn around the screen).
    pub fn frame(&self) -> config::Frame {
        if self.game_mode || self.config.bar.style == config::BarStyle::Pills {
            config::Frame {
                thickness: 0.0,
                radius: 0.0,
            }
        } else {
            self.config.frame
        }
    }

    /// How far panels keep from the screen's sides: flush with the frame, or lined up with the
    /// floating pills (never touching the screen edge).
    pub fn panel_edge(&self) -> f32 {
        if self.config.bar.style == config::BarStyle::Pills {
            crate::shell::bar::PILL_MARGIN
        } else {
            self.frame().thickness
        }
    }

    /// Exclusive zones for the top, left, right and bottom reservations.
    pub(crate) fn reserve_zones(&self) -> [i32; 4] {
        let t = self.frame().thickness as i32;
        let top = if self.config.bar.style == config::BarStyle::Pills {
            // Pills float a few pixels down; windows start just below them.
            shell::bar::PILL_BOTTOM.ceil() as i32
        } else {
            tok::BAR as i32
        };
        [top, t, t, t]
    }

    /// The frame in effect changed (game mode, bar style): move the reserved space and redraw
    /// the strips, without recreating any surface (that flashed the whole screen).
    pub fn apply_frame(&mut self) -> Task<Message> {
        let zones = self.reserve_zones();
        let mut tasks = Vec::new();
        for o in &self.outputs {
            for (id, zone) in o.reserves.iter().zip(zones) {
                tasks.push(set_exclusive_zone(*id, zone));
            }
        }
        self.update_pills(true);
        tasks.push(self.sync_surfaces());
        Task::batch(tasks)
    }

    /// Recreates every output's surfaces (only when the configured frame size changes).
    pub fn rebuild_outputs(&mut self) -> Task<Message> {
        let infos: Vec<iced::OutputInfo> = self.outputs.iter().map(|o| o.info.clone()).collect();
        let mut tasks = Vec::new();
        for info in infos {
            tasks.push(self.remove_output(info.id));
            tasks.push(self.add_output(info));
        }
        self.update_pills(true);
        Task::batch(tasks)
    }

    pub(crate) fn remove_output(&mut self, id: OutputId) -> Task<Message> {
        let Some(i) = self.outputs.iter().position(|o| o.id == id) else {
            return Task::none();
        };
        let o = self.outputs.remove(i);
        let mut gone = Task::none();
        if self.panel.as_ref().is_some_and(|p| p.output == id) {
            self.panel = None;
            gone = self.drop_panel_surface();
        }
        Task::batch(std::iter::once(gone).chain(o.surfaces.into_iter().map(|s| {
            self.roles.remove(&s);
            destroy_layer_surface(s)
        })))
    }

    /// The output where toasts and the OSD appear: the focused one.
    pub fn focused_output(&self) -> Option<OutputId> {
        self.outputs
            .iter()
            .find(|o| o.name == self.hypr.focused_monitor)
            .or(self.outputs.first())
            .map(|o| o.id)
    }

    /// The overlay's input and keyboard for what it shows: everything below the bar while a
    /// panel is open (a click outside closes it; the bar's own pills still switch panels), the
    /// toasts otherwise, and nothing when idle so the screen stays usable.
    pub(crate) fn sync_surfaces(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        // A closing panel gives the keyboard and the screen back at once, not when its
        // animation ends, so the app you return to responds immediately.
        let panel = self
            .panel
            .as_ref()
            .filter(|p| p.open)
            .map(|p| (p.output, p.kind));
        let focused = self.focused_output();
        let keys = self.panel_keys;
        let self_typing = &self.panels.control;
        let own = self.config.bar.style == config::BarStyle::Pills;
        // The keyboard a panel on its own surface takes (given to that surface, below).
        let mut panel_keyboard: Option<u8> = None;
        let card_h = self.toast_sheet.natural;
        for o in &mut self.outputs {
            let plan = if let Some((_, kind)) = panel.filter(|(id, _)| *id == o.id) {
                // Taking the keyboard makes the app you are using lose focus (it greys out).
                // So a panel takes it only when it is for typing, or was opened from the
                // keyboard (a shortcut: Esc and the arrows are expected there). Opened with
                // the mouse, it leaves your app focused; a click outside closes it.
                let typing = kind.takes_keyboard()
                    || (kind == PanelKind::Control && panels::control::typing(self_typing));
                let keyboard = if typing || keys { 2 } else { 0 };
                if own {
                    panel_keyboard = Some(keyboard);
                }
                Plan {
                    region: Some(vec![(
                        0,
                        tok::BAR as i32,
                        o.size.width as i32,
                        (o.size.height - tok::BAR) as i32,
                    )]),
                    // A panel on its own surface takes the keyboard there (its text fields
                    // live there); the overlay behind it only catches clicks.
                    keyboard: if own { 0 } else { keyboard },
                }
            } else {
                let mut region = Vec::new();
                // Toasts take clicks (to open or dismiss them); the OSD never does.
                // Where the island grows into the card (shell::bar: centred, from the pill's top);
                // it stayed at the old top-right banner's place, so clicks on the card fell
                // through to the window behind.
                if focused == Some(o.id) && self.toast_sheet.visible() {
                    let (x, y, w, h) = shell::bar::card_rect(card_h, o.size.width);
                    region.push((x as i32, y as i32, w as i32, h.ceil() as i32));
                }
                Plan {
                    region: Some(region),
                    keyboard: 0,
                }
            };
            if o.plan.as_ref() == Some(&plan) {
                continue;
            }
            log::debug!("surface plan for {}: {:?}", o.name, plan);
            let old = o.plan.replace(plan.clone());
            if old.as_ref().map(|p| &p.region) != Some(&plan.region) {
                tasks.push(set_input_region(
                    o.overlay,
                    plan.region.as_ref().map(|r| {
                        r.iter()
                            .map(|&(x, y, width, height)| InputRegionRect {
                                x,
                                y,
                                width,
                                height,
                            })
                            .collect()
                    }),
                ));
            }
            if old.as_ref().map(|p| p.keyboard) != Some(plan.keyboard) {
                tasks.push(set_keyboard_interactivity(
                    o.overlay,
                    match plan.keyboard {
                        2 => KeyboardInteractivity::Exclusive,
                        1 => KeyboardInteractivity::OnDemand,
                        _ => KeyboardInteractivity::None,
                    },
                ));
            }
        }
        if let (Some(k), Some((id, kind, given))) = (panel_keyboard, self.panel_surface)
            && given & !FRESH != k
        {
            self.panel_surface = Some((id, kind, k | (given & FRESH)));
            tasks.push(set_keyboard_interactivity(
                id,
                match k {
                    2 => KeyboardInteractivity::Exclusive,
                    1 => KeyboardInteractivity::OnDemand,
                    _ => KeyboardInteractivity::None,
                },
            ));
        }
        Task::batch(tasks)
    }

    /// Panels have surfaces of their own in the floating style (the full style joins them to
    /// the bar's band, drawn on the overlay).
    pub fn own_panel_surfaces(&self) -> bool {
        self.config.bar.style == config::BarStyle::Pills
    }

    /// Where the open panel's own surface goes and how big it is: (x, top, width, height).
    pub(crate) fn panel_rect(&self) -> Option<(OutputId, f32, f32, f32, f32)> {
        let p = self.panel.as_ref().filter(|p| p.open)?;
        let o = self.output(p.output)?;
        let (x, w) = shell::bar::panel_span(self, p, o.size.width, self.panel_edge());
        let h = p
            .kind
            .height_for(self)
            .or_else(|| self.natural.get(&p.kind).map(|s| s.height))
            .unwrap_or(p.h.value)
            .max(1.0);
        Some((p.output, x, tok::BAR + crate::ui::shape::FLOAT_GAP, w, h))
    }

    /// The open panel on a surface of its own, made (or moved and sized) to fit it: created
    /// on open, it is mapped once, so Hyprland's layer animation plays with its blur.
    pub(crate) fn sync_panel_surface(&mut self) -> Task<Message> {
        if !self.own_panel_surfaces() {
            return self.drop_panel_surface();
        }
        let Some((output, x, top, w, h)) = self.panel_rect() else {
            return self.drop_panel_surface();
        };
        let kind = self.panel.as_ref().map(|p| p.kind);
        let size = (w.ceil() as u32, h.ceil() as u32);
        let margin = (top.round() as i32, 0, 0, x.round() as i32);
        match self.panel_surface {
            Some((id, k, _))
                if Some(k) == kind && self.roles.get(&id).map(|r| r.0) == Some(output) =>
            {
                // Told only when its place or size really changes (this runs after every
                // update).
                if self.panel_geom == Some((size, margin)) {
                    return Task::none();
                }
                self.panel_geom = Some((size, margin));
                Task::batch([set_size(id, size), set_margin(id, margin)])
            }
            _ => {
                let gone = self.drop_panel_surface();
                self.panel_geom = Some((size, margin));
                // The keyboard it needs from the start (set after creation it raced the
                // surface being made, and typing went nowhere).
                let k = self.panel_keyboard_mode();
                let (id, task) = new_layer_surface(LayerShellSettings {
                    anchor: Anchor::TOP | Anchor::LEFT,
                    layer: Layer::Overlay,
                    keyboard_interactivity: match k {
                        2 => KeyboardInteractivity::Exclusive,
                        1 => KeyboardInteractivity::OnDemand,
                        _ => KeyboardInteractivity::None,
                    },
                    exclusive_zone: -1,
                    size: Some(size),
                    margin,
                    namespace: "kestrel-panel".into(),
                    output: Some(output),
                });
                self.roles.insert(id, (output, Role::Panel));
                if let Some(kd) = kind {
                    self.panel_surface = Some((id, kd, k | FRESH));
                }
                Task::batch([gone, task])
            }
        }
    }

    /// The keyboard the open panel needs: 2 exclusive (typing panels, or opened from a key), 0
    /// none (opened with the mouse: your app keeps its focus).
    pub(crate) fn panel_keyboard_mode(&self) -> u8 {
        let Some(kind) = self.panel.as_ref().filter(|p| p.open).map(|p| p.kind) else {
            return 0;
        };
        let typing = kind.takes_keyboard()
            || (kind == PanelKind::Control && panels::control::typing(&self.panels.control));
        if typing || self.panel_keys { 2 } else { 0 }
    }

    /// The panel's own surface goes (Hyprland plays its closing animation).
    pub(crate) fn drop_panel_surface(&mut self) -> Task<Message> {
        self.panel_geom = None;
        match self.panel_surface.take() {
            Some((id, _, _)) => {
                self.roles.remove(&id);
                destroy_layer_surface(id)
            }
            None => Task::none(),
        }
    }

    pub fn update_pills(&mut self, snap: bool) {
        let n = self.config.workspaces;
        for o in &mut self.outputs {
            let Some(active) = self.hypr.active_on(&o.name) else {
                continue;
            };
            let (x, w) = shell::bar::pill_geometry(&self.hypr, &o.name, active, n);
            let r = x + w;
            if snap || o.pill_r.value == 0.0 {
                o.pill_x.snap(x);
                o.pill_r.snap(r);
            } else if (x - o.pill_x.target).abs() > 0.5 {
                // ii's stretch: the edge facing the move leads, the other follows, so the
                // circle reaches out and gathers itself. Both critically damped: no overshoot.
                // Settled in ~0.11 s / ~0.15 s (was 0.17 / 0.28: it trailed a fast scroll, the user's
                // "sluggish", 2026-11-15).
                const LEAD: (f32, f32) = (60.0, 1.0);
                const TRAIL: (f32, f32) = (45.0, 1.0);
                let right = x > o.pill_x.target;
                o.pill_r.to(r, if right { LEAD } else { TRAIL });
                o.pill_x.to(x, if right { TRAIL } else { LEAD });
            }
        }
        self.clock.reset();
    }

    pub fn output(&self, id: OutputId) -> Option<&Output> {
        self.outputs.iter().find(|o| o.id == id)
    }
}

pub(crate) fn bar_rect(width: f32) -> InputRegionRect {
    InputRegionRect {
        x: 0,
        y: 0,
        width: width as i32,
        height: tok::BAR as i32,
    }
}

/// Centre a panel under its button. Near a screen edge it merges into the frame instead,
/// so there is never a sliver of wallpaper between panel and frame.
/// Where a panel of width `w` hangs under a button centred at `center`: centred on it, or held
/// against the edge (`edge` px in: the frame, or the floating pills' margin) when it would
/// come close to the screen's side.
pub(crate) fn place(center: f32, w: f32, screen_w: f32, edge: f32) -> f32 {
    let margin = edge + tok::JOIN + 24.0;
    let x = center - w / 2.0;
    if x < margin {
        edge
    } else if x + w > screen_w - margin {
        screen_w - edge - w
    } else {
        x
    }
}
