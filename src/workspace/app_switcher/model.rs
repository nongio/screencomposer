use std::hash::{Hash, Hasher};

use crate::workspace::Application;

#[derive(Debug, Clone, Default)]
pub struct AppSwitcherModel {
    pub apps: Vec<Application>,
    pub current_app: usize,
    pub width: i32,
}

impl Hash for AppSwitcherModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.apps.hash(state);
        self.current_app.hash(state);
        self.width.hash(state);
    }
}

impl AppSwitcherModel {
    pub fn new() -> Self {
        Default::default()
    }
}
