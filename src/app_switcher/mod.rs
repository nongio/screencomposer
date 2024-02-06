pub mod appswitcher_view;
pub mod app_icon_view;
pub mod state;

use layers::{
    engine::{
        animation::{timing::TimingFunction, Transition},
        LayersEngine,
    },
    prelude::taffy,
    taffy::style::Style,
    types::Size,
};
use smithay::{input::pointer::PointerTarget, wayland::shell::xdg::XdgToplevelSurfaceData};

use crate::{
    app_switcher::appswitcher_view::render_appswitcher_view,
    shell::WindowElement, state::Backend, ScreenComposer,
};

use self::state::AppSwitcherState;

pub struct AppSwitcherView {
    pub app_switcher: AppSwitcherState,
    pub layer: layers::prelude::Layer,
    pub view: layers::prelude::View<AppSwitcherState>,
    active: bool,
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

        let view = layers::prelude::View::new(layer.clone(), Box::new(render_appswitcher_view));
        Self {
            app_switcher: AppSwitcherState::new(),
            layer: wrap.clone(),
            view,
            active: false,
        }
    }

    pub fn update(&mut self) {
        self.app_switcher.width = 1000;
        if self.view.render(&self.app_switcher) {
            
        }
    }

    pub(crate) fn update_with_window_elements(&mut self, windows: &[WindowElement]) {
        let mut apps = Vec::new();
        // let mut seen = HashSet::new();
        windows
            .iter()
            .filter(|w| w.wl_surface().is_some())
            .for_each(|w| {
                smithay::wayland::compositor::with_states(
                    w.wl_surface().as_ref().unwrap(),
                    |states| {
                        let attributes: std::sync::MutexGuard<'_, smithay::wayland::shell::xdg::XdgToplevelSurfaceRoleAttributes> = states
                            .data_map
                            .get::<XdgToplevelSurfaceData>()
                            .unwrap()
                            .lock()
                            .unwrap();
                        if let Some(app_id) = attributes.app_id.as_ref() {
                            apps.push((app_id.clone(), w.clone()));
                        }
                    },
                );
            });
        
        self.app_switcher.update_apps(apps.as_slice());
        self.update();
    }
    pub fn next(&mut self) {
        self.app_switcher.next_app();
        self.update();
        self.active = true;
        self.layer.set_opacity(
            1.0,
            Some(Transition {
                duration: 0.1,
                delay: 0.1,
                timing: TimingFunction::default(),
            }),
        );
    }
    pub fn previous(&mut self) {
        self.app_switcher.previous_app();
        self.update();
        self.active = true;
        self.layer.set_opacity(
            1.0,
            Some(Transition {
                duration: 0.1,
                delay: 0.1,
                timing: TimingFunction::default(),
            }),
        );
    }
    pub fn next_window(&mut self) {
        self.app_switcher.next_window();
    }
    pub fn hide(&mut self) {
        self.active = false;
        self.layer.set_opacity(
            0.0,
            Some(Transition {
                duration: 0.3,
                delay: 0.0,
                timing: TimingFunction::default(),
            }),
        );
    }

    pub fn quit_current_app(&mut self) {
        if self.active {
            let windows = self.app_switcher.current_window_elements();
            for we in windows {
                match we {
                    WindowElement::Wayland(w) => w.toplevel().send_close(),
                    #[cfg(feature = "xwayland")]
                    WindowElement::X11(w) => {
                        let _ = w.close();
                    }
                };
            }
        }
    }
}

// impl<BackendData: Backend> PointerTarget<ScreenComposer<BackendData>> for AppSwitcherView {
    
// }