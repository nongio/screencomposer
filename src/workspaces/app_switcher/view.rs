use std::{
    collections::HashSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use lay_rs::{
    engine::{
        animation::{TimingFunction, Transition},
        Engine, TransactionRef,
    },
    prelude::taffy,
    taffy::style::Style,
    types::Size,
    view::RenderLayerTree,
};
use smithay::utils::IsAlive;
use tokio::sync::mpsc;

use crate::{
    interactive_view::ViewInteractions,
    utils::Observer,
    workspaces::{apps_info::ApplicationsInfo, Application, WorkspacesModel},
};

use super::render::render_appswitcher_view;

use super::model::AppSwitcherModel;

#[derive(Debug, Clone)]
pub struct AppSwitcherView {
    pub wrap_layer: lay_rs::prelude::Layer,
    pub view_layer: lay_rs::prelude::Layer,
    pub view: lay_rs::prelude::View<AppSwitcherModel>,
    active: Arc<AtomicBool>,
    notify_tx: tokio::sync::mpsc::Sender<WorkspacesModel>,
    latest_event: Arc<tokio::sync::RwLock<Option<WorkspacesModel>>>,
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

/// # AppSwitcherView Layer Structure
///
/// ```diagram
/// AppSwitcherView
/// └── app_switcher_container `app_switcher_container`
///     └── app_switcher `app_switcher`
///         └── apps_container `apps_container`
///             ├── App
///             ├── App
/// ```
///

impl AppSwitcherView {
    pub fn new(layers_engine: Arc<Engine>) -> Self {
        let wrap = layers_engine.new_layer();
        wrap.set_key("app_switcher");
        wrap.set_size(Size::percent(1.0, 1.0), None);
        wrap.set_layout_style(Style {
            position: lay_rs::taffy::style::Position::Absolute,
            display: lay_rs::taffy::style::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        });
        wrap.set_opacity(0.0, None);

        let layer = layers_engine.new_layer();
        layers_engine.add_layer(&wrap);
        wrap.add_sublayer(&layer);
        wrap.set_pointer_events(false);
        layer.set_pointer_events(false);
        let mut initial_state = AppSwitcherModel::new();
        initial_state.width = 1000;
        let view = lay_rs::prelude::View::new(
            "apps_switcher_inner",
            initial_state,
            Box::new(render_appswitcher_view),
        );
        view.mount_layer(layer.clone());
        let (notify_tx, notify_rx) = mpsc::channel(5);
        let app_switcher = Self {
            // app_switcher: Arc::new(RwLock::new(AppSwitcherModel::new())),
            wrap_layer: wrap.clone(),
            view_layer: layer.clone(),
            view,
            active: Arc::new(AtomicBool::new(false)),
            notify_tx,
            latest_event: Arc::new(tokio::sync::RwLock::new(None)),
        };
        app_switcher.init_notification_handler(notify_rx);
        app_switcher
    }

    pub fn next(&self) {
        let app_switcher = self.view.get_state();
        let mut current_app = app_switcher.current_app;

        // reset current_app on first load
        // the current app is on the first place
        if !self.active.load(std::sync::atomic::Ordering::Relaxed) {
            current_app = 0;
        }

        if !app_switcher.apps.is_empty() {
            current_app = (current_app + 1) % app_switcher.apps.len();
        } else {
            current_app = 0;
        }

        self.view.update_state(&AppSwitcherModel {
            current_app,
            ..app_switcher
        });

        self.active
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            1.0,
            Some(Transition {
                delay: 0.1,
                timing: TimingFunction::ease_out_quad(0.150),
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

        self.view.update_state(&AppSwitcherModel {
            current_app,
            ..app_switcher
        });

        self.active
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            1.0,
            Some(Transition {
                delay: 0.05,
                timing: TimingFunction::linear(0.01),
            }),
        );
    }

    pub fn hide(&self) -> TransactionRef {
        self.active
            .store(false, std::sync::atomic::Ordering::Relaxed);

        self.wrap_layer
            .set_opacity(0.0, Some(Transition::ease_in_quad(0.1)))
    }

    pub fn reset(&self) {
        let state = self.view.get_state();
        self.view.update_state(&AppSwitcherModel {
            current_app: 0,
            ..state
        });
    }

    pub fn get_current_app_id(&self) -> Option<String> {
        let state = self.view.get_state();
        state
            .apps
            .get(state.current_app)
            .map(|app| app.identifier.clone())
    }

    fn init_notification_handler(&self, mut rx: tokio::sync::mpsc::Receiver<WorkspacesModel>) {
        let view = self.view.clone();
        let latest_event = self.latest_event.clone();
        // Task to receive events
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                // Store the latest event
                *latest_event.write().await = Some(event.clone());
            }
        });
        let latest_event = self.latest_event.clone();
        tokio::spawn(async move {
            loop {
                // app switcher updates don't need to be instantanious
                tokio::time::sleep(Duration::from_secs_f32(0.4)).await;

                let event = {
                    let mut latest_event_lock = latest_event.write().await;
                    latest_event_lock.take()
                };

                if let Some(workspace) = event {
                    let mut app_set = HashSet::new();
                    let mut apps: Vec<Application> = Vec::new();

                    for app_id in workspace.zindex_application_list.iter().rev() {
                        if app_set.insert(app_id.clone()) {
                            if let Some(app) = ApplicationsInfo::get_app_info_by_id(app_id).await {
                                apps.push(app);
                            }
                        }
                    }

                    let switcher_state = view.get_state();
                    let mut current_app = switcher_state.current_app;
                    if apps.is_empty() {
                        current_app = 0;
                    } else if (current_app + 1) > apps.len() {
                        current_app = apps.len() - 1;
                    }
                    view.update_state(&AppSwitcherModel {
                        current_app,
                        apps,
                        width: workspace.width,
                        // ..switcher_state
                    });
                }
            }
        });
    }
}

impl Observer<WorkspacesModel> for AppSwitcherView {
    fn notify(&self, event: &WorkspacesModel) {
        let _ = self.notify_tx.try_send(event.clone());
    }
}

impl<Backend: crate::state::Backend> ViewInteractions<Backend> for AppSwitcherView {
    fn id(&self) -> Option<usize> {
        Some(self.wrap_layer.id.0.into())
    }
    fn is_alive(&self) -> bool {
        self.alive()
    }
    // fn on_motion(
    //     &self,
    //     _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
    //     _data: &mut crate::ScreenComposer<Backend>,
    //     event: &smithay::input::pointer::MotionEvent,
    // ) {
    //     let id = self.view_layer.id().unwrap();
    //     let scale = Config::with(|c| c.screen_scale);
    //     self.view_layer.engine.pointer_move(
    //         (
    //             (event.location.x * scale) as f32,
    //             (event.location.y * scale) as f32,
    //         ),
    //         id.0,
    //     );
    // }
    // fn on_button(
    //     &self,
    //     _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
    //     _data: &mut crate::ScreenComposer<Backend>,
    //     event: &smithay::input::pointer::ButtonEvent,
    // ) {
    //     // let id = self.view_layer.id().unwrap();
    //     // let scale = Config::with(|c| c.screen_scale);
    //     match event.state {
    //         ButtonState::Pressed => {
    //             self.view_layer.engine.pointer_button_down();
    //         }
    //         ButtonState::Released => {
    //             self.view_layer.engine.pointer_button_up();
    //         }
    //     }
    // }
}
