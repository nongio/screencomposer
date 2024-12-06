use smithay::{
    delegate_xdg_decoration,
    reexports::wayland_protocols::xdg::decoration::{
        self as xdg_decoration, zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    },
    wayland::{
        compositor::with_states,
        shell::xdg::{decoration::XdgDecorationHandler, ToplevelSurface, XdgToplevelSurfaceData},
    },
};

use super::{Backend, ScreenComposer};

impl<BackendData: Backend> XdgDecorationHandler for ScreenComposer<BackendData> {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        // Set the default to client side
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;

        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(match mode {
                DecorationMode::ServerSide => Mode::ClientSide,
                _ => Mode::ClientSide,
            });
        });

        let initial_configure_sent = with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });
        if initial_configure_sent {
            toplevel.send_pending_configure();
        }
    }
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
        let initial_configure_sent = with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });
        if initial_configure_sent {
            toplevel.send_pending_configure();
        }
    }
}
delegate_xdg_decoration!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
