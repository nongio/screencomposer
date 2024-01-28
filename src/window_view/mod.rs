use layers::{engine::{LayersEngine, NodeRef}, prelude::taffy};

use self::view::{view_window, WindowViewState};

pub mod view;

pub struct WindowView {
    engine: layers::prelude::LayersEngine,
    pub view: layers::prelude::View<WindowViewState>,
    pub state: WindowViewState,
    pub layer: layers::prelude::Layer,
    parent_layer: NodeRef,
}

impl WindowView {
    pub fn new(layers_engine: LayersEngine, parent_layer: NodeRef) -> Self {
        let layer = layers_engine.new_layer();
        layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layers_engine.scene_add_layer_to(layer.clone(), Some(parent_layer));

        let mut view = layers::prelude::View::new(layer.clone(), Box::new(view_window));
        let state = WindowViewState {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            window_element: None,
            render_elements: Vec::new(),
            title: String::new(),
        };
        view.render(&state);
        Self {
            view,
            engine: layers_engine,
            state,
            layer,
            parent_layer,
        }
    }
    #[profiling::function]
    pub fn render(&mut self) {
        self.view.render(&self.state);
    }

    pub fn raise(&self) {
        self.engine.scene_add_layer_to(self.layer.clone(), Some(self.parent_layer.clone()));
    }
}
