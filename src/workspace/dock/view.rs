use std::{
    collections::HashSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use layers::{
    engine::{
        animation::{timing::TimingFunction, Transition},
        LayersEngine,
    },
    prelude::taffy,
    taffy::style::Style,
    types::Size,
    view::{RenderLayerTree, View},
};
use smithay::utils::IsAlive;
use tokio::sync::mpsc;

use crate::{
    config::Config,
    interactive_view::ViewInteractions,
    utils::Observer,
    workspace::{Application, WorkspaceModel},
};

use super::{model::DockModel, render::render_dock_view};

#[derive(Debug, Clone)]
pub struct DockView {
    // pub app_switcher: Arc<RwLock<AppSwitcherModel>>,
    pub wrap_layer: layers::prelude::Layer,
    pub view_layer: layers::prelude::Layer,
    pub view: layers::prelude::View<DockModel>,
    active: Arc<AtomicBool>,
    notify_tx: tokio::sync::mpsc::Sender<WorkspaceModel>,
    latest_event: Arc<tokio::sync::RwLock<Option<WorkspaceModel>>>,
}
impl PartialEq for DockView {
    fn eq(&self, other: &Self) -> bool {
        self.wrap_layer == other.wrap_layer
    }
}
impl IsAlive for DockView {
    fn alive(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl DockView {
    pub fn new(layers_engine: LayersEngine) -> Self {
        let wrap_layer = layers_engine.new_layer();
        wrap_layer.set_size(Size::percent(1.0, 1.0), None);
        wrap_layer.set_layout_style(Style {
            position: layers::taffy::style::Position::Absolute,
            display: layers::taffy::style::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center), // horizontal
            align_items: Some(taffy::AlignItems::FlexEnd),        // vertical alignment
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        });
        layers_engine.scene_add_layer(wrap_layer.clone());
        let view_layer = layers_engine.new_layer();
        wrap_layer.add_sublayer(view_layer.clone());
        // FIXME
        view_layer.set_position((0.0, 500.0), None);
        let mut initial_state = DockModel::new();
        initial_state.width = 1000;
        let view = View::new("dock", initial_state, render_dock_view);
        view.mount_layer(view_layer.clone());
        let (notify_tx, notify_rx) = mpsc::channel(5);
        let dock = Self {
            wrap_layer,
            view_layer,
            view,
            active: Arc::new(AtomicBool::new(true)),
            notify_tx,
            latest_event: Arc::new(tokio::sync::RwLock::new(None)),
        };
        dock.init_notification_handler(notify_rx);
        dock
    }

    pub fn hide(&self) {
        self.active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            0.0,
            Some(Transition {
                duration: 0.4,
                delay: 0.0,
                timing: TimingFunction::default(),
            }),
        );
    }

    fn init_notification_handler(&self, mut rx: tokio::sync::mpsc::Receiver<WorkspaceModel>) {
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
                    let apps: Vec<Application> = workspace
                        .application_list
                        .iter()
                        .rev()
                        .filter_map(|app_id| {
                            let app = workspace.applications_cache.get(app_id).unwrap().to_owned();

                            if app_set.insert(app.identifier.clone()) {
                                Some(app)
                            } else {
                                None
                            }
                        })
                        .collect();

                    let state = view.get_state();
                    view.update_state(&DockModel {
                        running_apps: apps,
                        ..state
                    });
                }
            }
        });
    }
    pub fn update_magnification_position(&self, pos: f32) {
        let bounds = self.view_layer.render_bounds();
        let focus = pos / bounds.width();
        let state = self.view.get_state();

        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
        let icon_size: f32 = 100.0 * draw_scale;

        let apps_len = state.running_apps.len() as f32;
        let mut component_padding_h: f32 = icon_size * 0.09 * draw_scale;
        if component_padding_h > 5.0 * draw_scale {
            component_padding_h = 5.0 * draw_scale;
        }
        let mut component_padding_v: f32 = icon_size * 0.09 * draw_scale;
        if component_padding_v > 50.0 * draw_scale {
            component_padding_v = 50.0 * draw_scale;
        }
        let mut total_width = component_padding_h * 2.0;
        for (index, app) in state.running_apps.iter().enumerate() {
            let id = format!("app_{}", app.identifier);
            if let Some(layer) = self.view.layer_by_id(&id) {
                let icon_pos = 1.0 / apps_len * index as f32 + 1.0 / (apps_len * 2.0);
                let icon_focus = 1.0 + magnify_function(focus - icon_pos) * 0.2;
                // println!("x: {} -> {}", icon_pos, icon_focus);
                let focused_icon_size = icon_size * icon_focus as f32;
                layer.set_size(
                    Size::points(focused_icon_size, focused_icon_size + 30.0),
                    Some(Transition {
                        duration: 0.1,
                        ..Default::default()
                    }),
                );
                total_width += focused_icon_size;
            }
        }
        if let Some(layer) = self.view.layer_by_id("dock_bar") {
            layer.set_size(
                Size::points(total_width, component_padding_v * 2.0 + icon_size),
                Some(Transition {
                    duration: 0.1,
                    ..Default::default()
                }),
            );
        }
    }
}

impl Observer<WorkspaceModel> for DockView {
    fn notify(&self, event: &WorkspaceModel) {
        let _ = self.notify_tx.try_send(event.clone());
    }
}

impl<Backend: crate::state::Backend> ViewInteractions<Backend> for DockView {
    fn id(&self) -> Option<usize> {
        self.wrap_layer.id().map(|id| id.0.into())
    }
    fn is_alive(&self) -> bool {
        self.alive()
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        _data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        // let _id = self.view_layer.id().unwrap();
        let scale = Config::with(|c| c.screen_scale);

        self.update_magnification_position(
            (event.location.x * scale) as f32 - self.view_layer.render_position().x,
        );
        // self.view_layer.engine.pointer_move(
        //     (
        //         (event.location.x * scale) as f32,
        //         (event.location.y * scale) as f32,
        //     ),
        //     id.0,
        // );
    }
    fn on_leave(&self, _serial: smithay::utils::Serial, _time: u32) {
        self.update_magnification_position(-500.0);
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        _data: &mut crate::ScreenComposer<Backend>,
        _event: &smithay::input::pointer::ButtonEvent,
    ) {
        // let id = self.view_layer.id().unwrap();
        // let scale = Config::with(|c| c.screen_scale);
        // match event.state {
        //     ButtonState::Pressed => {
        //         self.view_layer.engine.pointer_button_down();
        //     }
        //     ButtonState::Released => {
        //         self.view_layer.engine.pointer_button_up();
        //     }
        // }
    }
}
use std::f64::consts::E;

pub fn magnify_function(x: impl Into<f64>) -> f64 {
    let x = x.into();
    E.powf(-10.0 * (x).powi(2))
}
