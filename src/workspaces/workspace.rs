use super::{BackgroundView, WindowSelectorView};
use crate::{config::Config, shell::WindowElement, utils::image_from_path};
use core::fmt;

use lay_rs::{
    engine::LayersEngine,
    prelude::{taffy, Layer},
};
use smithay::reexports::wayland_server::backend::ObjectId;
use std::sync::{atomic::AtomicBool, Arc, RwLock};

#[derive(Clone)]
pub struct WorkspaceView {
    pub index: usize,
    pub windows_list: Arc<RwLock<Vec<ObjectId>>>,

    // views
    pub window_selector_view: Arc<WindowSelectorView>,
    pub background_view: Arc<BackgroundView>,

    // scene
    pub layers_engine: LayersEngine,
    pub workspace_layer: Layer,
    pub windows_layer: Layer,

    fullscreen_mode: Arc<AtomicBool>,

}

impl fmt::Debug for WorkspaceView {
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

/// # Workspace Layer Structure
///
/// ```diagram
/// WorkspaceView
/// └── workspace_view
///     ├── background_view
///     ├── workspace_windows_container
///     │   ├── window
///     │   ├── window
///     │   └── window
///     └── overlay
///         └── fullscreen_surface
/// ```
///
impl WorkspaceView {
    pub fn new(index: usize, layers_engine: LayersEngine, parent: &Layer) -> Self {
        println!("add_workspace {}", index);

        let workspace_layer = layers_engine.new_layer();
        workspace_layer.set_key(format!("workspace_view_{}", index));
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
        windows_layer.set_key(format!("workspace_windows_container_{}", index));
        windows_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        windows_layer.set_pointer_events(false);

        let workspace_id = layers_engine.append_layer_to(workspace_layer.clone(), parent.id());

        layers_engine.append_layer_to(background_layer.clone(), Some(workspace_id));
        layers_engine.append_layer_to(windows_layer.clone(), Some(workspace_id));

        let background_view = BackgroundView::new(index, background_layer.clone());
        let background_path = Config::with(|c| c.background_image.clone());
        if let Some(background_image) = image_from_path(&background_path, (2048,2048)) {
            background_view.set_image(background_image);
        }
        let background_view = Arc::new(background_view);

        let window_selector_view = WindowSelectorView::new(
            index,
            layers_engine.clone(),
            background_view.base_layer.clone(),
        );

        let window_selector_view = Arc::new(window_selector_view);

        Self {
            index,
            windows_list: Arc::new(RwLock::new(Vec::new())),
            window_selector_view: window_selector_view.clone(),
            background_view,
            layers_engine,
            windows_layer,
            workspace_layer,
            fullscreen_mode: Arc::new(AtomicBool::new(false)),
        }
    }

    /// add a window layer to the workspace windows container
    /// and append the window to the windows list
    /// and creates a clone of the window layer to be used in the window selector view
    /// (if the window is already in the windows list, it will not be added)
    pub fn map_window(&self, window_element: &WindowElement, location: smithay::utils::Point<i32, smithay::utils::Logical>) {
        let mut window_list = self.windows_list.write().unwrap();
        let wid = window_element.id();
        if !window_list.contains(&wid) {
            window_list.push(wid.clone());
            
            self.windows_layer
            .add_sublayer(window_element.base_layer().clone());
        
            
            let mirror_window = self.layers_engine.new_layer();
            mirror_window.set_key(format!(
                "mirror_window_{}",
                window_element.base_layer().id().unwrap().0
            ));
            mirror_window.set_layout_style(taffy::Style {
                position: taffy::Position::Absolute,
                ..Default::default()
            });
            mirror_window.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
            self.window_selector_view
                .windows_layer
                .add_sublayer(mirror_window.clone());
            let mirror_window_id = mirror_window.id().unwrap();
            let mirror_window_node = self
                .layers_engine
                .scene_get_node(&mirror_window_id)
                .unwrap();
            let mirror_window_node = mirror_window_node.get();
            mirror_window_node.replicate_node(&window_element.base_layer().id());
            self.window_selector_view.map_window(wid, mirror_window);
        }

        
        let scale = Config::with(|c| c.screen_scale);
        let location = location
            .to_f64()
            .to_physical(scale);


        window_element.base_layer().set_position(
            lay_rs::types::Point {
                x: location.x as f32,
                y: location.y as f32,
            },
            None,
        );

        if let Some(l) = self
            .window_selector_view
            .layer_for_window(&window_element.id())
        {
            l.set_position(
                lay_rs::types::Point {
                    x: location.x as f32,
                    y: location.y as f32,
                },
                None,
            );
        }
        
    }

    /// remove the window from the windows list
    /// and remove the window layer from the window selector view
    pub fn unmap_window(&self, window_id: &ObjectId) {
        let mut window_list = self.windows_list.write().unwrap();

        if let Some(index) = window_list.iter().position(|x| x == window_id) {
            window_list.remove(index);
        }
        
        self.window_selector_view.unmap_window(window_id);
    }

    pub fn set_fullscreen_mode(&self, fullscreen: bool) {
        self.fullscreen_mode
            .store(fullscreen, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_fullscreen_mode(&self) -> bool {
        self.fullscreen_mode
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Drop for WorkspaceView {
    fn drop(&mut self) {
        self.layers_engine
            .scene_remove_layer(self.windows_layer.id());
        self.layers_engine
            .scene_remove_layer(self.workspace_layer.id());
        self.layers_engine
            .scene_remove_layer(self.window_selector_view.layer.id());
    }
}
