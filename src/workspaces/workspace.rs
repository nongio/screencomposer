use super::{BackgroundView, WindowSelectorView};
use crate::{shell::WindowElement, utils::image_from_path};
use core::fmt;

use lay_rs::skia;
use lay_rs::{
    engine::LayersEngine,
    prelude::{taffy, Layer},
};
use smithay::reexports::wayland_server::{
    backend::ObjectId, protocol::wl_surface::WlSurface, Resource,
};
use std::{
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Window {
    pub wl_surface: Option<WlSurface>,
    pub window_element: Option<WindowElement>,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub is_fullscreen: bool,
    pub is_maximized: bool,
    pub is_minimized: bool,
    pub app_id: String,
    pub base_layer: Layer,
}
impl Window {
    pub fn new_with_layer(layer: Layer) -> Self {
        Self {
            base_layer: layer,
            wl_surface: None,
            window_element: None,
            title: "".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            is_fullscreen: false,
            is_maximized: false,
            is_minimized: false,
            app_id: "".to_string(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Application {
    pub identifier: String,
    pub desktop_name: Option<String>,
    pub icon_path: Option<String>,
    pub icon: Option<skia::Image>,
}
impl PartialEq for Window {
    fn eq(&self, other: &Self) -> bool {
        self.wl_surface == other.wl_surface
    }
}
impl Eq for Window {}
impl Hash for Window {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.wl_surface.hash(state);
    }
}
impl Window {
    pub fn id(&self) -> Option<ObjectId> {
        self.wl_surface.as_ref().map(|s| s.id())
    }
}
impl fmt::Debug for Application {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Application")
            .field("identifier", &self.identifier)
            .field("desktop_name", &self.desktop_name)
            .field("icon_path", &self.icon_path)
            .field("icon", &self.icon.is_some())
            .finish()
    }
}

impl Hash for Application {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identifier.hash(state);
        self.icon_path.hash(state);
        self.desktop_name.hash(state);
        self.icon.as_ref().map(|i| i.unique_id().hash(state));
    }
}

impl PartialEq for Application {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}
impl Eq for Application {}

#[derive(Clone)]
pub struct Workspace {
    // views
    pub window_selector_view: Arc<WindowSelectorView>,
    pub background_view: Arc<BackgroundView>,

    // scene
    pub layers_engine: LayersEngine,
    pub workspace_layer: Layer,
    pub windows_layer: Layer,
}

impl fmt::Debug for Workspace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // let model = self.model.read().unwrap();

        f.debug_struct("WorkspaceModel")
            // .field("applications", &model.applications_cache)
            // .field("application_list", &self.application_list)
            // .field("windows", &self.windows)
            // .field("current_application", &self.current_application)
            .finish()
    }
}

impl Application {
    pub fn new(app_id: &str) -> Self {
        Self {
            identifier: app_id.to_string(),
            ..Default::default()
        }
    }
}

impl Workspace {
    pub fn new(layers_engine: LayersEngine, parent: &Layer) -> Self {
        let workspace_layer = layers_engine.new_layer();
        workspace_layer.set_key("workspace_view");
        workspace_layer.set_layout_style(taffy::Style {
            flex_grow: 1.0,
            flex_shrink: 0.0,
            flex_basis: taffy::Dimension::Percent(1.0),
            ..Default::default()
        });
        workspace_layer.set_size(lay_rs::types::Size::percent(0.7, 1.0), None);
        workspace_layer.set_pointer_events(false);

        let background_layer = layers_engine.new_layer();
        background_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        background_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        // background_layer.set_opacity(0.0, None);

        let windows_layer = layers_engine.new_layer();
        windows_layer.set_key("windows_container");
        windows_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        windows_layer.set_pointer_events(false);

        let workspace_id = layers_engine.scene_add_layer_to(workspace_layer.clone(), parent.id());

        layers_engine.scene_add_layer_to(background_layer.clone(), Some(workspace_id));
        layers_engine.scene_add_layer_to(windows_layer.clone(), Some(workspace_id));

        let background_view = BackgroundView::new(layers_engine.clone(), background_layer.clone());
        if let Some(background_image) = image_from_path("./resources/background.jpg", None) {
            background_view.set_image(background_image);
        }
        let background_view = Arc::new(background_view);

        let window_selector_view = WindowSelectorView::new(layers_engine.clone());
        let window_selector_view = Arc::new(window_selector_view);

        Self {
            // app_switcher,
            window_selector_view: window_selector_view.clone(),
            background_view,
            layers_engine,
            windows_layer,
            workspace_layer,
        }
    }
}
