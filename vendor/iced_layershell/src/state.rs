//! Central Wayland state management.
//!
//! This module defines the core state structures and provides
//! methods for surface management. Protocol handlers are in
//! the [`handlers`](crate::handlers) module.

use std::collections::{HashMap, HashSet};

use calloop::LoopHandle;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::data_device_manager::data_device::DataDevice;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{LayerShell, LayerSurface};
use wayland_client::protocol::wl_keyboard::WlKeyboard;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::wl_touch::WlTouch;
use wayland_client::{Connection, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};

use crate::settings::{OutputEvent, OutputId, OutputInfo, SurfaceId};

/// What a surface is to the compositor: a layer surface (bar, overlay) or, kestrel patch, an
/// ordinary window (xdg-toplevel) that can be moved, tiled and closed like any app.
pub(crate) enum Role {
    Layer(LayerSurface),
    Window(smithay_client_toolkit::shell::xdg::window::Window),
    /// kestrel patch: a session lock surface (ext-session-lock-v1), one per output while the
    /// session is locked. Never committed without a buffer: the compositor treats that as a
    /// protocol error and drops the client (the session then stays locked, unusable).
    Lock(smithay_client_toolkit::session_lock::SessionLockSurface),
}

impl Role {
    pub fn wl_surface(&self) -> &WlSurface {
        match self {
            Role::Layer(l) => l.wl_surface(),
            Role::Window(w) => w.wl_surface(),
            Role::Lock(l) => l.wl_surface(),
        }
    }
    pub fn layer(&self) -> Option<&LayerSurface> {
        match self {
            Role::Layer(l) => Some(l),
            Role::Window(_) | Role::Lock(_) => None,
        }
    }
    pub fn is_lock(&self) -> bool {
        matches!(self, Role::Lock(_))
    }
}

/// Per-surface Wayland data.
pub(crate) struct SurfaceData {
    pub id: SurfaceId,
    pub role: Role,
    /// A window's size before the compositor picks one (layer surfaces: unused).
    pub default_size: (u32, u32),
    pub size: (u32, u32),
    pub scale_factor: i32,
    pub configured: bool,
    pub frame_pending: bool,
    /// Set when we drew but couldn't present (`frame_pending` was true).
    /// Frame callback will trigger a redraw to flush the pending visual.
    pub needs_rerender: bool,
    /// Output the surface is currently mapped on, set from `wl_surface.enter`
    /// events. `None` until the compositor maps the surface (or after a
    /// `wl_surface.leave` if not re-entered).
    pub current_output: Option<OutputId>,
    /// `ext_background_effect_surface_v1` proxy, created on first use.
    pub bg_effect_surface: Option<ExtBackgroundEffectSurfaceV1>,
    /// Last blur region pushed, so we only re-send when it changes. `None` until
    /// the first push, which has to happen even when the region is empty.
    pub blur_region: Option<Vec<crate::task_impl::BlurRect>>,
    /// Scale the last presented buffer was committed at. While it trails
    /// `scale_factor` a `set_buffer_scale` is still waiting for its buffer, and
    /// committing the surface would tag the one still attached with the new
    /// scale — a protocol error as soon as its size is not divisible by it.
    pub committed_scale_factor: i32,
}

/// Central Wayland state, holding all SCTK protocol states and event queues.
pub(crate) struct WaylandState {
    pub registry: RegistryState,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    /// xdg-shell, for ordinary windows (kestrel patch); None if the compositor has none.
    pub xdg_shell: Option<smithay_client_toolkit::shell::xdg::XdgShell>,
    /// ext-session-lock-v1's manager (kestrel patch); asking it for a lock fails cleanly where
    /// the compositor has none.
    pub session_lock_state: Option<smithay_client_toolkit::session_lock::SessionLockState>,
    /// The session lock this client holds, from the request until it is unlocked or finished.
    pub session_lock: Option<smithay_client_toolkit::session_lock::SessionLock>,
    /// The connection, to flush an unlock before the process ends (kestrel patch).
    pub conn: Connection,
    pub seat: SeatState,
    pub output: OutputState,

    // Surface tracking
    pub surfaces: HashMap<WlSurface, SurfaceData>,
    pub surface_id_map: HashMap<SurfaceId, WlSurface>,

    // Input state
    pub cursor_position: iced_core::Point,
    pub pointer_surface: Option<SurfaceId>,
    pub keyboard_focus: Option<SurfaceId>,
    pub wl_keyboard: Option<WlKeyboard>,
    pub wl_touch: Option<WlTouch>,
    pub modifiers: iced_core::keyboard::Modifiers,

    // Event queues (drained each frame by the application runner)
    pub pending_events: Vec<(SurfaceId, iced_core::Event)>,
    pub output_events: Vec<OutputEvent>,
    pub surfaces_need_redraw: HashSet<SurfaceId>,
    pub closed_surfaces: Vec<SurfaceId>,

    // Output tracking
    pub outputs: HashMap<WlOutput, OutputInfo>,
    next_output_id: u32,

    // Clipboard / data device
    pub data_device_manager: DataDeviceManagerState,
    pub data_device: Option<DataDevice>,

    // Touch state: track active finger positions for cancel events
    pub touch_fingers: HashMap<i32, (SurfaceId, iced_core::Point)>,
    // Finger IDs to remove from touch_fingers after event processing.
    // Deferred so scaled_cursor() still sees the finger during FingerLifted.
    pub pending_finger_removals: Vec<i32>,

    // Cursor shape
    pub cursor_shape_manager: Option<CursorShapeManager>,
    pub current_mouse_interaction: iced_core::mouse::Interaction,
    pub pointer_enter_serial: u32,
    pub wl_pointer: Option<WlPointer>,

    // ext-background-effect-v1 (compositor-side blur regions)
    pub bg_effect_manager: Option<ExtBackgroundEffectManagerV1>,
    pub bg_effect_supports_blur: bool,

    // Calloop loop handle for keyboard repeat
    pub loop_handle: LoopHandle<'static, WaylandState>,

    // Raw display pointer for raw-window-handle
    pub display_ptr: *mut std::ffi::c_void,
}

impl WaylandState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: RegistryState,
        compositor: CompositorState,
        layer_shell: LayerShell,
        seat: SeatState,
        output: OutputState,
        data_device_manager: DataDeviceManagerState,
        loop_handle: LoopHandle<'static, WaylandState>,
        conn: &Connection,
    ) -> Self {
        let display_ptr = conn.backend().display_ptr().cast::<std::ffi::c_void>();

        Self {
            registry,
            compositor,
            layer_shell,
            xdg_shell: None,
            session_lock_state: None,
            session_lock: None,
            conn: conn.clone(),
            seat,
            output,
            surfaces: HashMap::new(),
            surface_id_map: HashMap::new(),
            cursor_position: iced_core::Point::ORIGIN,
            pointer_surface: None,
            keyboard_focus: None,
            wl_keyboard: None,
            wl_touch: None,
            modifiers: iced_core::keyboard::Modifiers::empty(),
            pending_events: Vec::new(),
            output_events: Vec::new(),
            surfaces_need_redraw: HashSet::new(),
            closed_surfaces: Vec::new(),
            outputs: HashMap::new(),
            next_output_id: 0,
            data_device_manager,
            data_device: None,
            touch_fingers: HashMap::new(),
            pending_finger_removals: Vec::new(),
            cursor_shape_manager: None,
            current_mouse_interaction: iced_core::mouse::Interaction::default(),
            pointer_enter_serial: 0,
            wl_pointer: None,
            bg_effect_manager: None,
            bg_effect_supports_blur: false,
            loop_handle,
            display_ptr,
        }
    }

    /// Register a new layer surface with the given ID and initial buffer scale.
    /// `scale_factor` must match whatever was applied via
    /// `wl_surface.set_buffer_scale` at creation, so the renderer and the
    /// compositor agree on the buffer's coordinate system.
    pub fn register_surface(
        &mut self,
        id: SurfaceId,
        layer_surface: LayerSurface,
        scale_factor: i32,
    ) {
        self.register_role(id, Role::Layer(layer_surface), scale_factor, (0, 0));
    }

    /// Register a surface of any role (kestrel patch: windows too).
    pub fn register_role(
        &mut self,
        id: SurfaceId,
        role: Role,
        scale_factor: i32,
        default_size: (u32, u32),
    ) {
        let wl_surface = role.wl_surface().clone();
        self.surfaces.insert(
            wl_surface.clone(),
            SurfaceData {
                id,
                role,
                default_size,
                size: (0, 0),
                scale_factor,
                configured: false,
                frame_pending: false,
                needs_rerender: false,
                committed_scale_factor: scale_factor,
                current_output: None,
                bg_effect_surface: None,
                blur_region: None,
            },
        );
        self.surface_id_map.insert(id, wl_surface);
    }

    /// Get the `SurfaceId` for a given Wayland surface.
    pub fn surface_id_for(&self, wl_surface: &WlSurface) -> Option<SurfaceId> {
        self.surfaces.get(wl_surface).map(|d| d.id)
    }

    /// Set the cursor shape based on mouse interaction.
    pub fn set_cursor_shape(
        &mut self,
        interaction: iced_core::mouse::Interaction,
        qh: &QueueHandle<WaylandState>,
    ) {
        use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;

        if interaction == self.current_mouse_interaction {
            return;
        }
        self.current_mouse_interaction = interaction;

        let Some(manager) = &self.cursor_shape_manager else {
            return;
        };
        let Some(pointer) = &self.wl_pointer else {
            return;
        };

        // For a status bar, only change cursor for text input fields.
        // Everything else keeps the default arrow.
        let shape = match interaction {
            iced_core::mouse::Interaction::Text => Shape::Text,
            _ => Shape::Default,
        };

        let device = manager.get_shape_device(pointer, qh);
        device.set_shape(self.pointer_enter_serial, shape);
        device.destroy();
    }

    /// Allocate a new unique output ID.
    pub(crate) fn allocate_output_id(&mut self) -> OutputId {
        let id = OutputId(self.next_output_id);
        self.next_output_id += 1;
        id
    }
}
