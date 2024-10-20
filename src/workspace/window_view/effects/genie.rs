use std::{fs::read_to_string, sync::{Arc, RwLock}};

use layers::{
    skia,
    prelude::Layer
};

#[derive(Clone)]
pub struct GenieEffect{
    pub genie_builder: Arc<RwLock<skia::runtime_effect::RuntimeShaderBuilder>>,
    layer: Arc<RwLock<Option<Layer>>>
}
impl GenieEffect {
    pub fn new() -> Self {
        let sksl = read_to_string("./assets/genie.sksl").expect("Failed to read SKSL file");

        let runtime_effect = skia::RuntimeEffect::make_for_shader(sksl, None).unwrap();

        let builder = skia::runtime_effect::RuntimeShaderBuilder::new(runtime_effect);

        Self {
            genie_builder: Arc::new(RwLock::new(builder)),
            layer: Arc::new(RwLock::new(None))
        }
    }

    pub fn set_destination(&self, to_rect: skia::Rect) {
        let offset = {
            let layer = self.layer.read().unwrap();
            layer.as_ref().map(|l| {
                let r = l.render_bounds_transformed();
                (r.x(), r.y())
            }).unwrap_or((0.0, 0.0))
        };

        let mut builder = self.genie_builder.write().unwrap();
        builder.set_uniform_float("dst_bounds", &[to_rect.x() - offset.0, to_rect.y() - offset.1, to_rect.width(), to_rect.height()]);
    }
    pub fn set_source(&self, from_rect: skia::Rect) {
        let mut builder = self.genie_builder.write().unwrap();
        builder.set_uniform_float("src_bounds", &[from_rect.x(), from_rect.y(), from_rect.width(), from_rect.height()]);
    }
}
impl layers::prelude::Effect for GenieEffect {
    fn init(&self, layer: &Layer) {
        self.layer.write().unwrap().replace(layer.clone());
    }
    fn start(&self, layer: &Layer) {
        let render_layer = layer.render_bounds_with_children();
        
        self.set_source(render_layer);
        let mut builder = self.genie_builder.write().unwrap();
        builder.set_uniform_float("progress", &[0.0]);
        if let Some(filter) = skia::image_filters::runtime_shader(&builder, "", None) {
            layer.set_filter(filter);
        }
    }
    fn update(&self, layer: &Layer, progress: f32) {
        let mut builder = self.genie_builder.write().unwrap();
        builder.set_uniform_float("progress", &[progress]);
        if let Some(filter) = skia::image_filters::runtime_shader(&builder, "", None) {
            if progress > 0.0 {
                layer.set_filter(filter);
            }
        }
        if progress == 0.0 {
            layer.set_filter(None);
        }
    }
    fn finish(&self, layer: &Layer) {
        layer.set_filter(None);
    }
}