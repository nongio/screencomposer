use std::{
    collections::{HashMap, VecDeque},
    fmt,
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicBool, AtomicI32},
        Arc, RwLock, Weak,
    },
};

use freedesktop_desktop_entry::{default_paths, DesktopEntry, Iter as DesktopEntryIter};
use lay_rs::{
    engine::LayersEngine,
    prelude::{taffy, Interpolate, Layer, Spring, TimingFunction, Transition},
    skia::{self, Contains},
    types::Size,
};
use smithay::{
    desktop::{layer_map_for_output, space::SpaceElement, Space, WindowSurface},
    output::Output,
    reexports::wayland_server::{backend::ObjectId, Resource},
    utils::{IsAlive, Rectangle},
    wayland::shell::xdg::XdgToplevelSurfaceData,
};

use workspace::WorkspaceView;

mod app_switcher;
mod background;
mod dnd_view;
mod dock;
pub mod workspace;

pub mod utils;

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
        acquire_write_lock_with_retry, image_from_path,
        natural_layout::{natural_layout, LayoutRect},
        notify_observers, Observable, Observer,
    },
};

#[derive(Clone, Default)]
pub struct Application {
    pub identifier: String,
    pub desktop_name: Option<String>,
    pub icon_path: Option<String>,
    pub icon: Option<skia::Image>,
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
        if let Some(i) = self.icon.as_ref() {
            i.unique_id().hash(state)
        }
    }
}

impl PartialEq for Application {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}
impl Eq for Application {}

impl Application {
    pub fn new(app_id: &str) -> Self {
        Self {
            identifier: app_id.to_string(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct WorkspacesModel {
    workspace_counter: usize,
    pub workspaces: Vec<Arc<WorkspaceView>>,
    pub current_workspace: usize,

    pub applications_cache: HashMap<String, Application>,

    pub app_windows_map: HashMap<String, Vec<ObjectId>>,
    pub zindex_application_list: Vec<String>,
    pub application_list: VecDeque<String>,

    pub windows_list: Vec<ObjectId>,
    pub minimized_windows: Vec<ObjectId>,
    pub current_application: usize,

    pub width: i32,
    pub height: i32,
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

    // gestures states
    pub show_all: Arc<AtomicBool>,
    pub show_desktop: Arc<AtomicBool>,
    pub expose_bin: Arc<RwLock<HashMap<ObjectId, LayoutRect>>>,
    pub show_all_gesture: Arc<AtomicI32>,
    pub show_desktop_gesture: Arc<AtomicI32>,

    // layers
    pub layers_engine: LayersEngine,
    pub overlay_layer: Layer,
    pub workspaces_layer: Layer,
    pub expose_layer: Layer,

    observers: Vec<Weak<dyn Observer<WorkspacesModel>>>,
}

/// # Workspace Layer Structure
///
/// ```
/// Workspaces
/// └── root
///     ├── workspaces
///         ├── workspace_view
///         ├── workspace_view
///     ├── expose
///         ├── expose_view
///         ├── expose_view
///     ├── app_switcher
///     └── dock
/// ```
///
///
///
impl Workspaces {
    pub fn new(layers_engine: LayersEngine) -> Self {
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

        layers_engine.scene_add_layer(workspaces_layer.clone());

        let expose_layer = layers_engine.new_layer();
        expose_layer.set_key("expose");
        expose_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            flex_wrap: taffy::FlexWrap::NoWrap,
            gap: taffy::Size::length(100.0),
            ..Default::default()
        });
        expose_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        expose_layer.set_pointer_events(false);
        expose_layer.set_hidden(true);

        layers_engine.scene_add_layer(expose_layer.clone());

        let overlay_layer = layers_engine.new_layer();
        overlay_layer.set_key("overlay_view");
        overlay_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        overlay_layer.set_pointer_events(false);

        let dock = DockView::new(layers_engine.clone());
        let dock = Arc::new(dock);
        dock.view_layer.set_position((0.0, -20.0), None);

        let app_switcher = AppSwitcherView::new(layers_engine.clone());
        let app_switcher = Arc::new(app_switcher);

        let workspace_selector_layer = layers_engine.new_layer();
        workspace_selector_layer.set_pointer_events(false);
        layers_engine.scene_add_layer(workspace_selector_layer.clone());

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

    fn space_mut(&mut self) -> &mut Space<WindowElement> {
        let index = self.with_model(|m| m.current_workspace);

        &mut self.spaces[index]
    }

    pub fn set_screen_dimension(&self, width: i32, height: i32) {
        self.with_model_mut(|model| {
            model.width = width;
            model.height = height;
            let event = model.clone();
            self.notify_observers(&event);
        });
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

            self.set_show_all(show_all);
        }
        if show_all {
            self.expose_layer.set_hidden(false);
        }

        let delta = new_gesture as f32 / 1000.0;

        let mut transition = Some(Transition {
            delay: 0.0,
            timing: TimingFunction::Spring(Spring::new_with_velocity(1.0, 170.0, 30.0, 0.0)),
        });
        if !end_gesture {
            // in the middle of the gesture
            transition = None;
        }
        let animation = transition.map(|t| self.layers_engine.new_animation(t, false));

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
        self.with_model(|model| {
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

                for window_id in model.windows_list.iter() {
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
                                window_title: window.title().to_string(),
                                index,
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
        });

        let _transactions = self.layers_engine.add_animated_changes(&changes, animation);

        let mut delta = delta.max(0.0);
        delta = delta.powf(0.65);

        let workspace_selector_y = (-200.0).interpolate(&0.0, delta);
        let workspace_opacity = 0.0.interpolate(&1.0, delta);
        let expose_layer = self.expose_layer.clone();
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
                },
                true,
            );

        self.workspace_selector_view
            .layer
            .set_opacity(workspace_opacity, transition);

        let dock_y = (-20.0).interpolate(&250.0, delta);
        self.dock.view_layer.set_position((0.0, dock_y), transition);

        if let Some(a) = animation {
            self.layers_engine.start_animation(a, 0.0);
        }
        if end_gesture {
            *bin = HashMap::new();
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

        for window in model.windows_list.iter() {
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
        let current_app = self.get_current_app();
        if let Some(app) = current_app {
            self.quit_app(&app.identifier);
        }
    }

    /// Close all the windows of the current focused App in th app switcher
    pub fn quit_appswitcher_app(&self) {
        let appswitcher_app = self.app_switcher.get_current_app();

        if let Some(app) = appswitcher_app {
            self.quit_app(&app.identifier);
        }
    }

    /// Minimise a WindowElement
    pub fn minimize_window(&mut self, we: &WindowElement) {
        let id = we.wl_surface().unwrap().id();

        if let Some(window) = self.windows_map.get_mut(&id) {
            window.set_is_minimised(true);
        }

        self.with_model_mut(|model| {
            // self.windows_cache.insert(id.clone(), window.clone());
            model.minimized_windows.push(id.clone());

            if let Some(view) = self.get_window_view(&id) {
                let (drawer, _) = self.dock.add_window_element(we);

                view.window_layer.set_layout_style(taffy::Style {
                    position: taffy::Position::Absolute,
                    ..Default::default()
                });
                self.layers_engine
                    .scene_add_layer_to_positioned(view.window_layer.clone(), drawer.clone());
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
        // let we = self.get_window_for_surface(id).unwrap();
        // if let Some(view) = self.get_window_view(&wid) {
        let event = self.with_model_mut(|model| {
            model.minimized_windows.retain(|w| w != wid);
            model.clone()
        });
        let workspace = self.get_current_workspace();
        let mut pos_x = 0.0;
        let mut pos_y = 0.0;
        if let Some(window) = self.windows_map.get_mut(wid) {
            window.set_is_minimised(false);
            let bbox = window.bbox();
            pos_x = bbox.loc.x as f32;
            pos_y = bbox.loc.y as f32;
        }
        if let Some(view) = self.get_window_view(wid) {
            if let Some(drawer) = self.dock.remove_window_element(wid) {
                let engine_ref = self.layers_engine.clone();

                let windows_layer_ref = workspace.windows_layer.clone();
                let layer_ref = view.window_layer.clone();
                self.layers_engine.update(0.0);

                let drawer_bounds = drawer.render_bounds_transformed();
                // let bbox = view.window.bbox();
                // let pos_x = bbox.loc.x as f32;
                // let pos_y = bbox.loc.y as f32;
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
                            engine_ref.scene_add_layer_to_positioned(
                                layer_ref.clone(),
                                windows_layer_ref.clone(),
                            );
                            layer_ref.set_position((pos_x, pos_y), Transition::ease_out(0.3));
                        },
                        true,
                    )
                    .then(move |layer: &Layer, _| {
                        layer.remove();
                    });

                view.unminimize(drawer_bounds);
            }
        }

        // let event = model.clone();
        self.notify_observers(&event);

        // let pos_x = view.unmaximized_rect.x as i32;
        // let pos_y = view.unmaximized_rect.y as i32;

        // self.map_element(we.clone(), (pos_x, pos_y), true);
        // we.set_activate(true);
        // FIXME seat
        // self.seat.get_keyboard().unwrap().set_focus(
        //     self,
        //     Some(we.clone().into()),
        //     SERIAL_COUNTER.next_serial(),
        // );
        // }
    }

    // Helpers / Windows Management

    /// Determine the initial placement of a new window within the workspace.
    /// It calculates the appropriate position and size for the window based
    /// on the current pointer location and the available output geometry
    pub fn place_new_window(
        &mut self,
        window: WindowElement,
        pointer_location: smithay::utils::Point<f64, smithay::utils::Logical>,
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

        // set the initial toplevel bounds
        #[allow(irrefutable_let_patterns)]
        if let WindowSurface::Wayland(window) = window.underlying_surface() {
            window.with_pending_state(|state| {
                state.bounds = Some(output_geometry.size);
            });
        }

        let num_open_windows = self.spaces_elements().count();
        let window_index = num_open_windows + 1; // Index of the new window

        let max_x = output_geometry.loc.x + output_geometry.size.w;
        let max_y = output_geometry.loc.y + output_geometry.size.h;

        // Calculate the position along the diagonal
        const MAX_WINDOW_COUNT: f32 = 40.0;
        let factor = window_index as f32 / MAX_WINDOW_COUNT;
        let x = (output_geometry.loc.x as f32 + factor * max_x as f32) as i32 + 100;
        let y = (output_geometry.loc.y as f32 + factor * max_y as f32) as i32 + 100;

        println!("Placing window at ({}, {})", x, y);

        self.map_element(window, (x, y), true);
    }

    pub fn unmap_window(&mut self, window_id: &ObjectId) {
        {
            if let Some(view) = self.get_window_view(window_id) {
                let noderef = view.window_layer.id().unwrap();
                let scene_layer = self.layers_engine.scene_get_node(&noderef).unwrap();
                let scene_layer = scene_layer.get().clone();
                scene_layer.mark_for_deletion();
                self.remove_window_view(window_id);
            }
        }
        self.with_model(|m| {
            for workspace in m.workspaces.iter() {
                workspace.unmap_window(window_id);
            }
        });

        self.windows_map.remove(window_id);
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
    pub fn get_current_app(&self) -> Option<Application> {
        let model = self.model.read().unwrap();
        let app_id = model.zindex_application_list[model.current_application].clone();
        model.applications_cache.get(&app_id).cloned()
    }

    /// Return the list of WlSurface ids of the current focused Application
    pub fn get_current_app_windows(&self) -> Vec<ObjectId> {
        self.get_current_app()
            .map(|app| self.get_app_windows(&app.identifier))
            .unwrap_or_default()
    }

    /// Return the Window object of WlSurface by its id
    pub fn get_window_for_surface(&self, id: &ObjectId) -> Option<&WindowElement> {
        // let model = self.model.read().unwrap();
        // model.windows_cache.get(id).cloned()
        self.windows_map.get(id)
    }

    pub fn get_or_add_window_view(
        &self,
        // object_id: &ObjectId,
        window: &WindowElement,
    ) -> WindowView {
        let mut window_views = self.window_views.write().unwrap();
        let wid = window.id();
        let insert = window_views
            .entry(wid.clone())
            .or_insert_with(|| WindowView::new(self.layers_engine.clone(), window));
        insert.clone()
    }

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

    pub fn move_window_to_workspace(
        &mut self,
        we: &WindowElement,
        workspace_index: usize,
        location: smithay::utils::Point<i32, smithay::utils::Logical>,
    ) {
        if let Some(space) = self.spaces.get_mut(workspace_index) {
            println!("move_window_to_workspace: {:?}", workspace_index);
            space.map_element(we.clone(), location, false);
            let id = we.wl_surface().unwrap().id();
            let model = self.model.read().unwrap();
            if let Some(workspace) = model.workspaces.get(workspace_index) {
                if let Some(window) = self.windows_map.get(&id) {
                    workspace.map_window(window);
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

    pub fn focus_app(&mut self, app_id: &str) {
        self.raise_app_elements(app_id);
        // scroll to the first workspace where the app is
        let current_space = self.space();
        let spaces = self.get_app_spaces(app_id);
        if spaces.iter().any(|s| *s == current_space) {
        } else {
            let first_space = spaces.first().unwrap();
            let index = self
                .spaces
                .iter()
                .position(|s| s == *first_space)
                .unwrap_or(0);
            self.set_current_workspace_index(index);
        }
    }

    pub fn raise_window_app_elements(&mut self, we: &WindowElement, activate: bool) {
        let windows = self.get_app_windows(we.app_id());
        for window_id in windows.iter() {
            self.raise_element(window_id, true, false);
        }
        if !we.is_minimised() {
            self.raise_element(&we.id(), activate, true);
        }
    }

    pub fn raise_element(&mut self, window_id: &ObjectId, activate: bool, update: bool) {
        // get the space with the window
        if let Some((index, space)) = self
            .spaces
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.elements().any(|e| e.id() == *window_id))
        {
            if let Some(window) = self.windows_map.get(window_id) {
                if window.is_minimised() && !activate {
                    return;
                }
                space.raise_element(window, activate);
                let workspace = self.with_model(|m| m.workspaces[index].clone());
                {
                    if let Some(view) = self.get_window_view(window_id) {
                        workspace
                            .windows_layer
                            .add_sublayer(view.window_layer.clone());
                    }
                    if let Some(layer) = workspace.window_selector_view.layer_for_window(window_id)
                    {
                        workspace
                            .window_selector_view
                            .layer
                            .add_sublayer(layer.clone());
                    }
                }
                if update {
                    self.update_workspace_model();
                }
            }
        }
    }

    pub fn raise_app_elements(&mut self, app_id: &str) -> Option<ObjectId> {
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
                    self.update_workspace_model();
                } else {
                    // if minimised and there is only one window in the app, unminimize it
                    if windows.len() == 1 {
                        self.unminimize_window(window_id);
                    }
                }
            }
        }
        focus_wid
    }

    fn load_async_app_info(&self, app_id: &str) {
        tracing::info!("load_async_app_info: {}", app_id);
        let app_id = app_id.to_string();
        let model = self.model.clone();
        let observers = self.observers.clone();
        // let ctx = None;//self.direct_context.clone();
        tokio::spawn(async move {
            let mut desktop_entry: Option<DesktopEntry<'_>> = None;
            let bytes;
            let path;
            let default_paths = default_paths();
            let path_result = DesktopEntryIter::new(default_paths)
                .find(|path| path.to_string_lossy().contains(&app_id));

            if let Some(p) = path_result {
                path = p.clone();
                let bytes_result = std::fs::read_to_string(&p);
                if let Ok(b) = bytes_result {
                    bytes = b.clone();
                    if let Ok(entry) = DesktopEntry::decode(&path, &bytes) {
                        desktop_entry = Some(entry);
                    }
                }
            }
            if let Some(desktop_entry) = desktop_entry {
                if let Some(mut model_mut) = acquire_write_lock_with_retry(&model) {
                    let icon_path = desktop_entry
                        .icon()
                        .map(|icon| icon.to_string())
                        .and_then(|icon_name| xdgkit::icon_finder::find_icon(icon_name, 512, 1))
                        .map(|icon| icon.to_str().unwrap().to_string());
                    let icon = icon_path
                        .as_ref()
                        .and_then(|icon_path| image_from_path(icon_path, None));

                    let mut app = model_mut
                        .applications_cache
                        .get(&app_id)
                        .unwrap_or(&Application {
                            identifier: app_id.to_string(),
                            ..Default::default()
                        })
                        .clone();
                    if app.icon_path != icon_path {
                        app.desktop_name = desktop_entry.name(None).map(|name| name.to_string());
                        app.icon_path = icon_path;
                        app.icon = icon.clone();
                        tracing::info!("loaded: {:?}", app);
                        model_mut.applications_cache.insert(app_id, app);
                        notify_observers(&observers, &model_mut.clone());
                    }
                }
            }
        });
    }

    // updates the workspace model using elemenets from Space
    pub(crate) fn update_workspace_model(&self) {
        // FIXME disaster
        let windows: Vec<_> = self
            .spaces_elements()
            .map(|we| (we.wl_surface().unwrap().id(), we.clone()))
            .collect();

        let window_views: Vec<_> = windows
            .iter()
            .filter_map(|(id, we)| {
                if let Some(wv) = self.get_window_view(id) {
                    let state = wv.view_base.get_state();
                    Some((we.clone(), wv.window_layer.clone(), state))
                } else {
                    None
                }
            })
            .collect();
        {
            if let Ok(mut model_mut) = self.model.write() {
                model_mut.zindex_application_list = Vec::new();
                model_mut.windows_list = Vec::new();
                model_mut.app_windows_map = HashMap::new();
            } else {
                return;
            }
        }
        let mut windows_peek = window_views
            .iter()
            .filter(|(w, _l, _state)| w.wl_surface().is_some()) // do we need this?
            .peekable();

        #[allow(clippy::while_let_on_iterator)]
        while let Some((w, _, _)) = windows_peek.next() {
            let surface = w.wl_surface().map(|s| (s.as_ref()).clone()).unwrap();
            smithay::wayland::compositor::with_states(&surface, |states| {
                let attributes: std::sync::MutexGuard<
                    '_,
                    smithay::wayland::shell::xdg::XdgToplevelSurfaceRoleAttributes,
                > = states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap();

                if let Some(app_id) = attributes.app_id.as_ref() {
                    let id = w.wl_surface().unwrap().id();

                    let app_index = {
                        let mut model = self.model.write().unwrap();
                        // don't allow duplicates in app switcher
                        // TODO use config
                        let app_index = model
                            .zindex_application_list
                            .iter()
                            .position(|id| id == app_id)
                            .unwrap_or_else(|| {
                                model.zindex_application_list.push(app_id.clone());
                                model.zindex_application_list.len() - 1
                            });
                        if !model.application_list.contains(app_id) {
                            model.application_list.push_front(app_id.clone());
                        }

                        let app = model
                            .applications_cache
                            .entry(app_id.to_owned())
                            .or_insert(Application {
                                identifier: app_id.to_string(),
                                ..Default::default()
                            })
                            .clone();

                        let windows_for_app =
                            model.app_windows_map.entry(app_id.clone()).or_default();

                        windows_for_app.push(id.clone());
                        drop(model);
                        {
                            if app.icon.is_none() {
                                self.load_async_app_info(app_id);
                            }
                        }
                        app_index
                    };

                    {
                        let mut model_mut: std::sync::RwLockWriteGuard<'_, WorkspacesModel> =
                            self.model.write().unwrap();
                        model_mut.windows_list.push(id.clone());

                        if windows_peek.peek().is_none() {
                            model_mut.current_application = app_index;
                        }
                    }
                }
            });
        }
        // keep only app in application_list that are in zindex_application_list
        {
            let mut model = self.model.write().unwrap();
            let app_list = model.zindex_application_list.clone();
            {
                // update app list
                model
                    .application_list
                    .retain(|app_id| app_list.contains(app_id));
            }
            {
                // update minimized windows
                let windows_list = model.windows_list.clone();
                model
                    .minimized_windows
                    .retain(|id| windows_list.contains(id));
            }
        }

        let model = self.model.read().unwrap();
        let event = model.clone();

        self.notify_observers(&event);
    }

    pub fn spaces_elements(&self) -> impl DoubleEndedIterator<Item = &WindowElement> {
        self.spaces.iter().flat_map(|space| space.elements())
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

    pub fn add_workspace(&mut self) {
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
        self.with_model_mut(|m| {
            m.workspace_counter += 1;

            let workspace = Arc::new(WorkspaceView::new(
                m.workspace_counter,
                self.layers_engine.clone(),
                &self.workspaces_layer,
            ));
            self.expose_layer
                .add_sublayer(workspace.window_selector_view.layer.clone());
            m.workspaces.push(workspace);
            self.notify_observers(m);
        });
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
        self.scroll_to_workspace_index(workspace_model.current_workspace);
        self.notify_observers(&workspace_model);
    }

    pub fn get_current_workspace(&self) -> Arc<WorkspaceView> {
        self.with_model(|m| m.workspaces[m.current_workspace].clone())
    }

    pub fn set_current_workspace_index(&mut self, i: usize) {
        if i > self.spaces.len() - 1 {
            return;
        }
        println!("bench {}", i);
        self.with_model_mut(|m| {
            if i > m.workspaces.len() - 1 {
                return;
            }
            m.current_workspace = i;
            self.scroll_to_workspace_index(i);

            self.notify_observers(m);
        });
    }

    fn scroll_to_workspace_index(&self, i: usize) {
        let width = self.workspaces_layer.render_size().x;
        self.workspaces_layer.set_position(
            (-(width + 100.0) * i as f32, 0.0),
            Some(Transition {
                delay: 0.0,
                timing: TimingFunction::Spring(Spring::new_with_velocity(1.0, 60.0, 15.0, 0.0)),
            }),
        );
        self.expose_layer.set_position(
            (-(width + 100.0) * i as f32, 0.0),
            Some(Transition {
                delay: 0.0,
                timing: TimingFunction::Spring(Spring::new_with_velocity(1.0, 60.0, 15.0, 0.0)),
            }),
        );
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

    // map the window position, called on every window move / drag
    pub fn map_element(
        &mut self,
        element: WindowElement,
        location: impl Into<smithay::utils::Point<i32, smithay::utils::Logical>>,
        activate: bool,
    ) {
        self.space_mut()
            .map_element(element.clone(), location, activate);
        self.space_mut().refresh();

        // FIXME not the right place
        self.windows_map.insert(element.id(), element.clone());

        // append the window to the workspace layer
        {
            let outputs = self.outputs_for_element(&element);
            let output = outputs.first().or_else(|| self.outputs().next());
            let scale = output
                .map(|o| o.current_scale().fractional_scale())
                .unwrap_or(1.0);
            let location = self
                .element_location(&element)
                .unwrap_or_default()
                .to_f64()
                .to_physical(scale);

            let workspace = self.get_current_workspace();
            // workspace.windows_layer.add_sublayer(element.base_layer().clone());
            // if let Some(window_layer_id) = workspace.windows_layer.id() {
            workspace.map_window(&element);
            let _view = self.get_or_add_window_view(&element);
            element.base_layer().set_position(
                lay_rs::types::Point {
                    x: location.x as f32,
                    y: location.y as f32,
                },
                None,
            );

            if let Some(l) = workspace
                .window_selector_view
                .layer_for_window(&element.id())
            {
                // println!("move clone window");
                l.set_position(
                    lay_rs::types::Point {
                        x: location.x as f32,
                        y: location.y as f32,
                    },
                    None,
                );
            }
            // }
        }
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
