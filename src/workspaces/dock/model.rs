use std::hash::{Hash, Hasher};

use crate::workspaces::{Application, Window};

#[derive(Debug, Clone, Default)]
pub struct DockModel {
    pub launchers: Vec<Application>,
    pub running_apps: Vec<Application>,
    pub minimized_windows: Vec<Window>,
    pub width: i32,
    pub focus: f32,
}

impl Hash for DockModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.launchers.hash(state);
        self.running_apps.hash(state);
        self.minimized_windows.hash(state);
        self.width.hash(state);
    }
}

impl DockModel {
    pub fn new() -> Self {
        Self {
            focus: -500.0,
            ..Default::default()
        }
    }
}
