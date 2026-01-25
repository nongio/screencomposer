use layers::{
    engine::Engine,
    prelude::taffy,
    types::Point,
    view::{RenderLayerTree, View},
};
use std::sync::Arc;

use crate::workspaces::utils::view_render_elements_wrapper;
use crate::workspaces::WindowViewSurface;

#[derive(Clone)]
pub struct DndView {
    pub view_content: layers::prelude::View<Vec<WindowViewSurface>>,

    pub layer: layers::prelude::Layer,
    pub content_layer: layers::prelude::Layer,
    // _parent_layer_noderef: NodeRef,
    pub initial_position: Point,
}

impl DndView {
    pub fn new(layers_engine: Arc<Engine>) -> Self {
        let layer = layers_engine.new_layer();
        layer.set_key("dnd_view");
        layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        // layer.set_opacity(0.0, None);
        let content_layer = layers_engine.new_layer();
        content_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });

        layers_engine.add_layer(&layer);
        layers_engine.append_layer(&content_layer, layer.id());

        let render_elements = Vec::new();

        let view_content = View::new("dnd", render_elements, view_render_elements_wrapper);
        view_content.mount_layer(content_layer.clone());

        Self {
            view_content,
            layer,
            content_layer,
            initial_position: Point::default(),
        }
    }
    pub fn set_initial_position(&mut self, point: Point) {
        self.initial_position = point;
    }
}
