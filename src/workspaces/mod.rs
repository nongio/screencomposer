use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hasher,
    sync::{
        atomic::{AtomicBool, AtomicI32},
        Arc, RwLock, Weak,
    },
};

use apps_info::Application;
use lay_rs::{
    engine::Engine, prelude::{taffy, Interpolate, Layer, Spring, TimingFunction, Transition}, skia::{self, Contains}, types::Size
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
pub use window_selector::{WindowSelection, WindowSelectorState, WindowSelectorView};
pub use window_view::{WindowView, WindowViewBaseModel, WindowViewSurface};

pub use app_switcher::AppSwitcherView;
pub use dnd_view::DndView;
pub use dock::DockView;
pub use workspace_selector::WorkspaceSelectorView;

use crate::{
    config::Config,
    shell::WindowElement,
    utils::{
        natural_layout::{natural_layout, LayoutRect}, Observable, Observer,
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
    pub expose_bin: Arc<RwLock<HashMap<ObjectId, LayoutRect>>>,
    pub show_all_gesture: Arc<AtomicI32>,
    pub show_desktop_gesture: Arc<AtomicI32>,

    // layers
    pub layers_engine: Arc<Engine>,
    pub overlay_layer: Layer,
    pub workspaces_layer: Layer,
    pub expose_layer: Layer,
    observers: Vec<Weak<dyn Observer<WorkspacesModel>>>,
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
    pub fn new(layers_engine: Arc<Engine>) -> Self {
        let model = WorkspacesModel::default();
        let spaces = Vec::new();

        let workspaces_layer = layers_engine.new_layer();
        workspaces_layer.set_key("workspaces");
        workspaces_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            flex_wrap: taffy::FlexWrap::NoWrap,
            gap: taffy::Size::length(100.0),
            ..Default::default()
        });

        workspaces_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        workspaces_layer.set_pointer_events(false);

        layers_engine.add_layer(&workspaces_layer);

        let expose_layer = layers_engine.new_layer();
        expose_layer.set_key("expose");
        expose_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            // display: taffy::Display::Flex,
            // flex_direction: taffy::FlexDirection::Row,
            // flex_wrap: taffy::FlexWrap::NoWrap,
            // gap: taffy::Size::length(100.0),
            ..Default::default()
        });
        expose_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        expose_layer.set_pointer_events(false);
        expose_layer.set_hidden(true);

        layers_engine.add_layer(&expose_layer);

        let overlay_layer = layers_engine.new_layer();
        overlay_layer.set_key("overlay_view");
        overlay_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
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
            expose_bin: Arc::new(RwLock::new(HashMap::new())),
            show_all_gesture: Arc::new(AtomicI32::new(0)),
            show_desktop_gesture: Arc::new(AtomicI32::new(0)),
            window_views: Arc::new(RwLock::new(HashMap::new())),
            observers: Vec::new(),
            layers_engine,
        };
        workspaces.add_workspace();
        // workspaces.add_workspace();
        // workspaces.add_workspace();
        // workspaces.add_workspace();

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
        self.with_model_mut(|model| {
            model.width = width;
            model.height = height;
            model.scale = scale;
            let event = model.clone();
            self.notify_observers(&event);
        });
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
    pub fn expose_show_all(&self, delta: f32, end_gesture: bool) {
        const MULTIPLIER: f32 = 1000.0;
        let gesture = self
            .show_all_gesture
            .load(std::sync::atomic::Ordering::Relaxed);

        let mut new_gesture = gesture + (delta * MULTIPLIER) as i32;
        let mut show_all = self.get_show_all();

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
        if show_all {
            self.expose_layer.set_hidden(false);
        }

        let delta = new_gesture as f32 / 1000.0;

        let mut transition = Some(Transition {
            delay: 0.0,
            timing: TimingFunction::Spring(Spring::with_duration_and_bounce(0.3, 0.1)),
        });
        if !end_gesture {
            // in the middle of the gesture
            transition = None;
        }
        let animation = transition.map(|t| self.layers_engine.add_animation_from_transition(t, false));

        self.show_all_gesture
            .store(new_gesture, std::sync::atomic::Ordering::Relaxed);

        // FIXME: remove hardcoded values
        let workspace_selector_height = 250.0;
        let padding_top = 10.0;
        let padding_bottom = 10.0;

        let size = self.workspaces_layer.render_size();
        let scale = Config::with(|c| c.screen_scale);
        let screen_size_w = size.x;
        let screen_size_h = size.y - padding_top - padding_bottom - workspace_selector_height;

        let mut changes = Vec::new();
        let mut bin = self.expose_bin.write().unwrap();

        let offset_y = 200.0;
        let layout_rect = LayoutRect::new(
            0.0,
            workspace_selector_height,
            screen_size_w,
            screen_size_h - offset_y,
        );
        let init_layout = bin.is_empty();
        let current_workspace = self.with_model(|model| {
            for (i, workspace) in model.workspaces.iter().enumerate() {
                let windows_list = workspace.windows_list.read().unwrap();

                let window_selector = workspace.window_selector_view.clone();
                let space = self.spaces.get(i).unwrap();

                let workspace_windows = windows_list.iter().filter_map(|wid| {
                    if let Some(window) = self.get_window_for_surface(wid) {
                        if !window.is_minimised() {
                            let bbox = space.element_geometry(window).unwrap().to_f64();
                            let bbox = bbox.to_physical(scale);
                            let rect = LayoutRect::new(
                                bbox.loc.x as f32,
                                bbox.loc.y as f32,
                                bbox.size.w as f32,
                                bbox.size.h as f32,
                            );
                            return Some((window, rect));
                        }
                    }
                    None
                });

                if init_layout {
                    natural_layout(&mut bin, workspace_windows, &layout_rect, false);
                }
                let mut state = WindowSelectorState {
                    rects: vec![],
                    current_selection: None,
                };
                let mut index = 0;
                let windows_list = workspace.windows_list.read().unwrap();
                for window_id in windows_list.iter() {
                    let window = self.get_window_for_surface(window_id).unwrap();
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
                            let scale = scale_x.min(scale_y).min(1.0);

                            let window_rect = WindowSelection {
                                x: rect.x,
                                y: rect.y + offset_y,
                                w: (window_width * scale),
                                h: (window_height * scale),
                                visible: true,
                                window_title: window.xdg_title().to_string(),
                                index,
                                window_id: Some(window_id.clone()),
                            };
                            index += 1;
                            state.rects.push(window_rect);
                            let scale = 1.0.interpolate(&scale, delta);
                            let delta = delta.clamp(0.0, 1.0);
                            let window_x = bbox.loc.x as f32;
                            let window_y = bbox.loc.y as f32;
                            let x = window_x.interpolate(&to_x, delta);
                            let y = window_y.interpolate(&to_y, delta);

                            if let Some(layer) = window_selector.layer_for_window(window_id) {
                                let translation =
                                    layer.change_position(lay_rs::types::Point { x, y });
                                let scale =
                                    layer.change_scale(lay_rs::types::Point { x: scale, y: scale });
                                changes.push(translation);
                                changes.push(scale);
                            }
                        }
                    }
                }

                workspace.window_selector_view.view.update_state(&state);
            }
            let current_workspace = model.workspaces.get(model.current_workspace).unwrap();

            current_workspace.clone()
        });

        let _transactions = self.layers_engine.schedule_changes(&changes, animation);

        let mut delta = delta.max(0.0);
        delta = delta.powf(0.65);

        let workspace_selector_y = (-400.0).interpolate(&0.0, delta);
        // let workspace_selector_y = -400.0;
        let workspace_opacity = 0.0.interpolate(&1.0, delta);
        let expose_layer = self.expose_layer.clone();
        let show_all_ref = self.show_all.clone();
        // disable pointer interactions during the animation
        self.set_show_all(false);
        self.workspace_selector_view
            .layer
            .set_position(
                lay_rs::types::Point {
                    x: 0.0,
                    y: workspace_selector_y,
                },
                transition,
            )
            .on_finish(
                move |_: &Layer, _: f32| {
                    expose_layer.set_hidden(!show_all);
                    show_all_ref.store(show_all, std::sync::atomic::Ordering::Relaxed);
                },
                true,
            );

        self.workspace_selector_view
            .layer
            .set_opacity(workspace_opacity, transition);

        let mut start_position = 0.0;
        let mut end_position = 250.0;
        if current_workspace.get_fullscreen_mode() {
            start_position = 250.0;
            end_position = 250.0;
        }
        let dock_y = (start_position).interpolate(&end_position, delta);
        let tr = self.dock.view_layer.set_position((0.0, dock_y), transition);

        if let Some(a) = animation {
            self.layers_engine.start_animation(a, 0.0);
        }
        if end_gesture {
            *bin = HashMap::new();
            let dock_ref = self.dock.clone();
            tr.on_finish(
                move |_: &Layer, _: f32| {
                    if show_all {
                        dock_ref.hide(None);
                    } else {
                        dock_ref.show(None);
                    }
                },
                true,
            );
        }
    }

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
                self.layers_engine
                    .add_layer_to_positioned(view.window_layer.clone(), Some(drawer.id));
                // bounds are calculate after this call
                let drawer_bounds = drawer.render_bounds_transformed();
                view.minimize(skia::Rect::from_xywh(
                    drawer_bounds.x(),
                    drawer_bounds.y(),
                    130.0,
                    130.0,
                ));

                let view_ref = view.clone();
                drawer.on_change_size(
                    move |layer: &Layer, _| {
                        let bounds = layer.render_bounds_transformed();
                        view_ref.genie_effect.set_destination(bounds);
                        view_ref.genie_effect.apply();
                    },
                    false,
                );
            }

            self.notify_observers(model);
        });
        we.set_activate(false);

        // ideally we set the focus to the next window in the stack
        let windows = self.spaces_elements();

        let win_len = windows.count();
        if win_len <= 1 {
            return;
        }
        let index = self.with_model(|m| m.current_workspace);
        let windows: Vec<_> = self.spaces[index].elements().map(|e| e.id()).collect();

        for (i, wid) in windows.iter().enumerate() {
            let activate = i == win_len - 2;
            // if !wid.is_minimized {
            self.raise_element(wid, activate, false);
            // }
        }
    }

    /// Unminimise a WindowElement
    pub fn unminimize_window(&mut self, wid: &ObjectId) {
        let event = self.with_model_mut(|model| {
            model.minimized_windows.retain(|(w, _title)| w != wid);
            model.clone()
        });
        let scale = Config::with(|c| c.screen_scale) as f32;
        if let Some((index, space)) = self
            .spaces
            .iter()
            .enumerate()
            .find(|(_, space)| space.elements().any(|e| e.id() == *wid))
        {
            let workspace = event.workspaces[index].clone();
            let window = self.get_window_for_surface(wid).unwrap();
            let window_geometry = space.element_geometry(window).unwrap();
            let pos_x = window_geometry.loc.x;
            let pos_y = window_geometry.loc.y;
            let layer_pos_x = pos_x as f32 * scale;
            let layer_pos_y = pos_y as f32 * scale;
            if let Some(window) = self.windows_map.get_mut(wid) {
                window.set_is_minimised(false);
            }
            if let Some(view) = self.get_window_view(wid) {
                if let Some(drawer) = self.dock.remove_window_element(wid) {
                    let engine_ref = self.layers_engine.clone();

                    let windows_layer_ref = workspace.windows_layer.clone();
                    let layer_ref = view.window_layer.clone();
                    self.layers_engine.update(0.0);

                    let drawer_bounds = drawer.render_bounds_transformed();

                    // close dock drawer animation
                    // on start animation move the window to the workspace

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
                                engine_ref.add_layer_to_positioned(
                                    layer_ref.clone(),
                                    Some(windows_layer_ref.id),
                                );
                                layer_ref.set_position(
                                    (layer_pos_x, layer_pos_y),
                                    Transition::ease_out(0.3),
                                );
                            },
                            true,
                        )
                        .then(move |layer: &Layer, _| {
                            layer.remove();
                        });

                    view.unminimize(drawer_bounds);

                    if let Some(window) = self.get_window_for_surface(wid).cloned() {
                        window.set_activate(true);
                        //     window.set_activate(true);
                        //     self.map_element(window.clone(), (pos_x as i32, pos_y as i32), true);
                        // self.map_element(window.clone(), (pos_x as i32, pos_y as i32), true);
                    }
                }
            }

            // let event = model.clone();
            self.notify_observers(&event);
        }
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

        if let std::collections::hash_map::Entry::Vacant(e) = self.windows_map.entry(window_element.id()) {
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
    }

    /// remove a WindowElement from the workspace model,
    /// remove the window layer from the scene,
    pub fn unmap_window(&mut self, window_id: &ObjectId) {
        tracing::info!("workspaces::unmap_window: {:?}", window_id);

        if let Some(element) = self.get_window_for_surface(window_id).cloned() {
            for space in self.spaces.iter_mut() {
                space.unmap_elem(&element);
            }
        }

        self.with_model(|m| {
            for workspace_view in m.workspaces.iter() {
                workspace_view.unmap_window(window_id);
            }
        });
        self.windows_map.remove(window_id);
        self.remove_window_view(window_id);

        self.refresh_space();
        self.update_workspace_model();
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

        // unmap from old space
        if let Some((index, space)) = self
            .spaces
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.elements().any(|e| e.id() == we.id()))
        {
            space.unmap_elem(we);
            let id = we.id();
            let model = self.model.read().unwrap();
            if let Some(workspace) = model.workspaces.get(index) {
                workspace.unmap_window(&id);
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
                        workspace
                            .windows_layer
                            .add_sublayer(&view.window_layer);
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
        tracing::info!("workspaces::update_workspace_model");

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
            ));
            self.expose_layer
                .add_sublayer(&workspace.window_selector_view.layer);

            m.workspaces.push(workspace.clone());
            self.notify_observers(m);
            (m.workspaces.len() - 1, workspace)
        });
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
                self.move_window_to_workspace(e, workspace_model.current_workspace, location);
            }
        }
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
    pub fn set_current_workspace_index(&mut self, i: usize, transition: Option<Transition>) {
        if i > self.spaces.len() - 1 {
            return;
        }
        self.with_model_mut(|m| {
            if i > m.workspaces.len() - 1 {
                return;
            }
            m.current_workspace = i;
        });
        self.update_workspace_model();
        self.scroll_to_workspace_index(i, transition);
    }
    /// Scroll to the workspace at index i, default transition is 1.0s spring
    fn scroll_to_workspace_index(&self, i: usize, transition: Option<Transition>) {
        let transition = transition.unwrap_or(Transition {
            delay: 0.0,
            timing: TimingFunction::Spring(Spring::with_duration_and_bounce(1.0, 0.1)),
        });

        if let Some(workspace) = self.get_workspace_at(i) {
            if workspace.get_fullscreen_mode() || self.get_show_all() {
                self.dock.hide(Some(transition));
            } else {
                self.dock.show(Some(transition));
            }
        }

        let width = self.workspaces_layer.render_size().x;
        self.workspaces_layer
            .set_position((-(width + 100.0) * i as f32, 0.0), Some(transition));
        self.expose_layer
            .set_position((-(width + 100.0) * i as f32, 0.0), Some(transition));
    }

    // Space management

    pub fn outputs_for_element(&self, element: &WindowElement) -> Vec<Output> {
        self.space().outputs_for_element(element)
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
