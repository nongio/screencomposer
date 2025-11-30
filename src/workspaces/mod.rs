use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicI32},
        Arc, RwLock, Weak,
    },
};

use apps_info::Application;
use lay_rs::{
    engine::{Engine, TransactionRef},
    prelude::{taffy, Interpolate, Layer, Spring, TimingFunction, Transition},
    skia::{self, Contains},
    types::Size,
};
use smithay::{
    desktop::{layer_map_for_output, space::SpaceElement, Space, WindowSurface},
    output::Output,
    reexports::wayland_server::{backend::ObjectId, Resource},
    utils::{IsAlive, Rectangle},
};

use workspace::WorkspaceView;

mod app_switcher;
mod background;
mod dnd_view;
mod dock;
pub mod workspace;

pub mod utils;

mod apps_info;
mod window_selector;
mod window_view;
mod workspace_selector;

pub use background::BackgroundView;
pub use window_selector::{
     WindowSelectorView, WindowSelectorWindow,
};
pub use window_view::{WindowView, WindowViewBaseModel, WindowViewSurface};

pub use app_switcher::AppSwitcherView;
pub use dnd_view::DndView;
pub use dock::DockView;
pub use workspace_selector::{WorkspaceSelectorView, WORKSPACE_SELECTOR_PREVIEW_WIDTH};

use crate::{
    config::Config,
    shell::WindowElement,
    utils::{
        natural_layout::LayoutRect,
        Observable, Observer,
    },
};

#[derive(Debug, Default, Clone)]
pub struct WorkspacesModel {
    workspace_counter: usize,
    pub workspaces: Vec<Arc<WorkspaceView>>,
    pub current_workspace: usize,

    pub app_windows_map: HashMap<String, Vec<ObjectId>>,
    /// list of applications in the order they are visually displayed
    /// mainly used for the app switcher
    pub zindex_application_list: Vec<String>,
    /// list of applications in the order they are launched
    /// mainly used for the dock
    pub application_list: VecDeque<String>,

    pub minimized_windows: Vec<(ObjectId, String)>,
    pub current_application: usize,
    /// The physical width of the workspace
    pub width: i32,
    /// The physical height of the workspace
    pub height: i32,
    pub scale: f64,
}


pub struct Workspaces {
    model: Arc<RwLock<WorkspacesModel>>,
    spaces: Vec<Space<WindowElement>>,
    outputs: Vec<Output>,

    pub windows_map: HashMap<ObjectId, WindowElement>,
    // views
    pub workspace_selector_view: Arc<WorkspaceSelectorView>,
    pub dock: Arc<DockView>,
    pub app_switcher: Arc<AppSwitcherView>,
    pub window_views: Arc<RwLock<HashMap<ObjectId, WindowView>>>,
    pub dnd_view: DndView,

    // gestures states
    pub show_all: Arc<AtomicBool>,
    pub show_desktop: Arc<AtomicBool>,
    pub show_all_gesture: Arc<AtomicI32>,
    pub show_desktop_gesture: Arc<AtomicI32>,

    // layers
    pub layers_engine: Arc<Engine>,
    pub overlay_layer: Layer,
    pub workspaces_layer: Layer,
    expose_layer: Layer,
    observers: Vec<Weak<dyn Observer<WorkspacesModel>>>,
    expose_dragging_window: Arc<std::sync::Mutex<Option<ObjectId>>>,
}

/// # Workspaces Layer Structure
///
/// ```diagram
/// Workspaces
/// root
/// ├── workspaces
/// │   ├── workspace_view_1
/// │   │   ├── background_view (mirrored)
/// │   │   └── workwspace_windows_container_1
/// │   │       ├── window_view_1
/// │   │       ├── window_view_2
/// │   │       ...
/// │   ├── workspace_view_2
/// │   ...
/// ├── expose
/// │   ├── windows_selector_root_1
/// │   │   ├── window_selector_background_1 (mirror: background_view)
/// │   │   ├── window_selector_windows_container_1
/// │   │   │   ├── mirror_window_1
/// │   │   │   ├── mirror_window_2
/// │   │   │   ...
/// │   │   ├── window_selector_view_1
/// │   ├── expose_view
/// │   ├── app_switcher
/// │
/// ├── dnd_view
/// ├── dock
/// ├── app_switcher
/// ├── workspace_selector_view
/// │   ├── workspace_selector_view_content
/// │   │   ├── workspace_selector_desktop_1
/// │   │   │   ├── workspace_selector_desktop_content_1
/// │   │   │   │   ├── workspace_selector_desktop_content_1 (mirror: workspace_view_1)
/// │   │   │   │   ├── workspace_selector_desktop_border_1
/// │   │   │   │   ├── workspace_selector_desktop_remove_1
/// │   │   │   ├── workspace_selector_desktop_label_1
/// │   │   ├── workspace_selector_desktop_2
/// │   │   │   ├── ...
/// │   │   ...
/// │   ├── workspace_selector_workspace_add
/// ```
///
impl Workspaces {
    pub fn start_window_selector_drag(&self, window_id: &ObjectId) {
        *self.expose_dragging_window.lock().unwrap() = Some(window_id.clone());
        self.expose_update_if_needed();
    }

    pub fn end_window_selector_drag(&self, window_id: &ObjectId) {
        let mut dragging = self.expose_dragging_window.lock().unwrap();
        if dragging.as_ref() == Some(window_id) {
            *dragging = None;
        }
        drop(dragging);
        self.expose_show_all(1.0, true);
    }
    pub fn new(layers_engine: Arc<Engine>) -> Self {
        let model = WorkspacesModel::default();
        let spaces = Vec::new();

        let workspaces_layer = layers_engine.new_layer();
        workspaces_layer.set_key("workspaces");
        workspaces_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            display: taffy::Display::Flex,
            ..Default::default()
        });

        workspaces_layer.set_size(lay_rs::types::Size::auto(), None);
        workspaces_layer.set_pointer_events(false);

        layers_engine.add_layer(&workspaces_layer);

        let expose_layer = layers_engine.new_layer();
        expose_layer.set_key("expose");
        expose_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        expose_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        expose_layer.set_pointer_events(false);
        expose_layer.set_hidden(false);
        expose_layer.set_picture_cached(false);
        expose_layer.set_image_cached(false);

        layers_engine.add_layer(&expose_layer);

        let overlay_layer = layers_engine.new_layer();
        overlay_layer.set_key("overlay_view");
        overlay_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            size: taffy::Size {
                width: taffy::Dimension::Percent(1.0),
                height: taffy::Dimension::Percent(1.0),
            },
            ..Default::default()
        });
        overlay_layer.set_pointer_events(false);
        let dnd_view = DndView::new(layers_engine.clone());

        let dock = DockView::new(layers_engine.clone());
        let dock = Arc::new(dock);
        dock.show(None);

        let app_switcher = AppSwitcherView::new(layers_engine.clone());
        let app_switcher = Arc::new(app_switcher);

        let workspace_selector_layer = layers_engine.new_layer();
        workspace_selector_layer.set_pointer_events(false);
        layers_engine.add_layer(&workspace_selector_layer);
        layers_engine.add_layer(&overlay_layer);

        let workspace_selector_view = Arc::new(WorkspaceSelectorView::new(
            layers_engine.clone(),
            workspace_selector_layer.clone(),
        ));

        // layer.add_sublayer(overlay_layer.clone());

        let mut workspaces = Self {
            // layer,
            spaces,
            outputs: Vec::new(),
            model: Arc::new(RwLock::new(model)),
            windows_map: HashMap::new(),
            workspaces_layer,
            expose_layer,
            app_switcher: app_switcher.clone(),
            workspace_selector_view: workspace_selector_view.clone(),
            dock: dock.clone(),
            dnd_view,
            overlay_layer,
            show_all: Arc::new(AtomicBool::new(false)),
            show_desktop: Arc::new(AtomicBool::new(false)),
            show_all_gesture: Arc::new(AtomicI32::new(0)),
            show_desktop_gesture: Arc::new(AtomicI32::new(0)),
            window_views: Arc::new(RwLock::new(HashMap::new())),
            observers: Vec::new(),
            layers_engine,
            expose_dragging_window: Arc::new(std::sync::Mutex::new(None)),
        };
        workspaces.add_workspace();
        workspaces.add_workspace();

        workspaces.add_listener(dock.clone());
        workspaces.add_listener(app_switcher.clone());
        workspaces.add_listener(workspace_selector_view.clone());
        workspaces
    }

    pub fn space(&self) -> &Space<WindowElement> {
        let index = self.with_model(|m| m.current_workspace);
        &self.spaces[index]
    }

    pub fn space_mut(&mut self) -> &mut Space<WindowElement> {
        let index = self.with_model(|m| m.current_workspace);

        &mut self.spaces[index]
    }
    /// Set the workspace screen physical size
    pub fn set_screen_dimension(&self, width: i32, height: i32) {
        let scale = Config::with(|c| c.screen_scale);
        let current_workspace = self.with_model_mut(|model| {
            model.width = width;
            model.height = height;
            model.scale = scale;
            let event = model.clone();
            self.notify_observers(&event);
            model.current_workspace
        });

        self.update_workspaces_layout();
        self.scroll_to_workspace_index(current_workspace, Some(Transition::ease_out_quad(0.0)));
    }

    pub fn get_logical_rect(&self) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        self.with_model(|model| {
            let scale = model.scale as f32;
            smithay::utils::Rectangle::from_loc_and_size(
                (0, 0),
                (
                    (model.width as f32 / scale) as i32,
                    (model.height as f32 / scale) as i32,
                ),
            )
        })
    }
    // Data model management

    pub fn with_model<T>(&self, f: impl FnOnce(&WorkspacesModel) -> T) -> T {
        let model = self.model.read().unwrap();
        f(&model)
    }

    fn update_workspaces_layout(&self) {
        let (width, height, workspaces) = self.with_model(|model| {
            (
                model.width as f32,
                model.height as f32,
                model.workspaces.clone(),
            )
        });

        if width <= 0.0 || height <= 0.0 {
            return;
        }

        self.workspaces_layer
            .set_size(Size::points(width, height), None);
        self.expose_layer
            .set_size(Size::points(width, height), None);

        for (logical_index, workspace) in workspaces.iter().enumerate() {
            workspace.update_layout(logical_index, width, height);
            let selector_layer = workspace.window_selector_view.layer.clone();
            selector_layer.set_size(Size::points(width, height), None);
            selector_layer.set_position((logical_index as f32 * width, 0.0), None);
        }
    }

    pub fn with_model_mut<T>(&self, f: impl FnOnce(&mut WorkspacesModel) -> T) -> T {
        let mut model = self.model.write().unwrap();
        f(&mut model)
    }

    // Gestures

    /// Return if we are in window selection mode
    pub fn get_show_all(&self) -> bool {
        self.show_all.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set the window selection mode
    fn set_show_all(&self, show_all: bool) {
        self.show_all
            .store(show_all, std::sync::atomic::Ordering::Relaxed);
    }

    /// Return if we are in show desktop mode
    pub fn get_show_desktop(&self) -> bool {
        self.show_desktop.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set the show desktop mode
    fn set_show_desktop(&self, show_all: bool) {
        self.show_desktop
            .store(show_all, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the mode to window selection mode using a delta for gestures
    ///
    /// # Arguments
    /// * `delta` - The incremental change value from a gesture:
    ///   - For continuous gestures (e.g., three-finger swipe): use incremental values (typically -1.0 to 1.0 range)
    ///   - For layout updates without mode change: use `0.0` or `1.0` as needed
    /// * `end_gesture` - Whether the gesture/action has completed:
    ///   - `true`: Finalize the transition with animations and state commitment (snaps to nearest state based on threshold)
    ///   - `false`: Track finger movement without animation smoothing (direct 1:1 response during gesture)
    ///
    /// # Behavior
    /// The function uses a hysteresis mechanism: you must swipe at least 10% to enter the mode,
    /// but must swipe back past 90% to exit when already active, preventing accidental toggles.
    ///
    /// # Usage Examples
    /// - Keyboard toggle on: `expose_show_all(1.0, true)`
    /// - Keyboard toggle off: `expose_show_all(-1.0, true)`
    /// - Gesture update mid-swipe: `expose_show_all(0.05, false)` (5% progress increment)
    /// - Gesture completion: `expose_show_all(0.0, true)` (finalize at current position)
    /// - Update layout during window drag: `expose_show_all(0.0, false)` (recalculate without animation)
    pub fn expose_show_all(&self, delta: f32, end_gesture: bool) {
        let current_workspace_index = self.get_current_workspace_index();
        self.expose_show_all_workspace(current_workspace_index, delta, end_gesture);
    }

    /// Process expose mode for a specific workspace
    /// Manages gesture state and delegates to layout/animation functions
    fn expose_show_all_workspace(&self, workspace_index: usize, delta: f32, end_gesture: bool) {
        const MULTIPLIER: f32 = 1000.0;
        let gesture = self
            .show_all_gesture
            .load(std::sync::atomic::Ordering::Relaxed);

        let mut new_gesture = gesture + (delta * MULTIPLIER) as i32;
        let mut show_all = self.get_show_all();
        let previous_show_all = show_all;

        if end_gesture {
            if show_all {
                if new_gesture <= (9.0 * MULTIPLIER / 10.0) as i32 {
                    new_gesture = 0;
                    show_all = false;
                } else {
                    new_gesture = MULTIPLIER as i32;
                    show_all = true;
                }
            } else {
                // animation_duration = 0.200;
                #[allow(clippy::collapsible_else_if)]
                if new_gesture >= (1.0 * MULTIPLIER / 10.0) as i32 {
                    new_gesture = MULTIPLIER as i32;
                    show_all = true;
                } else {
                    new_gesture = 0;
                    show_all = false;
                }
            }
        }

        // Persist desired show_all state immediately on gesture end so that
        // follow-up calls (e.g. focus_app_with_window) don't re-open expose
        if end_gesture {
            self.show_all
                .store(show_all, std::sync::atomic::Ordering::Relaxed);
        }

        let gesture_active = show_all || (!end_gesture && new_gesture > 0);
        
        let delta_normalized = new_gesture as f32 / 1000.0;

        let gesture_changed = new_gesture != gesture || show_all != previous_show_all;

        let should_animate = end_gesture || gesture_changed;

        let transition = if should_animate {
            Some(Transition {
                delay: 0.0,
                timing: TimingFunction::Spring(Spring::with_duration_and_bounce(0.3, 0.1)),
            })
        } else {
            None
        };

        self.show_all_gesture
            .store(new_gesture, std::sync::atomic::Ordering::Relaxed);
        
        // Animate based on current state
        self.expose_show_all_animate(
            workspace_index,
            delta_normalized,
            transition,
            show_all,
            gesture_active,
            end_gesture,
        );
    }

    /// Update the layout bin and window selector state for a workspace
    /// This ensures the bin has correct layout positions for all windows
    /// Returns true when a relayout was performed.
    fn expose_show_all_layout(&self, workspace_index: usize) -> bool {
        let Some(workspace) = self.get_workspace_at(workspace_index) else {
            tracing::warn!("Workspace {} not found for expose layout", workspace_index);
            return false;
        };
        
        // FIXME: remove hardcoded values
        let workspace_selector_height = 250.0;
        let padding_top = 10.0;
        let padding_bottom = 10.0;

        let size = self.workspaces_layer.render_size_transformed();
        let scale = Config::with(|c| c.screen_scale);
        let screen_size_w = size.x;
        let screen_size_h = size.y - padding_top - padding_bottom - workspace_selector_height;

        let offset_y = 200.0;
        let layout_rect = LayoutRect::new(
            0.0,
            workspace_selector_height,
            screen_size_w,
            screen_size_h - offset_y,
        );
        let dragging_window = self.expose_dragging_window.lock().unwrap().clone();
        let windows = self.with_model(|model| {
            if let Some(workspace_model) = model.workspaces.get(workspace_index) {
                let windows_list = workspace_model.windows_list.read().unwrap();
                let space = self.spaces.get(workspace_index).unwrap();
                let mut windows = Vec::new();

                for window_id in windows_list.iter() {
                    if dragging_window.as_ref() == Some(window_id) {
                        continue;
                    }
                    if let Some(window) = self.get_window_for_surface(window_id) {
                        if window.is_minimised() {
                            continue;
                        }
                        if let Some(bbox) = space.element_geometry(window) {
                            let bbox = bbox.to_f64().to_physical(scale);
                            window.mirror_layer().set_size(
                                Size::points(bbox.size.w as f32, bbox.size.h as f32),
                                None,
                            );
                            windows.push(WindowSelectorWindow {
                                id: window_id.clone(),
                                rect: LayoutRect::new(
                                    bbox.loc.x as f32,
                                    bbox.loc.y as f32,
                                    bbox.size.w as f32,
                                    bbox.size.h as f32,
                                ),
                                title: window.xdg_title().to_string(),
                            });
                        }
                    }
                }

                windows
            } else {
                Vec::new()
            }
        });

        // Skip relayout if window set and geometry match previous layout
        if workspace
            .window_selector_view
            .is_layout_up_to_date(&layout_rect, offset_y, &windows)
        {
            return false;
        }

        workspace
            .window_selector_view
            .update_windows(layout_rect, offset_y, &windows);
        true
    }

    /// Animate window positions and UI elements based on current delta and state
    /// This applies interpolated positions/scales to layers and schedules animations
    fn expose_show_all_animate(
        &self,
        workspace_index: usize,
        delta: f32,
        transition: Option<Transition>,
        show_all: bool,
        visible: bool,
        end_gesture: bool,
    ) {
        let delta = delta.clamp(0.0, 1.0);

        let scale = Config::with(|c| c.screen_scale);

        let offset_y = 200.0;
        let mut changes = Vec::new();
        let Some(workspace_view) = self.get_workspace_at(workspace_index) else {
            tracing::warn!("Workspace {} not found for expose animation", workspace_index);
            return;
        };
        let bin = workspace_view.window_selector_view.expose_bin.read().unwrap();
        let dragging_window = self.expose_dragging_window.lock().unwrap().clone();

        // Keep the overlay hidden unless we're animating or expose should be visible
        let overlay_layer = workspace_view.window_selector_view.overlay_layer.clone();
        let is_animating = transition.is_some();
        overlay_layer.set_hidden(!is_animating && !visible);


        workspace_view.window_selector_view.windows_layer.set_hidden(false);
        workspace_view.window_selector_view.layer.set_hidden(false);

        // Create animation if transition is specified
        let animation = transition.as_ref().map(|t| {
            self.layers_engine.add_animation_from_transition(t, false)
        });

        // Animate window layers
        let current_workspace = self.with_model(|model| {
            if let Some(workspace) = model.workspaces.get(workspace_index) {
                let windows_list = workspace.windows_list.read().unwrap();
                let window_selector = workspace.window_selector_view.clone();
                let space = self.spaces.get(workspace_index).unwrap();

                for window_id in windows_list.iter() {
                    if dragging_window.as_ref() == Some(window_id) {
                        continue;
                    }
                    if let Some(window) = self.get_window_for_surface(window_id) {
                        if window.is_minimised() {
                            continue;
                        }
                        if let Some(bbox) = space.element_geometry(window) {
                            let bbox = bbox.to_f64().to_physical(scale);
                            if let Some(rect) = bin.get(window_id) {
                                let to_x = rect.x;
                                let to_y = rect.y + offset_y;
                                let to_width = rect.width;
                                let to_height = rect.height;
                                let (window_width, window_height) =
                                    (bbox.size.w as f32, bbox.size.h as f32);

                                let scale_x = to_width / window_width;
                                let scale_y = to_height / window_height;
                                let target_scale = scale_x.min(scale_y).min(1.0);

                                // Interpolate between current and target positions
                                let scale = 1.0.interpolate(&target_scale, delta);
                                let delta_clamped = delta.clamp(0.0, 1.0);
                                let window_x = bbox.loc.x as f32;
                                let window_y = bbox.loc.y as f32;
                                let x = window_x.interpolate(&to_x, delta_clamped);
                                let y = window_y.interpolate(&to_y, delta_clamped);

                                if let Some(layer) = window_selector.layer_for_window(window_id) {
                                    if let Some(_) = transition {
                                        let translation =
                                            layer.change_position(lay_rs::types::Point { x, y });
                                        let scale_change =
                                            layer.change_scale(lay_rs::types::Point { x: scale, y: scale });
                                        changes.push(translation);
                                        changes.push(scale_change);
                                    } else {
                                        layer.set_position(
                                            lay_rs::types::Point { x, y },
                                            None,
                                        );
                                        layer.set_scale(
                                            lay_rs::types::Point { x: scale, y: scale },
                                            None,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            model.workspaces.get(workspace_index).cloned()
        });

        // Schedule layer changes with animation
        if let Some(anim_ref) = animation {
            let _transactions = self.layers_engine.schedule_changes(&changes, anim_ref);
        }
        ;

        // Animate workspace selector and dock
        let mut delta = delta.max(0.0);
        delta = delta.powf(0.65);

        // Workspace selector
        let workspace_selector_y = (-400.0).interpolate(&0.0, delta);
        let workspace_selector_y = workspace_selector_y.clamp(-400.0, 0.0);
        let workspace_opacity = 0.0.interpolate(&1.0, delta);
        let workspace_opacity = workspace_opacity.clamp(0.0, 1.0);

        if (!end_gesture && delta > 0.0) || animation.is_some() {
            workspace_view.window_selector_view.overlay_layer.set_opacity(0.0, None);
        }

        let window_selector_overlay_ref = workspace_view.window_selector_view.overlay_layer.clone();
        let expose_layer = self.expose_layer.clone();
        let show_all_ref = self.show_all.clone();
        expose_layer.set_hidden(false);
        let transaction = self.workspace_selector_view
            .layer
            .set_position(
                lay_rs::types::Point {
                    x: 0.0,
                    y: workspace_selector_y,
                },
                transition,
            );
        if transition .is_some() {
            transaction
            .on_finish(
                move |_: &Layer, _: f32| {
                    window_selector_overlay_ref.set_opacity(1.0, None);
                    window_selector_overlay_ref.set_hidden(!show_all);
                    expose_layer.set_hidden(!show_all);
                    show_all_ref.store(show_all, std::sync::atomic::Ordering::Relaxed);
                },
                true,
            );
        }
        self.workspace_selector_view
            .layer
            .set_opacity(workspace_opacity, transition);

        // Animate dock position
        if let Some(current_workspace) = current_workspace {
            let mut start_position = 0.0;
            let mut end_position = 250.0;
            if current_workspace.get_fullscreen_mode() {
                start_position = 250.0;
                end_position = 250.0;
            }
            let dock_y = start_position.interpolate(&end_position, delta);
            let dock_y = dock_y.clamp(0.0, 250.0);
            let tr = self.dock.view_layer.set_position((0.0, dock_y), transition);

            if let Some(anim_ref) = animation {
                self.layers_engine.start_animation(anim_ref, 0.0);
            }
            if end_gesture {
                // let mut bin = self.expose_bin.write().unwrap();
                // *bin = HashMap::new();
                let dock_ref = self.dock.clone();
                tr.on_finish(
                    move |_: &Layer, _: f32| {
                        if show_all || current_workspace.get_fullscreen_mode() {
                            dock_ref.hide(None);
                        } else {
                            dock_ref.show(None);
                        }
                    },
                    true,
                );
            }
        }
    }

    /// Recalculates the layout for a single workspace without animation
    /// Used when windows are added/removed/moved between workspaces
    // pub fn expose_recalculate_workspace(&self, workspace_index: usize) {
    //     // Only recalculate if we're in expose mode
    //     if !self.get_show_all() {
    //         return;
    //     }
        
    //     let delta = self.show_all_gesture.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
    //     let transition = Some(Transition {
    //         delay: 0.0,
    //         timing: TimingFunction::Spring(Spring::with_duration_and_bounce(0.3, 0.1)),
    //     });
        
    //     // Force relayout and animate
    //     if self.expose_show_all_layout(workspace_index) && self.get_show_all(){
    //         self.expose_show_all_animate(workspace_index, delta, transition, true, true, true);
    //     }
    // }
    /// Set the mode to show desktop mode using a delta for gestures
    pub fn expose_show_desktop(&self, delta: f32, end_gesture: bool) {
        const MULTIPLIER: f32 = 1000.0;
        let gesture = self
            .show_desktop_gesture
            .load(std::sync::atomic::Ordering::Relaxed);

        let mut new_gesture = gesture + (delta * MULTIPLIER) as i32;
        let show_desktop = self.get_show_desktop();

        let model = self.model.read().unwrap();

        if end_gesture {
            if show_desktop {
                if new_gesture <= (9.0 * MULTIPLIER / 10.0) as i32 {
                    new_gesture = 0;
                    self.set_show_desktop(false);
                } else {
                    new_gesture = MULTIPLIER as i32;
                    self.set_show_desktop(true);
                }
            } else {
                #[allow(clippy::collapsible_else_if)]
                if new_gesture >= (1.0 * MULTIPLIER / 10.0) as i32 {
                    new_gesture = MULTIPLIER as i32;
                    self.set_show_desktop(true);
                } else {
                    new_gesture = 0;
                    self.set_show_desktop(false);
                }
            }
        } else if !show_desktop {
            new_gesture -= MULTIPLIER as i32;
        }

        let delta = new_gesture as f32 / 1000.0;

        let delta = delta.clamp(0.0, 1.0);

        let mut transition = Some(Transition::ease_in(0.5));

        if !end_gesture {
            // in the middle of the gesture
            transition = None;
        }
        let workspace = self.get_current_workspace();
        let windows_list = workspace.windows_list.read().unwrap();
        for window in windows_list.iter() {
            let window = self.get_window_for_surface(window).unwrap();
            if window.is_minimised() {
                continue;
            }
            let bbox = window.bbox();
            let window_width = bbox.size.w as f32;
            let window_height = bbox.size.h as f32;
            let window_x = bbox.loc.x as f32;
            let window_y = bbox.loc.y as f32;
            let to_x = -window_width;
            let to_y = -window_height;
            let x = window_x.interpolate(&to_x, delta);
            let y = window_y.interpolate(&to_y, delta);

            if let Some(view) = self.get_window_view(&window.id()) {
                view.window_layer
                    .set_position(lay_rs::types::Point { x, y }, transition);
            }
        }
    }

    pub fn expose_update_if_needed(&self) {
        let current_workspace_index = self.get_current_workspace_index();
        self.expose_update_if_needed_workspace(current_workspace_index);
    }
    pub fn expose_update_if_needed_workspace(&self, workspace_index: usize) {
        let relayout = self.expose_show_all_layout(workspace_index);
        if self.get_show_all() && relayout {
            let transition = Some(Transition {
                delay: 0.0,
                timing: TimingFunction::Spring(Spring::with_duration_and_bounce(0.3, 0.1)),
            });
            self.expose_show_all_animate(workspace_index, 1.0, transition, true, true, true);
        }
    }
    /// Close all the windows of an app by its id
    pub fn quit_app(&self, app_id: &str) {
        for window_id in self.get_app_windows(app_id) {
            let window = self.get_window_for_surface(&window_id);
            if let Some(we) = window {
                match we.underlying_surface() {
                    WindowSurface::Wayland(t) => t.send_close(),
                    #[cfg(feature = "xwayland")]
                    WindowSurface::X11(w) => {
                        let _ = w.close();
                    }
                }
            }
        }
    }

    /// Close all the windows of the current focused App
    pub fn quit_current_app(&self) {
        if let Some(app_id) = self.get_current_app_id() {
            self.quit_app(&app_id);
        }
    }

    /// Close all the windows of the current focused App in th app switcher
    pub fn quit_appswitcher_app(&self) {
        if let Some(app_id) = self.app_switcher.get_current_app_id() {
            self.quit_app(&app_id);
        }
    }

    /// Minimise a WindowElement
    pub fn minimize_window(&mut self, we: &WindowElement) {
        let id = we.id();

        if let Some(window) = self.windows_map.get_mut(&id) {
            window.set_is_minimised(true);
        }

        self.with_model_mut(|model| {
            model
                .minimized_windows
                .push((id.clone(), we.xdg_title().to_string()));

            if let Some(view) = self.get_window_view(&id) {
                let (drawer, _) = self.dock.add_window_element(we);

                view.window_layer.set_layout_style(taffy::Style {
                    position: taffy::Position::Absolute,
                    ..Default::default()
                });

                // Hide mirror layer so it won't appear in expose
                view.mirror_layer.set_hidden(true);

                self.layers_engine
                    .add_layer_to_positioned(view.window_layer.clone(), Some(drawer.id));
                // bounds are calculate after this call
                let drawer_bounds = drawer.render_bounds_transformed();
                view.minimize(skia::Rect::from_xywh(
                    drawer_bounds.x(),
                    drawer_bounds.y(),
                    drawer_bounds.width(),
                    drawer_bounds.height(),
                ));

                let view_ref = view.clone();
                drawer.clear_on_change_size_handlers();
                drawer.on_change_size(
                    move |layer: &Layer, _| {
                        let bounds = layer.render_bounds_transformed();
                        // Keep the minimized window scaled to the drawer bounds
                        view_ref.apply_minimized_scale(bounds);
                    },
                    false,
                );
            }

            self.notify_observers(model);
        });
        we.set_activate(false);

        // ideally we set the focus to the next (non-minimized) window in the stack
        let index = self.with_model(|m| m.current_workspace);
        let windows: Vec<_> = self.spaces[index]
            .elements()
            .filter_map(|e| {
                let id = e.id();
                if let Some(window) = self.windows_map.get(&id) {
                    if window.is_minimised() {
                        return None;
                    }
                }
                Some(id)
            })
            .collect();

        let win_len = windows.len();
        if win_len <= 1 {
            return;
        }

        for (i, wid) in windows.iter().enumerate() {
            let activate = i == win_len - 2;
            // if !wid.is_minimized {
            self.raise_element(wid, activate, false);
            // }
        }
    }

    /// Unminimise a WindowElement
    pub fn unminimize_window(&mut self, wid: &ObjectId) {
        let workspace_for_window = self.with_model(|model| {
            model
                .workspaces
                .iter()
                .position(|ws| ws.windows_list.read().unwrap().contains(wid))
        });
        if workspace_for_window.is_none() {
            tracing::warn!("Trying to unminimize a window that is not in any workspace: {}", wid);
            return;
        }
        let workspace_for_window = workspace_for_window.unwrap();
        let current_workspace_index = self.get_current_workspace_index();

        let ctx = match self.build_unminimize_context(wid) {
            Some(ctx) => ctx,
            None => return,
        };

        if workspace_for_window != current_workspace_index {
            if let Some(tr) = self.set_current_workspace_index(
                workspace_for_window,
                Some(Transition::ease_out_quad(0.2)),
            ) {
                let ctx_clone = ctx.clone();
                tr.on_finish(
                    move |_: &Layer, _: f32| {
                        ctx_clone.run();
                    },
                    true,
                );
                return;
            }
        }

        self.unminimize_window_in_workspace(ctx);
    }

    fn unminimize_window_in_workspace(&self, ctx: UnminimizeContext) {
        ctx.run();
    }

    fn build_unminimize_context(&self, wid: &ObjectId) -> Option<UnminimizeContext> {
        let scale = Config::with(|c| c.screen_scale) as f32;
        let (index, space) = self
            .spaces
            .iter()
            .enumerate()
            .find(|(_, space)| space.elements().any(|e| e.id() == *wid))?;

        let workspace = self.with_model(|m| m.workspaces[index].clone());
        let window = self.get_window_for_surface(wid)?.clone();
        let view = self.get_window_view(wid)?;
        let window_geometry = space.element_geometry(&window)?;
        let pos_x = window_geometry.loc.x;
        let pos_y = window_geometry.loc.y;
        let layer_pos_x = pos_x as f32 * scale;
        let layer_pos_y = pos_y as f32 * scale;

        Some(UnminimizeContext {
            wid: wid.clone(),
            workspace,
            window,
            view,
            dock: self.dock.clone(),
            layers_engine: self.layers_engine.clone(),
            expose_layer: self.expose_layer.clone(),
            model: self.model.clone(),
            observers: self.observers.clone(),
            layer_pos: (layer_pos_x, layer_pos_y),
            pos_logical: (pos_x, pos_y),
        })
    }

    // Helpers / Windows Management

    /// Determine the initial placement of a new window within the workspace.
    /// It calculates the appropriate position and bounds for the window based
    /// on the current pointer location and the output geometry under the pointer.
    pub fn new_window_placement_at(
        &self,
        pointer_location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> (
        smithay::utils::Rectangle<i32, smithay::utils::Logical>,
        smithay::utils::Point<i32, smithay::utils::Logical>,
    ) {
        let output = self
            .output_under(pointer_location)
            .next()
            .or_else(|| self.outputs().next())
            .cloned();
        let output_geometry = output
            .and_then(|o| {
                let geo = self.output_geometry(&o)?;
                let map = layer_map_for_output(&o);
                let zone = map.non_exclusive_zone();
                Some(Rectangle::from_loc_and_size(geo.loc + zone.loc, zone.size))
            })
            .unwrap_or_else(|| Rectangle::from_loc_and_size((0, 0), (800, 800)));

        let num_open_windows = self.spaces_elements().count();
        let window_index = num_open_windows + 1; // Index of the new window

        let max_x = output_geometry.loc.x + output_geometry.size.w;
        let max_y = output_geometry.loc.y + output_geometry.size.h;

        // Calculate the position along the diagonal
        const MAX_WINDOW_COUNT: f32 = 40.0;
        let factor = window_index as f32 / MAX_WINDOW_COUNT;
        let x = (output_geometry.loc.x as f32 + factor * max_x as f32) as i32 + 100;
        let y = (output_geometry.loc.y as f32 + factor * max_y as f32) as i32 + 100;

        (output_geometry, (x, y).into())
    }

    /// map the window element, in the position on the current space,
    /// should be called on every window move / drag
    /// sets the position of the window layer in the scene
    pub fn map_window(
        &mut self,
        window_element: &WindowElement,
        location: impl Into<smithay::utils::Point<i32, smithay::utils::Logical>>,
        activate: bool,
    ) {
        self.space_mut()
            .map_element(window_element.clone(), location, activate);
        // self.space_mut().refresh();

        if let std::collections::hash_map::Entry::Vacant(e) =
            self.windows_map.entry(window_element.id())
        {
            e.insert(window_element.clone());

            self.update_workspace_model();
        }

        // append the window to the workspace layer
        {
            let location = self.element_location(window_element).unwrap_or_default();

            let workspace_view = self.get_current_workspace();

            workspace_view.map_window(window_element, location);
            let _view = self.get_or_add_window_view(window_element);
        }
        self.refresh_space();
        self.expose_update_if_needed();
    }

    /// remove a WindowElement from the workspace model,
    /// remove the window layer from the scene,
    pub fn unmap_window(&mut self, window_id: &ObjectId) {
        tracing::info!("workspaces::unmap_window: {:?}", window_id);

        let mut workspace_index = None;
        
        if let Some(element) = self.get_window_for_surface(window_id).cloned() {
            for (i, space) in self.spaces.iter_mut().enumerate() {
                if space.elements().any(|e| e.id() == element.id()) {
                    workspace_index = Some(i);
                }
                space.unmap_elem(&element);
            }
        }

        self.with_model(|m| {
            for workspace_view in m.workspaces.iter() {
                workspace_view.unmap_window(window_id);
            }
        });
        self.windows_map.remove(window_id);
        // Remove debug texture snapshot for this surface
        crate::textures_storage::remove(window_id);
        self.remove_window_view(window_id);

        self.refresh_space();
        self.update_workspace_model();
        
        // Recalculate expose layout if in expose mode
        if let Some(index) = workspace_index {
            self.expose_update_if_needed_workspace(index);
        }
    }
    /// Return if the current coordinates are over the dock
    pub fn is_cursor_over_dock(&self, x: f32, y: f32) -> bool {
        self.dock.alive()
            && self
                .dock
                .view_layer
                .render_bounds_transformed()
                .contains(skia::Point::new(x, y))
    }

    /// Return the list of WlSurface ids of an app by its id
    pub fn get_app_windows(&self, app_id: &str) -> Vec<ObjectId> {
        let model = self.model.read().unwrap();
        model
            .app_windows_map
            .get(app_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Return the list of Spaces where an app has windows by its id
    pub fn get_app_spaces(&self, app_id: &str) -> Vec<&Space<WindowElement>> {
        let model = self.model.read().unwrap();
        let mut spaces = Vec::new();

        model
            .app_windows_map
            .get(app_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .for_each(|id| {
                let window = self.get_window_for_surface(id);
                if let Some(we) = window {
                    for space in self.spaces.iter() {
                        if space.elements().any(|e| e == we) {
                            spaces.push(space);
                            break;
                        }
                    }
                }
            });

        spaces
    }

    /// Return the current focused Application
    pub fn get_current_app_id(&self) -> Option<String> {
        let model = self.model.read().unwrap();
        model.zindex_application_list.last().cloned()
    }

    /// Return the list of WlSurface ids of the current focused Application
    pub fn get_current_app_windows(&self) -> Vec<ObjectId> {
        self.get_current_app_id()
            .map(|app_id| self.get_app_windows(&app_id))
            .unwrap_or_default()
    }

    /// Return the Window object of WlSurface by its id
    pub fn get_window_for_surface(&self, id: &ObjectId) -> Option<&WindowElement> {
        self.windows_map.get(id)
    }

    pub fn get_or_add_window_view(
        &self,
        // object_id: &ObjectId,
        window: &WindowElement,
    ) -> WindowView {
        let mut window_views = self.window_views.write().unwrap();
        let wid = window.id();
        let entry = window_views
            .entry(wid.clone())
            .or_insert_with(|| WindowView::new(self.layers_engine.clone(), window));
        entry.clone()
    }

    /// Remove a WindowView from the scene and delete it from the window_views map
    pub fn remove_window_view(&self, object_id: &ObjectId) {
        let mut window_views = self.window_views.write().unwrap();
        if let Some(view) = window_views.remove(object_id) {
            view.window_layer.remove();
        }
    }

    pub fn get_window_view(&self, id: &ObjectId) -> Option<WindowView> {
        let window_views = self.window_views.read().unwrap();

        window_views.get(id).cloned()
    }

    pub fn set_window_view(&self, id: &ObjectId, window_view: WindowView) {
        let mut window_views = self.window_views.write().unwrap();

        window_views.insert(id.clone(), window_view);
    }

    /// unmap the window from the current space and workspaceview
    /// map the window to the new space and workspaceview
    pub fn move_window_to_workspace(
        &mut self,
        we: &WindowElement,
        workspace_index: usize,
        location: impl Into<smithay::utils::Point<i32, smithay::utils::Logical>>,
    ) {
        let location = location.into();

        let mut source_workspace_index = None;
        
        // unmap from old space
        if let Some((index, space)) = self
            .spaces
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.elements().any(|e| e.id() == we.id()))
        {
            source_workspace_index = Some(index);
            space.unmap_elem(we);
            let id = we.id();
            let model = self.model.read().unwrap();
            if let Some(workspace) = model.workspaces.get(index) {
                // Don't remove mirror layer during move - it causes SlotMap key issues
                // The expose view will be rebuilt with updated state after the move
                workspace.unmap_window_internal(&id);
            }
        }

        // map to new space

        if let Some(space) = self.spaces.get_mut(workspace_index) {
            tracing::info!(
                "workspaces::move_window_to_workspace: {:?}",
                workspace_index
            );
            space.map_element(we.clone(), location, false);
            let id = we.wl_surface().unwrap().id();
            let model = self.model.read().unwrap();
            if let Some(workspace) = model.workspaces.get(workspace_index) {
                if let Some(window) = self.windows_map.get(&id) {
                    workspace.map_window(window, location);
                }
            }
        }
        
        // Recalculate layout for both source and target workspaces if in expose mode
        if let Some(source_index) = source_workspace_index {
            self.expose_update_if_needed_workspace(source_index);
            self.expose_update_if_needed_workspace(workspace_index);
        }
    }

    pub fn raise_next_app_window(&mut self) -> Option<ObjectId> {
        let windows = self.get_current_app_windows();
        let mut wid = None;
        if !windows.is_empty() {
            for (i, window_id) in windows.iter().enumerate() {
                if i == 0 {
                    self.raise_element(window_id, true, true);
                    wid = Some(window_id.clone());
                }
            }
        }
        wid
    }

    pub fn raise_prev_app_window(&mut self) -> Option<ObjectId> {
        let windows = self.get_current_app_windows();
        let mut wid = None;
        if !windows.is_empty() {
            let current_window = (windows.len() as i32) - 1;
            let current_window = std::cmp::max(current_window, 0) as usize;
            for (i, window_id) in windows.iter().enumerate() {
                if i == current_window {
                    self.raise_element(window_id, true, true);
                    wid = Some(window_id.clone());
                }
            }
        }
        wid
    }

    /// Raise thw windowelement on top of all the windows in its space
    /// activate: will set the window as active
    /// update: will update the workspace model
    fn raise_element(&mut self, window_id: &ObjectId, activate: bool, update: bool) {
        // get the space with the window
        // tracing::info!("workspaces::raise_element: {:?}", window_id);
        if let Some((index, space)) = self
            .spaces
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.elements().any(|e| e.id() == *window_id))
        {
            if let Some(window) = self.windows_map.get(window_id) {
                if space.elements().last().unwrap().id() == *window_id {
                    return;
                }
                if window.is_minimised() && !activate {
                    return;
                }
                // FIXME: this is a hack to prevent raising a window that is already fullscreen
                // ideally we avoid resort a window already on top
                if window.is_fullscreen() {
                    return;
                }
                space.raise_element(window, activate);
                let workspace = self.with_model(|m| m.workspaces[index].clone());
                {
                    if let Some(view) = self.get_window_view(window_id) {
                        workspace.windows_layer.add_sublayer(&view.window_layer);
                    }
                    if let Some(layer) = workspace.window_selector_view.layer_for_window(window_id)
                    {
                        workspace
                            .window_selector_view
                            .windows_layer
                            .add_sublayer(&layer);
                    }
                }
                if update {
                    self.update_workspace_model();
                }
            }
        } else {
            tracing::warn!("workspaces::raise_element: window not found");
        }
    }

    /// Raise all the windows of a given app
    /// returns the window id of the last window raised, if any
    fn raise_app_elements(
        &mut self,
        app_id: &str,
        focus_window: Option<&ObjectId>,
    ) -> Option<ObjectId> {
        // for every window in the app, raise it
        let windows = self.get_app_windows(app_id);
        let mut focus_wid = None;
        for (i, window_id) in windows.iter().enumerate() {
            if let Some(we) = self.get_window_for_surface(window_id) {
                if !we.is_minimised() {
                    if i == windows.len() - 1 {
                        self.raise_element(window_id, true, false);
                        focus_wid = Some(window_id.clone());
                    } else {
                        self.raise_element(window_id, false, false);
                    }
                } else {
                    // if minimised and there is only one window in the app, unminimize it
                    if windows.len() == 1 {
                        self.unminimize_window(window_id);
                    }
                }
            }
        }
        focus_window.map(|wid| {
            if let Some(we) = self.get_window_for_surface(wid) {
                if !we.is_minimised() {
                    self.raise_element(wid, true, false);
                    focus_wid = Some(wid.clone());
                }
            }
        });

        focus_wid
    }

    /// Raise all the windows of a given app
    /// returns the window id of the last window raised, and set to active, if any
    pub fn focus_app(&mut self, app_id: &str) -> Option<ObjectId> {
        tracing::trace!("workspaces::focus_app: {:?}", app_id);
        let wid = self.raise_app_elements(app_id, None);
        if wid.is_none() {
            // return early
            return wid;
        }
        let wid = wid.unwrap();
        let current_space_index = self.with_model(|m| m.current_workspace);
        let index = self
            .spaces
            .iter()
            .position(|s| s.elements().any(|e| e.id() == wid))
            .unwrap_or(current_space_index);

        self.set_current_workspace_index(index, None);

        Some(wid)
    }

    pub fn focus_app_with_window(&mut self, wid: &ObjectId) -> Option<ObjectId> {
        let app_id = self
            .get_window_for_surface(wid)
            .map(|w| w.xdg_app_id())
            .unwrap_or_default();
        tracing::info!("workspaces::focus_app_with_window {:?}", app_id);
        let wid = self.raise_app_elements(&app_id, Some(wid));
        if wid.is_none() {
            // return early
            return wid;
        }
        let wid = wid.unwrap();
        let current_space_index = self.with_model(|m| m.current_workspace);
        let index = self
            .spaces
            .iter()
            .position(|s| s.elements().any(|e| e.id() == wid))
            .unwrap_or(current_space_index);

        self.set_current_workspace_index(index, None);

        Some(wid)
    }

    /// Update the workspace model using elements from Space: windows_list, app_windows_map, zindex_application_list
    /// - app_windows_map: is a map of app_id to a list of toplevel surfaces
    /// - applications_list: is the list of app_id in the order they are opened
    /// - zindex_application_list: is the list of app_id in the order they are in the zindex
    pub(crate) fn update_workspace_model(&self) {
        let windows: Vec<(ObjectId, WindowElement)> = self
            .spaces_elements()
            .map(|we| (we.wl_surface().unwrap().id(), we.clone()))
            .collect();

        {
            // reset the model
            if let Ok(mut model_mut) = self.model.write() {
                model_mut.zindex_application_list = Vec::new();
                model_mut.app_windows_map = HashMap::new();
            } else {
                return;
            }
        }

        let mut app_set = HashSet::new();
        for (window_id, we) in windows.iter() {
            let app_id = we.xdg_app_id();
            if app_id.is_empty() {
                continue;
            }
            if let Ok(mut model_mut) = self.model.write() {
                model_mut
                    .app_windows_map
                    .entry(app_id.clone())
                    .or_default()
                    .push(window_id.clone());

                if !model_mut.application_list.contains(&app_id) {
                    model_mut.application_list.push_front(app_id.clone());
                }
                if app_set.insert(app_id.clone()) {
                    model_mut.zindex_application_list.push(app_id.clone());
                }
            }
        }

        // keep only app in application_list that are in zindex_application_list
        {
            let mut model: std::sync::RwLockWriteGuard<'_, WorkspacesModel> =
                self.model.write().unwrap();

            let app_list = model.zindex_application_list.clone();
            {
                // update app list
                model
                    .application_list
                    .retain(|app_id| app_list.contains(app_id));
            }

            {
                // update minimized windows
                model
                    .minimized_windows
                    .retain(|(id, _)| windows.iter().any(|(wid, _)| wid == id));
            }
        }

        let model = self.model.read().unwrap();
        let event = model.clone();

        self.notify_observers(&event);
    }

    /// Returns all the window elements from all the spaces
    /// starting from current space
    pub fn spaces_elements(&self) -> impl DoubleEndedIterator<Item = &WindowElement> {
        let current_space = self.space();

        self.spaces
            .iter()
            .filter(move |s| s != &current_space)
            .chain(std::iter::once(current_space))
            .flat_map(|space| space.elements())
    }

    // Outputs Management

    /// Returns the list of outputs associated with the current workspace
    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.space().outputs()
    }

    /// Attach a new output to every workspace
    pub fn map_output(
        &mut self,
        output: &Output,
        location: impl Into<smithay::utils::Point<i32, smithay::utils::Logical>>,
    ) {
        let location = location.into();

        self.outputs.push(output.clone());
        // add the new output to every space
        for space in self.spaces.iter_mut() {
            space.map_output(output, location);
        }
    }

    /// Detach an output from every workspace
    pub fn unmap_output(&mut self, output: &Output) {
        self.outputs.retain(|o| o != output);
        // remove the new output from every space
        for space in self.spaces.iter_mut() {
            space.unmap_output(output);
        }
    }

    // Workspaces Management

    pub fn add_workspace(&mut self) -> (usize, Arc<WorkspaceView>) {
        let mut new_space = Space::default();

        if !self.spaces.is_empty() {
            // init the space with the current outputs
            let current_space = self.space();
            for output in self.outputs.iter() {
                let geo = current_space.output_geometry(output).unwrap();
                new_space.map_output(output, geo.loc);
            }
        }

        self.spaces.push(new_space);
        let (index, workspace) = self.with_model_mut(|m| {
            m.workspace_counter += 1;

            let workspace = Arc::new(WorkspaceView::new(
                m.workspace_counter,
                self.layers_engine.clone(),
                &self.workspaces_layer,
                self.overlay_layer.clone(),
            ));
            self.expose_layer
                .add_sublayer(&workspace.window_selector_view.layer);

            m.workspaces.push(workspace.clone());
            self.notify_observers(m);
            (m.workspaces.len() - 1, workspace)
        });
        self.update_workspaces_layout();
        (index, workspace)
    }

    pub fn get_next_free_workspace(&mut self) -> (usize, Arc<WorkspaceView>) {
        let current_workspace = self.get_current_workspace_index();
        if current_workspace < self.spaces.len() - 1 {
            for i in current_workspace + 1..self.spaces.len() {
                if self.spaces[i].elements().count() == 0 {
                    return (i, self.with_model(|m| m.workspaces[i].clone()));
                }
            }
        }
        self.add_workspace()
    }

    pub fn remove_workspace_at(&mut self, n: usize) {
        if self.spaces.len() == 1 {
            return;
        }

        let workspace_model = self.with_model_mut(|m| {
            if m.workspaces.len() == 1 {
                return m.clone();
            }

            if n < m.workspaces.len() {
                m.workspaces.remove(n);
                if m.current_workspace >= m.workspaces.len() {
                    m.current_workspace = m.workspaces.len() - 1;
                }
            }
            m.clone()
        });

        if n < self.spaces.len() {
            // move all windows to previous workspace
            let space_to_remove = self.spaces.remove(n);
            for e in space_to_remove.elements() {
                let location = space_to_remove.element_location(e).unwrap_or_default();
                // Drop fullscreen state so the window restores to its normal size on the target workspace
                if e.is_fullscreen() {
                    e.set_fullscreen(false, workspace_model.current_workspace);
                    
                    if let Some(ws) = self.get_workspace_at(workspace_model.current_workspace) {
                        ws.set_fullscreen_mode(false);
                    }
                }
                self.move_window_to_workspace(e, workspace_model.current_workspace, location);
            }
        }
        self.update_workspaces_layout();
        self.scroll_to_workspace_index(workspace_model.current_workspace, None);
        self.notify_observers(&workspace_model);
    }

    pub fn get_workspace_at(&self, i: usize) -> Option<Arc<WorkspaceView>> {
        self.with_model(|m| m.workspaces.get(i).cloned())
    }

    pub fn get_current_workspace(&self) -> Arc<WorkspaceView> {
        self.with_model(|m| m.workspaces[m.current_workspace].clone())
    }

    pub fn get_current_workspace_index(&self) -> usize {
        self.with_model(|m| m.current_workspace)
    }
    pub fn set_current_workspace_index(&mut self, i: usize, transition: Option<Transition>) -> Option<TransactionRef> {
        if i > self.spaces.len() - 1 {
            return None;
        }
        self.with_model_mut(|m| {
            if i > m.workspaces.len() - 1 {
                return;
            }
            m.current_workspace = i;
        });
        self.update_workspace_model();
        self.scroll_to_workspace_index(i, transition)
    }
    /// Scroll to the workspace at index i, default transition is 1.0s spring
    fn scroll_to_workspace_index(&self, i: usize, transition: Option<Transition>) -> Option<TransactionRef> {
        let transition = transition.unwrap_or(Transition {
            delay: 0.0,
            timing: TimingFunction::Spring(Spring::with_duration_and_bounce(1.0, 0.1)),
        });
        let mut x = 0.0;
        if let Some(workspace) = self.get_workspace_at(i) {
            if workspace.get_fullscreen_mode() || self.get_show_all() {
                self.dock.hide(Some(transition));
            } else {
                self.dock.show(Some(transition));
            }
            if self.get_show_all() {
                self.expose_show_all_workspace(i, 1.0, true);
            }

            let workspace_width = self.with_model(|m| m.width as f32);
            if workspace_width > 0.0 {
                x = i as f32 * workspace_width;
            } else {
                x = workspace
                    .workspace_layer
                    .render_layer()
                    .local_transformed_bounds
                    .left();
            }
        }

        self.apply_scroll_offset(x, Some(transition))
    }

    // Space management

    pub fn outputs_for_element(&self, element: &WindowElement) -> Vec<Output> {
        self.space().outputs_for_element(element)
    }

    fn apply_scroll_offset(
        &self,
        offset: f32,
        transition: Option<Transition>,
    ) -> Option<TransactionRef> {
        if !offset.is_finite() {
            return None;
        }
        if let Some(transition) = &transition {
            let animation = self
                .workspaces_layer
                .engine
                .add_animation_from_transition(transition, true);
            let change1 = self.workspaces_layer.change_position((-offset, 0.0));
            let change2 = self.expose_layer.change_position((-offset, 0.0));
            let changes = vec![change1, change2];
            return self
                .workspaces_layer
                .engine
                .schedule_changes(&changes, animation)
                .into_iter()
                .next();
        }
        None
    }

    pub fn element_under(
        &self,
        point: impl Into<smithay::utils::Point<f64, smithay::utils::Logical>>,
    ) -> Option<(
        &WindowElement,
        smithay::utils::Point<i32, smithay::utils::Logical>,
    )> {
        self.space().element_under(point)
    }

    pub fn output_geometry(
        &self,
        output: &Output,
    ) -> Option<smithay::utils::Rectangle<i32, smithay::utils::Logical>> {
        self.space().output_geometry(output)
    }

    pub fn refresh_space(&mut self) {
        self.space_mut().refresh();
    }

    pub fn element_location(
        &self,
        we: &WindowElement,
    ) -> Option<smithay::utils::Point<i32, smithay::utils::Logical>> {
        self.space().element_location(we)
    }

    pub fn output_under<P: Into<smithay::utils::Point<f64, smithay::utils::Logical>>>(
        &self,
        point: P,
    ) -> impl Iterator<Item = &Output> {
        self.space().output_under(point)
    }

    pub fn element_geometry(
        &self,
        we: &WindowElement,
    ) -> Option<smithay::utils::Rectangle<i32, smithay::utils::Logical>> {
        self.space().element_geometry(we)
    }

    // Add these helper methods
    fn find_space_for_element(&self, element: &WindowElement) -> Option<&Space<WindowElement>> {
        self.spaces
            .iter()
            .find(|space| space.elements().any(|e| e.id() == element.id()))
    }

    fn find_space_index_for_element(&self, element: &WindowElement) -> Option<usize> {
        self.spaces
            .iter()
            .position(|space| space.elements().any(|e| e.id() == element.id()))
    }
}

#[derive(Clone)]
struct UnminimizeContext {
    wid: ObjectId,
    workspace: Arc<WorkspaceView>,
    window: WindowElement,
    view: WindowView,
    dock: Arc<DockView>,
    layers_engine: Arc<Engine>,
    expose_layer: Layer,
    model: Arc<RwLock<WorkspacesModel>>,
    observers: Vec<Weak<dyn Observer<WorkspacesModel>>>,
    layer_pos: (f32, f32),
    pos_logical: (i32, i32),
}

impl UnminimizeContext {
    fn run(&self) {
        let wid = self.wid.clone();
        let workspace = self.workspace.clone();
        let window = self.window.clone();
        let view = self.view.clone();
        let dock = self.dock.clone();
        let layers_engine = self.layers_engine.clone();
        let expose_layer = self.expose_layer.clone();
        let model = self.model.clone();
        let observers = self.observers.clone();
        let layer_pos = self.layer_pos;
        let pos_logical = self.pos_logical;

        let event = {
            let mut model = model.write().unwrap();
            model.minimized_windows.retain(|(w, _title)| w != &wid);
            model.clone()
        };

        window.set_is_minimised(false);

        if let Some(drawer) = dock.remove_window_element(&wid) {
            let windows_layer_ref = workspace.windows_layer.clone();
            let expose_windows_ref = expose_layer.clone();
            let layer_ref = view.window_layer.clone();
            let mirror_ref = view.mirror_layer.clone();
            let target_pos = layer_pos;
            layer_ref.set_hidden(true);
            mirror_ref.set_hidden(true);

            layers_engine.update(0.0);

            let drawer_bounds = drawer.render_bounds_transformed();

            drawer
                .set_size(
                    Size::points(0.0, 130.0),
                    Transition {
                        delay: 0.2,
                        timing: TimingFunction::ease_out_quad(0.3),
                    },
                )
                .on_start(
                    move |_layer: &Layer, _| {
                        layer_ref.remove_draw_content();
                        windows_layer_ref.add_sublayer(&layer_ref);
                        expose_windows_ref.add_sublayer(&mirror_ref);
                        layer_ref.set_position(target_pos, None);
                    },
                    true,
                )
                .then(move |layer: &Layer, _| {
                    layer.remove();
                });

            view.unminimize(drawer_bounds);

            // Make sure the mirror layer is visible again for expose
            view.mirror_layer.set_hidden(false);
        }

        window.set_activate(true);
        workspace.map_window(&window, (pos_logical.0, pos_logical.1).into());

        crate::utils::notify_observers(&observers, &event);
    }
}

impl Observable<WorkspacesModel> for Workspaces {
    fn add_listener(&mut self, observer: std::sync::Arc<dyn Observer<WorkspacesModel>>) {
        let observer = std::sync::Arc::downgrade(&observer);
        self.observers.push(observer);
    }

    fn observers<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = std::sync::Weak<dyn Observer<WorkspacesModel>>> + 'a> {
        Box::new(self.observers.iter().cloned())
    }
}
