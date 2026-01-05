pub mod rendering;
pub mod components;
pub mod surfaces;
pub mod app_runner;

// Re-export commonly used items
pub use components::window::{Window, SimpleWindow};
pub use components::menu_bar::{MenuBar, MenuBarItem, surface::MenuBarSurface};
pub use components::layer::{Layer, surface::LayerSurface};

// Re-export new surface types
pub use surfaces::{
    Surface, ScLayerAugment, SurfaceError,
    ToplevelSurface, SubsurfaceSurface, PopupSurface,
};

// Re-export app framework
pub use app_runner::{App, AppRunner, AppContext};

/// Convenience prelude for application development
pub mod prelude {
    pub use crate::app_runner::{App, AppRunner, AppContext};
    pub use crate::components::window::Window;
    pub use skia_safe::{Color, Paint, Canvas, Font, Rect};
    // Add more common types as needed
}
