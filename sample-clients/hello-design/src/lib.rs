pub mod rendering;
pub mod components;

// Re-export commonly used items
pub use components::window::SimpleWindow;
pub use components::menu_bar::{MenuBar, MenuBarItem, surface::MenuBarSurface};
