use smithay::reexports::wayland_server;
use wayland_server::protocol::wl_output::WlOutput;

pub mod __interfaces {
    pub use smithay::reexports::wayland_server::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("./protocols/sc-layer-unstable-v1.xml");
}
use crate::{state::Backend, ScreenComposer};

use self::__interfaces::*;
use smithay::reexports::wayland_server::protocol::*;

use super::{LayerSurface, ScLayerShellState};
wayland_scanner::generate_server_code!("./protocols/sc-layer-unstable-v1.xml");

/// The role of a wlr_layer_shell_surface
pub const SCLAYER_SURFACE_ROLE: &str = "zsc_layer_surface_v1";

/// Handler for wlr layer shell
#[allow(unused_variables)]
pub trait ScLayerShellHandler {
    /// [WlrLayerShellState] getter
    fn shell_state(&mut self) -> &mut ScLayerShellState;

    /// Create a new layer surface
    fn new_layer_surface(&mut self, surface: LayerSurface, output: Option<WlOutput>);

    /// A layer surface was destroyed.
    fn destroy_layer_surface(&mut self, surface: &LayerSurface) {}

    fn get_animation(&mut self, duration: f32, speed: f32);
}

impl std::cmp::PartialEq for LayerSurface {
    fn eq(&self, other: &Self) -> bool {
        self.wl_surface == other.wl_surface
    }
}

impl<BackendData: Backend> ScLayerShellHandler for ScreenComposer<BackendData> {
    fn destroy_layer_surface(&mut self, surface: &LayerSurface) {
        self.engine.scene_remove_layer(surface.layer.id());
    }
    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        output: Option<wayland_server::protocol::wl_output::WlOutput>,
    ) {
        self.engine.scene_add_layer(surface.layer);
    }
    fn shell_state(&mut self) -> &mut ScLayerShellState {
        &mut self.sc_shell_state
    }

    fn get_animation(&mut self, duration: f32, speed: f32) {}
}
