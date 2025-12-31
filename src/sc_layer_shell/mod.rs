mod handlers;
mod protocol;

use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::DisplayHandle;

use crate::state::Backend;

pub use handlers::create_layer_shell_global;
pub use protocol::{gen, ScLayer, ScLayerShellHandler, ScLayerZOrder, ScTransaction};

/// Shell global state
#[derive(Clone)]
pub struct ScLayerShellState {
    shell_global: GlobalId,
}

impl ScLayerShellState {
    /// Create a new sc_layer_shell global
    pub fn new<BackendData: Backend + 'static>(display: &DisplayHandle) -> ScLayerShellState {
        let shell_global = create_layer_shell_global::<BackendData>(display);

        ScLayerShellState { shell_global }
    }

    /// Get shell global id
    pub fn shell_global(&self) -> GlobalId {
        self.shell_global.clone()
    }
}
