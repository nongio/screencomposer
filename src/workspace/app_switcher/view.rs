use std::sync::{atomic::AtomicBool, Arc};

use layers::{
    engine::{
        animation::{timing::TimingFunction, Transition},
        LayersEngine,
    },
    prelude::taffy,
    taffy::style::Style,
    types::Size,
};
use smithay::utils::IsAlive;

use crate::{interactive_view::ViewInteractions, state::Backend, utils::Observer, workspace::Workspace};

use super::render::render_appswitcher_view;

use super::model::AppSwitcherModel;

#[derive(Debug, Clone)]
pub struct AppSwitcherView {
    // pub app_switcher: Arc<RwLock<AppSwitcherModel>>,
    pub wrap_layer: layers::prelude::Layer,
    pub view_layer: layers::prelude::Layer,
    pub view: layers::prelude::View<AppSwitcherModel>,
    active: Arc<AtomicBool>,
}
impl PartialEq for AppSwitcherView {
    fn eq(&self, other: &Self) -> bool {
        self.wrap_layer == other.wrap_layer
    }
}
impl IsAlive for AppSwitcherView {
    fn alive(&self) -> bool {    
        self.active.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl AppSwitcherView {
    pub fn new(layers_engine: LayersEngine) -> Self {
        let wrap = layers_engine.new_layer();
        wrap.set_size(Size::percent(1.0, 1.0), None);
        wrap.set_layout_style(Style {
            display: layers::taffy::style::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        });
        wrap.set_opacity(0.0, None);

        layers_engine.scene_add_layer(wrap.clone());
        let layer = layers_engine.new_layer();
        wrap.add_sublayer(layer.clone());
        let mut initial_state = AppSwitcherModel::new();
        initial_state.width = 1000;
        let view = layers::prelude::View::new(layer.clone(), initial_state, Box::new(render_appswitcher_view));
        Self {
            // app_switcher: Arc::new(RwLock::new(AppSwitcherModel::new())),
            wrap_layer: wrap.clone(),
            view_layer: layer.clone(),
            view,
            active: Arc::new(AtomicBool::new(false)),
        }
    }
    // pub fn set_width(&self, width: i32) {
    //     self.view.update_state(AppSwitcherModel {
    //         width,
    //         ..self.view.get_state()
    //     });
    // }

    pub fn update(&self) {
        self.view.update_state(AppSwitcherModel {
            width: 1000,
            ..self.view.get_state()
        });
        // let state = self.app_switcher.read().unwrap();
        // let view = self.view;//.read().unwrap();
        // if self.view.render(&state) {
        //     // if let Some(layer) = view.get_layer_by_id("app_org.freedesktop.weston.wayland-terminal") {
        //     //     layer.on_pointer_move(|x,y| {
        //     //         println!("pointer move {}, {}", x, y);
        //     //     });
        //     // }
        // }
    }
    // pub fn next_app(&mut self) {
    //     // let apps = self.apps.read().unwrap();
    //     if !self.apps.is_empty() {
    //         self.current_app = (self.current_app + 1) % self.apps.len();
    //     } else {
    //         self.current_app = 0;
    //     }
    // }
    // pub fn previous_app(&mut self) {
    //     // let apps = self.apps.read().unwrap();
        
    //     if !self.apps.is_empty() {
    //         self.current_app = (self.current_app + self.apps.len() - 1) % self.apps.len();
    //     } else {
    //         self.current_app = 0;
    //     }
    // }
    pub fn next(&self) {
        let app_switcher = self.view.get_state();
        let mut current_app = app_switcher.current_app;
        if !app_switcher.apps.is_empty() {
            current_app = (current_app + 1) % app_switcher.apps.len();
        } else {
            current_app = 0;
        }

        self.view.update_state(AppSwitcherModel {
            current_app,
            ..app_switcher
        });

        self.active.store(true, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            1.0,
            Some(Transition {
                duration: 0.1,
                delay: 0.1,
                timing: TimingFunction::default(),
            }),
        );
    }
    pub fn previous(&self) {
        let app_switcher = self.view.get_state();
        let mut current_app = app_switcher.current_app;
        if !app_switcher.apps.is_empty() {
            current_app = (current_app + app_switcher.apps.len() - 1) % app_switcher.apps.len();
        } else {
            current_app = 0;
        }

        self.view.update_state(AppSwitcherModel {
            current_app,
            ..app_switcher
        });
        
        self.active.store(true, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            1.0,
            Some(Transition {
                duration: 0.1,
                delay: 0.1,
                timing: TimingFunction::default(),
            }),
        );
    }

    pub fn hide(&self) {
        self.active.store(false, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            0.0,
            Some(Transition {
                duration: 0.3,
                delay: 0.0,
                timing: TimingFunction::default(),
            }),
        );
    }

    pub fn quit_current_app(&mut self) {
        // if self.active {
        //     let windows = self.app_switcher.current_window_elements();
        //     for we in windows {
        //         match we {
        //             WindowElement::Wayland(w) => w.toplevel().send_close(),
        //             #[cfg(feature = "xwayland")]
        //             WindowElement::X11(w) => {
        //                 let _ = w.close();
        //             }
        //         };
        //     }
        // }
    }
}

impl Observer<Workspace> for AppSwitcherView {
   fn notify(&self, event: &Workspace) {
        println!("AppSwitcherView received event");

        let workspace = event.model.read().unwrap();

        let apps = workspace.application_list.iter().map(|app_id| {
            workspace.applications.get(app_id).unwrap().to_owned()
        }).collect();
        self.view.update_state(AppSwitcherModel {
            apps,
            ..self.view.get_state()
        });
    }
}

impl<Backend: crate::state::Backend> ViewInteractions<Backend> for AppSwitcherView {
    fn id(&self) -> Option<usize> {
        self.wrap_layer.id().map(|id| id.0.into())
    }
    fn is_alive(&self) -> bool {
        self.alive()
    }
    fn on_motion(&self, event: &smithay::input::pointer::MotionEvent) {
        // println!("AppSwitcherView on_motion {} {}", event.location.x, event.location.y);
        let id = self.view_layer.id().unwrap();
        self.view_layer.engine.pointer_move((event.location.x as f32, event.location.y as f32), id.0);
    }
}