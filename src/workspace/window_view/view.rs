
use layers::{engine::{LayersEngine, NodeRef}, prelude::taffy};

use crate::shell::WindowElement;

use super::{model::{WindowViewBaseModel, WindowViewSurface}, render::{view_base_window, view_content_window}};

#[derive(Clone)]
pub struct WindowView {
    engine: layers::prelude::LayersEngine,
    pub view_base: layers::prelude::View<WindowViewBaseModel>,
    pub view_content: layers::prelude::View<Vec<WindowViewSurface>>,
    // pub state: WindowViewModel,
    pub layer: layers::prelude::Layer,
    pub base_layer: layers::prelude::Layer,
    pub content_layer: layers::prelude::Layer,
    parent_layer_noderef: NodeRef,
    pub window: WindowElement,
}

impl WindowView {
    pub fn new(layers_engine: LayersEngine, parent_layer_noderef: NodeRef, window: WindowElement) -> Self {
        let layer = layers_engine.new_layer();
        layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        let content_layer = layers_engine.new_layer();
        content_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        
        let base_layer = layers_engine.new_layer();
        base_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layers_engine.scene_add_layer_to(layer.clone(), Some(parent_layer_noderef));
        layers_engine.scene_add_layer_to(base_layer.clone(), layer.id());
        layers_engine.scene_add_layer_to(content_layer.clone(), layer.id());

        // let state = WindowViewModel {
        //     window_element: None,
        //     title: String::new(),
        // };
        let render_elements = Vec::new();
        let base_rect = WindowViewBaseModel {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            title: "".to_string(),
        };
        let view_base = layers::prelude::View::new(base_layer.clone(), base_rect, Box::new(view_base_window));
        let view_content = layers::prelude::View::new(content_layer.clone(), render_elements, Box::new(view_content_window));
        
        // view_base.render(&state.base_rect);
        // view_content.render(&state.render_elements);
        Self {
            view_base,
            view_content,
            engine: layers_engine,
            // state,
            layer,
            content_layer,
            base_layer,
            parent_layer_noderef,
            window,
        }
    }
    // #[profiling::function]
    // pub fn render(&mut self) {
    //     self.view_base.render(&self.state.base_rect);
    //     self.view_content.render(&self.state.render_elements);
    // }

    pub fn raise(&self) {
        self.engine.scene_add_layer_to(self.layer.clone(), Some(self.parent_layer_noderef));
    }
}
