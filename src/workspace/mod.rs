mod app_switcher;
mod background;
mod window_selector;
mod window_view;
mod workspace_selector;

use crate::{
    shell::WindowElement,
    utils::{
        image_from_path,
        natural_layout::{natural_layout, LayoutRect},
        Observable, Observer,
    },
};
use core::fmt;
use freedesktop_desktop_entry::{default_paths, DesktopEntry, Iter as DesktopEntryIter};
use layers::{
    engine::LayersEngine,
    prelude::{taffy, Easing, Interpolate, Layer, TimingFunction, Transition},
};
use smithay::{
    input::pointer::CursorImageStatus,
    reexports::wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    wayland::shell::xdg::XdgToplevelSurfaceData,
};
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicBool, AtomicI32},
        Arc, Mutex, RwLock, Weak,
    },
};
use workspace_selector::WorkspaceSelectorView;

pub use background::BackgroundView;
pub use window_selector::{WindowSelection, WindowSelectorState, WindowSelectorView};
pub use window_view::{WindowView, WindowViewBaseModel, WindowViewSurface};

pub use app_switcher::AppSwitcherView;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Window {
    pub wl_surface: Option<WlSurface>,
    pub window_element: Option<WindowElement>,
    title: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    is_fullscreen: bool,
    is_maximized: bool,
    is_minimized: bool,
    app_id: String,
    pub base_layer: Layer,
}
#[derive(Clone, Default)]
pub struct Application {
    pub identifier: String,
    pub desktop_name: Option<String>,
    pub icon_path: Option<String>,
    pub icon: Option<skia_safe::Image>,
    pub windows: Vec<Window>,
}

impl fmt::Debug for Application {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Application")
            .field("identifier", &self.identifier)
            .field("desktop_name", &self.desktop_name)
            .field("icon_path", &self.icon_path)
            // .field("icon", &self.icon)
            .field("windows", &self.windows)
            .finish()
    }
}

impl PartialEq for Application {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
    }
}
impl Eq for Application {}

impl Hash for Application {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identifier.hash(state);
        self.desktop_name.hash(state);

        if let Some(icon) = self.icon.as_ref() {
            icon.unique_id().hash(state);
        }
    }
}

pub struct Workspace {
    pub model: Arc<RwLock<WorkspaceModel>>,

    pub windows_map: Arc<RwLock<HashMap<ObjectId, Window>>>,
    pub app_switcher: Arc<AppSwitcherView>,
    pub window_selector_view: Arc<WindowSelectorView>,
    pub background_view: Arc<BackgroundView>,
    pub workspace_selector_view: WorkspaceSelectorView,
    pub layers_engine: LayersEngine,
    pub workspace_layer: Layer,
    pub windows_layer: Layer,
    pub overlay_layer: Layer,
    pub show_all: AtomicBool,
    pub show_desktop: AtomicBool,
    pub expose_bin: Arc<RwLock<HashMap<ObjectId, LayoutRect>>>,
    pub show_all_gesture: AtomicI32,
    pub show_desktop_gesture: AtomicI32,
}

#[derive(Default, Clone)]
pub struct WorkspaceModel {
    pub applications: HashMap<String, Application>,
    pub application_list: Vec<String>,
    pub windows: Vec<ObjectId>,
    pub current_application: usize,
    observers: Vec<Weak<dyn Observer<WorkspaceModel>>>,
}

impl fmt::Debug for Workspace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let model = self.model.read().unwrap();

        f.debug_struct("WorkspaceModel")
            .field("applications", &model.applications)
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
    pub fn new(
        layers_engine: LayersEngine,
        cursor_status: Arc<Mutex<CursorImageStatus>>,
    ) -> Arc<Self> {
        let workspace_layer = layers_engine.new_layer();
        workspace_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        workspace_layer.set_size(layers::types::Size::percent(1.0, 1.0), None);
        let background_layer = layers_engine.new_layer();
        background_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        background_layer.set_size(layers::types::Size::percent(1.0, 1.0), None);
        background_layer
            .set_background_color(layers::prelude::Color::new_rgba(0.0, 0.0, 0.0, 1.0), None);
        background_layer
            .set_border_corner_radius(layers::prelude::BorderRadius::new_single(20.0), None);
        // background_layer.set_opacity(0.0, None);
        let windows_layer = layers_engine.new_layer();
        windows_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        let overlay_layer = layers_engine.new_layer();
        overlay_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        let workspace_id = layers_engine.scene_add_layer(workspace_layer.clone());

        let workspace_selector_layer = layers_engine.new_layer();
        workspace_selector_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });

        layers_engine.scene_add_layer_to(background_layer.clone(), Some(workspace_id));
        layers_engine.scene_add_layer_to(windows_layer.clone(), Some(workspace_id));
        layers_engine.scene_add_layer_to(workspace_selector_layer.clone(), Some(workspace_id));
        layers_engine.scene_add_layer(overlay_layer.clone());

        let mut model = WorkspaceModel::default();

        let app_switcher = AppSwitcherView::new(layers_engine.clone());
        let app_switcher = Arc::new(app_switcher);

        model.add_listener(app_switcher.clone());
        let background_view = BackgroundView::new(layers_engine.clone(), background_layer.clone());
        if let Some(background_image) = image_from_path("./resources/background.jpg") {
            background_view.set_image(background_image);
        }
        let background_view = Arc::new(background_view);

        let window_selector_view =
            WindowSelectorView::new(layers_engine.clone(), cursor_status.clone());
        let window_selector_view = Arc::new(window_selector_view);

        let workspace_selector_view =
            WorkspaceSelectorView::new(layers_engine.clone(), workspace_selector_layer.clone());

        Arc::new(Self {
            model: Arc::new(RwLock::new(model)),

            windows_map: Arc::new(RwLock::new(HashMap::new())),
            app_switcher,
            window_selector_view: window_selector_view.clone(),
            background_view,
            workspace_selector_view,
            layers_engine,
            windows_layer,
            overlay_layer,
            workspace_layer,
            show_all: AtomicBool::new(false),
            show_desktop: AtomicBool::new(false),
            expose_bin: Arc::new(RwLock::new(HashMap::new())),
            show_all_gesture: AtomicI32::new(0),
            show_desktop_gesture: AtomicI32::new(0),
        })
    }

    pub fn get_show_all(&self) -> bool {
        self.show_all.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn set_show_all(&self, show_all: bool) {
        self.show_all
            .store(show_all, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_show_desktop(&self) -> bool {
        self.show_desktop.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn set_show_desktop(&self, show_all: bool) {
        self.show_desktop
            .store(show_all, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn update_window(&self, id: &ObjectId, model: &WindowViewBaseModel) {
        let mut map = self.windows_map.write().unwrap();
        if let Some(window) = map.get_mut(id) {
            window.x = model.x;
            window.y = model.y;
            window.w = model.w;
            window.h = model.h;
            window.title = model.title.clone();
        }
    }
    pub(crate) fn update_with_window_elements<I>(&self, windows: I)
    where
        I: Iterator<Item = (WindowElement, layers::prelude::Layer, WindowViewBaseModel)>,
    {
        {
            let mut model_mut = self.model.write().unwrap();
            model_mut.application_list = Vec::new();
            model_mut.windows = Vec::new();
            model_mut.applications = HashMap::new();
            let mut map = self.windows_map.write().unwrap();
            map.clear();
        }

        windows
            .filter(|(w, l, state)| w.wl_surface().is_some()) // do we need this?
            .for_each(|(w, l, state)| {
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
                        let wl_surface = w.wl_surface().map(|s| (s.as_ref()).clone());
                        let window = Window {
                            app_id: app_id.to_string(),
                            wl_surface,
                            window_element: Some(w),
                            base_layer: l,
                            x: state.x,
                            y: state.y,
                            w: state.w,
                            h: state.h,
                            title: state.title.clone(),
                            is_fullscreen: false,
                            is_maximized: false,
                            is_minimized: false,
                        };
                        {
                            let mut model = self.model.write().unwrap();
                            // don't allow duplicates in app switcher
                            // TODO use config
                            if !model.application_list.iter().any(|id| id == app_id) {
                                model.application_list.push(app_id.clone());
                            }
                            if !model.applications.contains_key(app_id) {
                                model.applications.insert(
                                    app_id.to_owned(),
                                    Application {
                                        identifier: app_id.to_string(),
                                        ..Default::default()
                                    },
                                );
                            }
                            let app = model.applications.get_mut(app_id).unwrap();
                            app.windows.push(window.clone());
                            {
                                drop(model);
                                self.load_async_app_info(app_id);
                            }
                        }

                        {
                            let mut map = self.windows_map.write().unwrap();
                            map.insert(id.clone(), window);
                            let mut model_mut = self.model.write().unwrap();
                            model_mut.windows.push(id);
                        }
                    }
                });
            });
        let model = self.model.read().unwrap();
        let event = model.clone();
        // println!("{:?}", apps.application_list);

        model.notify_observers(&event);
    }

    fn load_async_app_info(&self, app_id: &str) {
        let app_id = app_id.to_string();
        let model = self.model.clone();
        // let instance = self.clone();
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
                let mut model_mut = model.write().unwrap();

                let icon_path = desktop_entry
                    .icon()
                    .map(|icon| icon.to_string())
                    .and_then(|icon_name| xdgkit::icon_finder::find_icon(icon_name, 512, 1))
                    .map(|icon| icon.to_str().unwrap().to_string());
                let icon = icon_path
                    .as_ref()
                    .and_then(|icon_path| image_from_path(icon_path));
                if let Some(state) = model_mut.applications.get_mut(&app_id) {
                    state.desktop_name = desktop_entry.name(None).map(|name| name.to_string());
                    state.icon_path = icon_path;
                    state.icon = icon.clone();
                } else {
                    let state = Application {
                        identifier: app_id.to_string(),
                        desktop_name: desktop_entry.name(None).map(|name| name.to_string()),
                        icon_path,
                        icon: icon.clone(),
                        ..Default::default()
                    };
                    model_mut.applications.insert(app_id, state);
                }

                model_mut.notify_observers(&model_mut.clone());
            }
        });
    }
    // pub fn get_window_view(&self, id: &ObjectId) -> Option<WindowView> {
    //     let window_views = self.window_views.read().unwrap();
    //     window_views.get(&id).cloned()
    // }
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

        let delta = new_gesture as f32 / 1000.0;
        self.show_all_gesture
            .store(new_gesture, std::sync::atomic::Ordering::Relaxed);

        let workspace_selector_height = 250.0;
        let padding_top = 10.0;
        let padding_bottom = 10.0;

        let size = self.workspace_layer.render_size();
        let screen_size_w = size.x;
        let screen_size_h = size.y - padding_top - padding_bottom - workspace_selector_height;
        let model = self.model.read().unwrap();
        let map = self.windows_map.read().unwrap();
        let windows = model
            .windows
            .iter()
            .map(|w| {
                let w = map.get(w).unwrap();
                w.clone()
            })
            .collect();

        let mut bin = self.expose_bin.write().unwrap();
        if bin.is_empty() {
            let layout_rect =
                LayoutRect::new(0.0, workspace_selector_height, screen_size_w, screen_size_h);
            *bin = natural_layout(&windows, &layout_rect, false);
        }

        let mut state = WindowSelectorState {
            rects: vec![],
            current_selection: None,
            ..self.window_selector_view.view.get_state()
        };

        let mut delta = delta.max(0.0);
        delta = delta.powf(0.65);

        let mut index = 0;

        let mut transition = Some(Transition {
            duration: 0.5,
            timing: TimingFunction::Easing(Easing::ease_in()),
            ..Default::default()
        });
        if !end_gesture {
            // in the middle of the gesture
            transition = None;
        }

        let workspace_selector_y = (-200.0).interpolate(&0.0, delta);
        let workspace_opacity = 0.0.interpolate(&1.0, delta);
        self.workspace_selector_view.layer.set_position(
            layers::types::Point {
                x: 0.0,
                y: workspace_selector_y,
            },
            transition,
        );
        self.workspace_selector_view
            .layer
            .set_opacity(workspace_opacity, transition);

        for window in model.windows.iter() {
            let window = map.get(window).unwrap();

            let id = window.wl_surface.as_ref().unwrap().id();
            if let Some(rect) = bin.get(&id) {
                let to_x = rect.x;
                let to_y = rect.y;
                let to_width = rect.width;
                let to_height = rect.height;
                let (window_width, window_height) = (window.w, window.h);

                let scale_x = to_width / window_width;
                let scale_y = to_height / window_height;
                let scale = scale_x.min(scale_y).min(1.0);

                let window_rect = WindowSelection {
                    x: rect.x,
                    y: rect.y,
                    w: (window_width * scale),
                    h: (window_height * scale),
                    visible: true,
                    window_title: window.title.clone(),
                    index,
                };
                index += 1;
                state.rects.push(window_rect);
                let scale = 1.0.interpolate(&scale, delta);
                let delta = delta.clamp(0.0, 1.0);

                let x = window.x.interpolate(&to_x, delta);
                let y = window.y.interpolate(&to_y, delta);

                window
                    .base_layer
                    .set_position(layers::types::Point { x, y }, transition);
                window
                    .base_layer
                    .set_scale(layers::types::Point { x: scale, y: scale }, transition);
            }
        }
        self.window_selector_view.view.update_state(state);

        if end_gesture {
            *bin = HashMap::new();
        }
    }

    pub fn expose_show_desktop(&self, delta: f32, end_gesture: bool) {
        const MULTIPLIER: f32 = 1000.0;
        let gesture = self
            .show_desktop_gesture
            .load(std::sync::atomic::Ordering::Relaxed);

        let mut new_gesture = gesture + (delta * MULTIPLIER) as i32;
        let show_desktop = self.get_show_desktop();

        let size = self.workspace_layer.render_size();
        let padding_top = 10.0;
        let padding_bottom = 10.0;
        let screen_size_w = size.x;
        let screen_size_h = size.y - padding_top - padding_bottom;
        let model = self.model.read().unwrap();
        let map = self.windows_map.read().unwrap();

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
        } else {
            if !show_desktop {
                new_gesture -= MULTIPLIER as i32;
            }
        }

        let delta = new_gesture as f32 / 1000.0;

        let delta = delta.clamp(0.0, 1.0);

        let mut transition = Some(Transition {
            duration: 0.5,
            timing: TimingFunction::Easing(Easing::ease_in()),
            ..Default::default()
        });
        if !end_gesture {
            // in the middle of the gesture
            transition = None;
        }

        for window in model.windows.iter() {
            let window = map.get(window).unwrap();
            let to_x = -window.w;
            let to_y = -window.h;
            let x = window.x.interpolate(&to_x, delta);
            let y = window.y.interpolate(&to_y, delta);

            window
                .base_layer
                .set_position(layers::types::Point { x, y }, transition);
        }
    }
}

impl Observable<WorkspaceModel> for WorkspaceModel {
    fn add_listener(&mut self, observer: std::sync::Arc<dyn Observer<WorkspaceModel>>) {
        let observer = std::sync::Arc::downgrade(&observer);
        self.observers.push(observer);
    }

    fn observers<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = std::sync::Weak<dyn Observer<WorkspaceModel>>> + 'a> {
        Box::new(self.observers.iter().cloned())
    }
}
