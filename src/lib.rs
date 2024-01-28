// If no backend is enabled, a large portion of the codebase is unused.
// So silence this useless warning for the CI.
#![cfg_attr(
    not(any(feature = "winit", feature = "x11", feature = "udev")),
    allow(dead_code, unused_imports)
)]

#![feature(core_panic)]

#[cfg(any(feature = "udev", feature = "xwayland"))]
pub mod cursor;
pub mod drawing;
pub mod focus;
pub mod input_handler;
pub mod render;
pub mod shell;
pub mod render_elements;
pub mod skia_renderer;
pub mod state;
#[cfg(feature = "udev")]
pub mod udev;
#[cfg(feature = "winit")]
pub mod winit;
#[cfg(feature = "x11")]
pub mod x11;

pub use state::{ScreenComposer, CalloopData, ClientState};
pub mod app_switcher;
pub mod window_view;
pub mod workspace_view;

pub mod utils;