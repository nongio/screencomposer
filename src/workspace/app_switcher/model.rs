
use std::hash::{Hash, Hasher};

use crate::{utils::Observer, workspace::{Application, Workspace}};


#[derive(Debug, Clone, Default)]
pub struct AppSwitcherModel {
    pub apps: Vec<Application>,
    pub current_app: usize,
    pub width: i32,
}



impl Hash for AppSwitcherModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // let apps = self.apps.read().unwrap();
        let apps:Vec<_> =  self.apps.iter().collect();
        apps.hash(state);
        self.current_app.hash(state);
    }
}

impl AppSwitcherModel {
    pub fn new() -> Self {
        Default::default()
    }
    // pub fn apps(&self) -> std::sync::RwLockReadGuard<'_, Vec<AppSwitcherAppState>> {
    //     // self.apps.read().unwrap()
    //     self.apps
    // }
    
    // pub fn update_apps(&mut self, new_apps: &[(String, WindowElement)]) {
    //     // Remove apps that are not in new_apps
    //     {
    //         let apps = &mut self.apps.write().unwrap();
    //         apps.retain(|app| {
    //             new_apps.iter().any(|(name, _)| name == &app.identifier)
    //         });
    //         // Add apps from new_apps that are not in self.apps
    //         for (app_id, we) in new_apps {
    //             let app = AppSwitcherAppState::new(app_id);
    //             if !apps.iter().any(|app| &app.identifier == app_id) {
    //                 apps.push(app.clone());
    //             }
    //         }
    //         if apps.is_empty() {
    //             self.current_app = 0;
    //         } else {
    //             self.current_app = self.current_app.min(apps.len() - 1);
    //         }
    //     }
    // }

    
    // pub fn next_window(&mut self) {
    //     let apps = self.apps.read().unwrap();
    //     if let Some(current_app) = apps.get(self.current_app) {
    //         if let Some(windows) = self.apps_windows.get_mut(&current_app.identifier) {
    //             windows.rotate_right(1);
    //         }
    //     }
    // }
}

impl  Observer<Workspace> for AppSwitcherModel {
    fn notify(&self, _event: &Workspace) {
        // println!("AppSwitcherState received event");
    }
 }