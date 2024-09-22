use layers::{
    engine::{LayersEngine, NodeRef},
    prelude::taffy,
    view::{RenderLayerTree, View},
};

use crate::{shell::WindowElement, workspace::utils::view_render_elements};

use super::{
    model::{WindowViewBaseModel, WindowViewSurface},
    render::view_window_shadow,
};

#[derive(Clone)]
pub struct WindowView {
    engine: layers::prelude::LayersEngine,
    pub view_base: layers::prelude::View<WindowViewBaseModel>,
    pub view_content: layers::prelude::View<Vec<WindowViewSurface>>,
    pub window_layer: layers::prelude::Layer,
    pub shadow_layer: layers::prelude::Layer,
    pub content_layer: layers::prelude::Layer,
    parent_layer_noderef: NodeRef,
    pub window: WindowElement,
    pub unmaximized_rect: layers::prelude::Rectangle,
}

impl WindowView {
    pub fn new(
        layers_engine: LayersEngine,
        parent_layer_noderef: NodeRef,
        window: WindowElement,
    ) -> Self {
        let layer = layers_engine.new_layer();
        layer.set_key("window");
        layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        let content_layer = layers_engine.new_layer();
        content_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });

        let shadow_layer = layers_engine.new_layer();
        shadow_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layers_engine.scene_add_layer_to(layer.clone(), Some(parent_layer_noderef));
        layers_engine.scene_add_layer_to(shadow_layer.clone(), layer.id());
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
            fullscreen: false,
        };
        let view_window_shadow =
            layers::prelude::View::new("window_shadow", base_rect, Box::new(view_window_shadow));

        view_window_shadow.mount_layer(shadow_layer.clone());

        let view_content = View::new("window_content", render_elements, view_render_elements);
        view_content.mount_layer(content_layer.clone());

        Self {
            view_base: view_window_shadow,
            view_content,
            engine: layers_engine,
            // state,
            window_layer: layer,
            content_layer,
            shadow_layer,
            parent_layer_noderef,
            window,
            unmaximized_rect: layers::prelude::Rectangle {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
        }
    }

    pub fn raise(&self) {
        self.engine
            .scene_add_layer_to(self.window_layer.clone(), Some(self.parent_layer_noderef));
    }
}
