//! kestrel patch: ordinary windows (xdg-toplevel). The compositor sizes them (tiled or
//! floating); a window it does not size keeps the size it asked for. Closing one (Super+Q, a
//! title bar's button) removes it and tells the app with `window::Event::CloseRequested`.

use smithay_client_toolkit::delegate_xdg_shell;
use smithay_client_toolkit::delegate_xdg_window;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use wayland_client::{Connection, QueueHandle};

use crate::state::WaylandState;

impl WindowHandler for WaylandState {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, window: &Window) {
        if let Some(data) = self.surfaces.get(window.wl_surface()) {
            let id = data.id;
            self.pending_events.push((
                id,
                iced_core::Event::Window(iced_core::window::Event::CloseRequested),
            ));
            self.closed_surfaces.push(id);
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        if let Some(data) = self.surfaces.get_mut(window.wl_surface()) {
            let (w, h) = configure.new_size;
            let size = match (w, h) {
                (Some(w), Some(h)) => (w.get(), h.get()),
                _ if data.size.0 > 0 => data.size,
                _ => data.default_size,
            };
            crate::ktrace!("configure window {:?} {:?}", data.id, size);
            data.size = size;
            data.configured = true;
            self.surfaces_need_redraw.insert(data.id);
        }
    }
}

delegate_xdg_shell!(WaylandState);
delegate_xdg_window!(WaylandState);
