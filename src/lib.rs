use state::{Backend, ScreenComposer};
use wayland_server::Display;

pub mod grabs;
pub mod handlers;
pub mod input;
pub mod renderer;
pub mod sc_layer_shell;
pub mod state;
pub mod winit;

pub struct CalloopData<BackendData: Backend + 'static> {
    pub state: ScreenComposer<BackendData>,
    pub display: Display<ScreenComposer<BackendData>>,
}
