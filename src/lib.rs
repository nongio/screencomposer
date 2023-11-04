use state::{Backend, ScreenComposer};

pub mod grabs;
pub mod handlers;
pub mod input;
pub mod renderer;
// pub mod sc_layer_shell;
pub mod cursor;
pub mod debug;
pub mod focus;
pub mod state;
pub mod udev;
pub mod winit;

pub struct CalloopData<BackendData: Backend + 'static> {
    pub state: ScreenComposer<BackendData>,
}
