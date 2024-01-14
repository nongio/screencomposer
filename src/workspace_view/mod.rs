use layers::{engine::LayersEngine, prelude::Layer};
use smithay::backend::renderer::utils::CommitCounter;

use self::{view::{view_workspace, WorkspaceViewState}, background::{BackgroundViewState, view_background}};

pub mod view;
pub mod background;

pub struct WorkspaceView {
    engine: layers::prelude::LayersEngine,
    pub view: layers::prelude::View<WorkspaceViewState>,
    pub state: WorkspaceViewState,
}

impl WorkspaceView {
    pub fn new(layers_engine: LayersEngine) -> Self {
        let layer = layers_engine.new_layer();
        layers_engine.scene_add_layer(layer.clone());

        let mut view = layers::prelude::View::new(layer, Box::new(view_workspace));
        let state = WorkspaceViewState {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            image: None,
        };
        view.render(&state);
        Self {
            view,
            engine: layers_engine,
            state,
        }
    }
    pub fn render(&mut self) {
        self.view.render(&self.state);
    }
}

pub struct BackgroundView {
    engine: layers::prelude::LayersEngine,
    pub view: layers::prelude::View<BackgroundViewState>,
    pub state: BackgroundViewState,
}

impl BackgroundView {
    pub fn new(layers_engine: LayersEngine, layer: Layer) -> Self {
        let mut view = layers::prelude::View::new(layer, Box::new(view_background));
        let state = BackgroundViewState {
            image: None,
        };
        view.render(&state);
        Self {
            view,
            engine: layers_engine,
            state,
        }
    }
    pub fn render(&mut self) {
        self.view.render(&self.state);
    }
}