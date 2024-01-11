use std::collections::HashMap;

use freedesktop_desktop_entry::{default_paths, PathSource, Iter, DesktopEntry};
use smithay::{desktop, wayland::shell::xdg::ToplevelSurface, backend::x11::Window};
use xdgkit::desktop_entry;

use crate::{render_elements::app_switcher::{image_from_svg, image_from_icon_path}, shell::WindowElement};

pub mod view;

#[derive(Clone, Hash)]
pub struct App {
    pub name: String,
    pub icon_path: Option<String>,
}

#[derive(Clone, Default)]
pub struct AppSwitcher {
    pub apps: Vec<(App, WindowElement)>,
    pub current_app: usize,
    preview_images: HashMap<std::string::String, skia_safe::Image>,
    pub width: i32,
}

use std::hash::{Hash, Hasher};

impl Hash for AppSwitcher {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let apps:Vec<_> =  self.apps.iter().map(|(a, _)| a).collect();
        apps.hash(state);
        self.current_app.hash(state);
    }
}

impl App {
    pub fn new(name: &str) -> Self {
        let mut desktop_entry: Option<DesktopEntry<'_>> = None;
        let bytes;
        let path;
        let path_result= Iter::new(default_paths()).find(|path| {
            path.to_string_lossy().contains(name)
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
        let mut name = name.to_string();
        let mut icon = None;
        if let Some(desktop_entry) = desktop_entry {
            icon = desktop_entry.icon().map(|icon| icon.to_string())
            .and_then(|icon_name| {
                xdgkit::icon_finder::find_icon(icon_name, 512, 1)
            }).map(|icon| {
                icon.to_str().unwrap().to_string()
            });

            if let Some(desktop_name) = desktop_entry.name(None) {
                name = desktop_name.to_string();
            }
        }
        Self {
            name,
            icon_path: icon,
        }
    }
}

impl AppSwitcher {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update_apps(&mut self, new_apps: &[(String, WindowElement)]) {
        // Remove apps that are not in new_apps
        self.apps.retain(|(app, _)| {
            new_apps.iter().find(|(name, _)| name == &app.name).is_some()
        });

        // Add apps from new_apps that are not in self.apps
        for (app_id, ts) in new_apps {
            if !self.apps.iter().any(|(app, _)| &app.name == app_id) {
                self.apps.push((App::new(app_id), ts.clone()));
            }
        }
        self.current_app = self.current_app.min(self.apps.len() - 1);
        self.update_icons();
    }
    pub fn update_icons(&mut self) {
        for (App {name, icon_path: icon}, _) in self.apps.iter() {
            if self.preview_images.contains_key(name) {
                continue;
            }
            if icon.is_none() {
                continue;
            }
            let icon_path = icon.as_ref().unwrap();
            let image = image_from_icon_path(icon_path);
            self.preview_images.insert(name.clone(), image);
        }
    }

    pub fn current_window_element(&self) -> Option<&WindowElement> {
        self.apps.get(self.current_app).map(|(_, we)| we)
    }
}