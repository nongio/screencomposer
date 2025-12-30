mod data;
mod drawing;
mod surface;

pub use data::{MenuItem, MenuItemBuilder, MenuItemId, MenuStyle, Position, Anchor, Gravity};
pub use surface::Menu;

use std::fmt;

#[derive(Debug)]
pub enum MenuError {
    SurfaceCreationFailed,
    NotConfigured,
    NoParent,
    NotImplemented,
    WaylandError(String),
}

impl fmt::Display for MenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MenuError::SurfaceCreationFailed => write!(f, "Failed to create menu surface"),
            MenuError::NotConfigured => write!(f, "Menu surface not configured"),
            MenuError::NoParent => write!(f, "No parent surface provided"),
            MenuError::NotImplemented => write!(f, "Feature not yet implemented"),
            MenuError::WaylandError(e) => write!(f, "Wayland error: {}", e),
        }
    }
}

impl std::error::Error for MenuError {}
