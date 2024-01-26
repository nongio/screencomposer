use std::collections::HashMap;
use freedesktop_desktop_entry::{default_paths, Iter as DesktopEntryIter, DesktopEntry};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

use crate::{shell::WindowElement, utils::image_from_path};


#[derive(Clone, Debug)]
pub struct AppSwitcherAppState {
    pub identifier: String,
    pub desktop_name: Option<String>,
    pub icon_path: Option<String>,
    pub icon: Option<skia_safe::Image>,
}

#[derive(Clone, Default)]
pub struct AppSwitcherState {
    pub apps: Arc<RwLock<Vec<AppSwitcherAppState>>>,
    pub apps_windows: HashMap<String, Vec<WindowElement>>,
    pub current_app: usize,
    preview_images: Arc<RwLock<HashMap<std::string::String, skia_safe::Image>>>,
    pub width: i32,
}

impl PartialEq for AppSwitcherAppState {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}
impl Eq for AppSwitcherAppState {}

impl Hash for AppSwitcherAppState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identifier.hash(state);
        self.desktop_name.hash(state);

        if let Some(icon) = self.icon.as_ref() {
            icon.unique_id().hash(state);
        }
    }
}

impl Hash for AppSwitcherState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let apps = self.apps.read().unwrap();
        let apps:Vec<_> =  apps.iter().collect();
        apps.hash(state);
        self.current_app.hash(state);
    }
}


impl AppSwitcherAppState {
    pub fn new(identifier: &str) -> Self {     
        let identifier = identifier.to_string();
        Self {
            identifier,
            desktop_name: None,
            icon_path: None,
            icon: None,
        }
    }
}

impl AppSwitcherState {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn apps(&self) -> std::sync::RwLockReadGuard<'_, Vec<AppSwitcherAppState>> {
        self.apps.read().unwrap()
    }
    fn load_async_app_info(&self, app_id: &str) {
        let app_id = app_id.to_string();
        let apps = self.apps.clone();
        let preview_images = self.preview_images.clone();
        tokio::spawn(async move{
            let mut desktop_entry: Option<DesktopEntry<'_>> = None;
            let bytes;
            let path;
            let default_paths = default_paths();
            let path_result= DesktopEntryIter::new(default_paths).find(|path| {
                path.to_string_lossy().contains(&app_id)
            });
        
            if let Some(p) = path_result {
                path = p.clone();
                let bytes_result = std::fs::read_to_string(&p);
                if  let Ok(b) = bytes_result {
                    bytes = b.clone();
                    if let Ok(entry) = DesktopEntry::decode(&path, &bytes) {
                        desktop_entry = Some(entry);
                    }
                }
            }
            if let Some(desktop_entry) = desktop_entry {
                let icon_path = desktop_entry.icon().map(|icon| icon.to_string())
                .and_then(|icon_name| {
                    xdgkit::icon_finder::find_icon(icon_name, 512, 1)
                }).map(|icon| {
                    icon.to_str().unwrap().to_string()
                });
                let icon = icon_path.as_ref().map(|icon_path| {image_from_path(icon_path)});
                let state = AppSwitcherAppState {
                    identifier: app_id.to_string(),
                    desktop_name: desktop_entry.name(None).map(|name| name.to_string()),
                    icon_path,
                    icon: icon.clone(),
                };
                if let Some(icon) = icon {
                    let mut preview_images = preview_images.write().unwrap();
                    preview_images.insert(state.identifier.clone(), icon);
                }
                let mut apps = apps.write().unwrap();
                if let Some(app) = apps.iter_mut().find(|app| app.identifier == app_id) {
                    *app = state;
                }
            }
        });
    }
    pub fn update_apps(&mut self, new_apps: &[(String, WindowElement)]) {
        // Remove apps that are not in new_apps
        {
            let apps = &mut self.apps.write().unwrap();
            apps.retain(|app| {
                new_apps.iter().any(|(name, _)| name == &app.identifier)
            });
            let mut app_windows = HashMap::new();
            // Add apps from new_apps that are not in self.apps
            for (app_id, we) in new_apps {
                let app = AppSwitcherAppState::new(app_id);
                if !apps.iter().any(|app| &app.identifier == app_id) {
                    apps.push(app.clone());
                    self.load_async_app_info(app_id);
                }
                let windows = app_windows.entry(app.identifier).or_insert(Vec::new());
                windows.push(we.clone());
            }
            self.apps_windows = app_windows;
            if apps.is_empty() {
                self.current_app = 0;
            } else {
                self.current_app = self.current_app.min(apps.len() - 1);
            }
        }
    }

    pub fn current_window_elements(&self) -> Vec<WindowElement> {
        let apps = self.apps.read().unwrap();
        let current_app = apps.get(self.current_app);
        if let Some(current_app) = current_app {
            if let Some(windows) = self.apps_windows.get(&current_app.identifier) {
                return windows.clone();
            }
        }
        Vec::new()
    }

    pub fn next_app(&mut self) {
        let apps = self.apps.read().unwrap();
        if !apps.is_empty() {
            self.current_app = (self.current_app + 1) % apps.len();
        } else {
            self.current_app = 0;
        }
    }
    pub fn previous_app(&mut self) {
        let apps = self.apps.read().unwrap();
        
        if !apps.is_empty() {
            self.current_app = (self.current_app + apps.len() - 1) % apps.len();
        } else {
            self.current_app = 0;
        }
    }
    pub fn next_window(&mut self) {
        let apps = self.apps.read().unwrap();
        if let Some(current_app) = apps.get(self.current_app) {
            if let Some(windows) = self.apps_windows.get_mut(&current_app.identifier) {
                windows.rotate_right(1);
            }
        }
    }
}