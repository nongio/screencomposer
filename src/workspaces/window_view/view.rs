use lay_rs::{
    engine::{Engine, TransactionRef},
    prelude::{taffy, Layer, Transition},
    skia,
    view::{RenderLayerTree, View},
};
use std::sync::Arc;
use smithay::{reexports::wayland_server::backend::ObjectId, utils::Logical};

use crate::{shell::WindowElement, workspaces::utils::view_render_elements};

use super::{
    effects::GenieEffect,
    model::{WindowViewBaseModel, WindowViewSurface},
    render::view_window_shadow,
};

#[derive(Clone)]
pub struct WindowView {
    pub window_id: ObjectId,
    // views
    pub view_base: lay_rs::prelude::View<WindowViewBaseModel>,
    pub view_content: lay_rs::prelude::View<Vec<WindowViewSurface>>,

    // layers
    pub window_layer: lay_rs::prelude::Layer,
    pub shadow_layer: lay_rs::prelude::Layer,
    pub content_layer: lay_rs::prelude::Layer,

    pub genie_effect: GenieEffect,

    pub unmaximised_rect: smithay::utils::Rectangle<i32, Logical>,
}

impl WindowView {
    pub fn new(layers_engine: Arc<Engine>, window: &WindowElement) -> Self {
        let window_id = window.id();
        let layer = window.base_layer().clone();
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

        layers_engine.append_layer(&shadow_layer, layer.id());
        layers_engine.append_layer(&content_layer, layer.id());

        let render_elements = Vec::new();
        let base_rect = WindowViewBaseModel {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            title: "".to_string(),
            fullscreen: false,
            active: false,
        };
        let view_base =
            lay_rs::prelude::View::new("window_shadow", base_rect, Box::new(view_window_shadow));

        view_base.mount_layer(shadow_layer.clone());

        let view_content = View::new("window_content", render_elements, view_render_elements);
        view_content.mount_layer(content_layer.clone());

        layer.set_image_cached(true);

        let genie_effect = GenieEffect::new();

        Self {
            window_id,
            view_base,
            view_content,
            // state,
            window_layer: layer,
            content_layer,
            shadow_layer,
            genie_effect,

            unmaximised_rect: smithay::utils::Rectangle::default(),
        }
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

        *self
            .window_layer
            .set_image_filter_progress(0.0, Transition::linear(0.7))
            .on_finish(move |l: &Layer, _| l.remove_effect(), true)
    }
}

impl std::fmt::Debug for WindowView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowView")
            .field("view_base", &self.view_base)
            .field("view_content", &self.view_content)
            .field("window_layer", &self.window_layer)
            .field("content_layer", &self.content_layer)
            .finish()
    }
}
