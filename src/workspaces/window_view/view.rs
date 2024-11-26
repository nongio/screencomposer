use lay_rs::{
    engine::{LayersEngine, NodeRef, TransactionRef},
    prelude::{taffy, Layer, Transition},
    skia,
    view::{RenderLayerTree, View},
};

use crate::{shell::WindowElement, workspaces::utils::view_render_elements};

use super::{
    effects::GenieEffect,
    model::{WindowViewBaseModel, WindowViewSurface},
    render::view_window_shadow,
};

#[derive(Clone)]
pub struct WindowView {
    engine: lay_rs::prelude::LayersEngine,
    // views
    pub view_base: lay_rs::prelude::View<WindowViewBaseModel>,
    pub view_content: lay_rs::prelude::View<Vec<WindowViewSurface>>,

    // layers
    pub window_layer: lay_rs::prelude::Layer,
    pub shadow_layer: lay_rs::prelude::Layer,
    pub content_layer: lay_rs::prelude::Layer,

    parent_layer_noderef: NodeRef,
    pub window: WindowElement,
    pub unmaximized_rect: lay_rs::prelude::Rectangle,
    pub genie_effect: GenieEffect,
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
            lay_rs::prelude::View::new("window_shadow", base_rect, Box::new(view_window_shadow));

        view_window_shadow.mount_layer(shadow_layer.clone());

        let view_content = View::new("window_content", render_elements, view_render_elements);
        view_content.mount_layer(content_layer.clone());

        layer.set_image_cache(true);

        let genie_effect = GenieEffect::new();

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
            genie_effect,
            unmaximized_rect: lay_rs::prelude::Rectangle {
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

    pub fn minimize(&self, to_rect: skia::Rect) -> TransactionRef {
        self.window_layer.set_effect(self.genie_effect.clone());
        self.genie_effect.set_destination(to_rect);

        let render_layer = self.window_layer.render_bounds_with_children();

        let w = render_layer.width();
        let h = render_layer.height();

        self.window_layer
            .set_draw_content(move |_: &skia::Canvas, _w, _h| {
                skia::Rect::join2(skia::Rect::from_wh(w, h), to_rect).with_outset((100.0, 100.0))
            });

        self.window_layer
            .set_image_filter_progress(1.0, Transition::linear(1.0))
    }

    pub fn unminimize(&self, from: skia::Rect) -> TransactionRef {
        self.genie_effect.set_destination(from);

        self.window_layer
            .set_image_filter_progress(0.0, Transition::linear(0.7))
            .on_finish(move |l: &Layer, _| l.remove_effect(), true)
            .clone()
    }
}

impl std::fmt::Debug for WindowView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowView")
            .field("view_base", &self.view_base)
            .field("view_content", &self.view_content)
            .field("window_layer", &self.window_layer)
            .field("content_layer", &self.content_layer)
            .field("parent_layer_noderef", &self.parent_layer_noderef)
            .field("window", &self.window)
            .finish()
    }
}
