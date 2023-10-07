use smithay::{delegate_xdg_decoration, wayland::shell::xdg::decoration::XdgDecorationHandler};

use crate::{state::Backend, ScreenComposer};

impl<BackendData: Backend> XdgDecorationHandler for ScreenComposer<BackendData> {
    fn new_decoration(&mut self, _toplevel: smithay::wayland::shell::xdg::ToplevelSurface) {
        println!("new_decoration");
    }
    fn request_mode(
        &mut self,
        _toplevel: smithay::wayland::shell::xdg::ToplevelSurface,
        mode: smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    ) {
        println!("request_mode: {:?}", mode);
    }
    fn unset_mode(&mut self, _toplevel: smithay::wayland::shell::xdg::ToplevelSurface) {
        println!("unset_mode");
    }
}
delegate_xdg_decoration!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
