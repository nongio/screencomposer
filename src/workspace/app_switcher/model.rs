use std::hash::{Hash, Hasher};

use crate::{
    utils::Observer,
    workspace::{Application, Workspace},
};

#[derive(Debug, Clone, Default)]
pub struct AppSwitcherModel {
    pub apps: Vec<Application>,
    pub current_app: usize,
    pub width: i32,
}

impl Hash for AppSwitcherModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // let apps = self.apps.read().unwrap();
        self.apps.hash(state);
        self.current_app.hash(state);
    }
}

impl AppSwitcherModel {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Observer<Workspace> for AppSwitcherModel {
    fn notify(&self, _event: &Workspace) {
        // println!("AppSwitcherState received event");
    }
}
