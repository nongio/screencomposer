use std::hash::{Hash, Hasher};

use lay_rs::{prelude::*, skia};

#[derive(Clone, Debug)]
pub struct BackgroundViewState {
    pub image: Option<skia::Image>,
    pub debug_string: String,
}
impl Hash for BackgroundViewState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(image) = self.image.as_ref() {
            image.unique_id().hash(state);
        }
        self.debug_string.hash(state);
    }
}

pub struct BackgroundView {
    // engine: lay_rs::prelude::LayersEngine,
    pub view: lay_rs::prelude::View<BackgroundViewState>,
    // pub state: RwLock<BackgroundViewState>,
    pub base_layer: Layer,
}

impl BackgroundView {
    pub fn new(index: usize, layer: Layer) -> Self {
        let state = BackgroundViewState {
            image: None,
            debug_string: "Screen composer 0.1".to_string(),
        };
        let view = lay_rs::prelude::View::new(
            format!("background_view_{}", index),
            state,
            Box::new(view_background),
        );
        view.mount_layer(layer.clone());
        Self {
            view,
            base_layer: layer,
        }
    }

    pub fn set_debug_text(&self, text: String) {
        self.view.update_state(&BackgroundViewState {
            debug_string: text,
            ..self.view.get_state()
        });
    }

    pub fn set_image(&self, image: skia::Image) {
        self.view.update_state(&BackgroundViewState {
            image: Some(image),
            ..self.view.get_state()
        });
    }
}

// static mut COUNTER: f32 = 1.0;
pub fn view_background(
    state: &BackgroundViewState,
    _view: &View<BackgroundViewState>,
) -> LayerTree {
    let image = state.image.clone();

    // let debug_text = state.debug_string.clone();

    let draw_container = move |canvas: &skia::Canvas, w, h| {
        let color = skia::Color4f::new(1.0, 1.0, 1.0, 1.0);
        let mut paint = skia::Paint::new(color, None);

        if let Some(image) = image.as_ref() {
            let mut matrix = skia::Matrix::new_identity();
            let image_width = image.width() as f32;
            let image_height = image.height() as f32;
            let scale_x: f32 = w / image_width;
            let scale_y: f32 = h / image_height;
            let scale = scale_x.max(scale_y); // Choose the smaller scale to maintain aspect ratio

            // Calculate the offsets for centering the image
            let offset_x = (w - image_width * scale) / 2.0;
            let offset_y = (h - image_height * scale) / 2.0;

            matrix.set_scale_translate((scale, scale), (offset_x, offset_y)); // canvas.concat(&matrix);
                                                                              // canvas.draw_image_rect(image, None, rect, &paint);
            paint.set_shader(image.to_shader(
                (skia::TileMode::Repeat, skia::TileMode::Repeat),
                skia::SamplingOptions::default(),
                &matrix,
            ));
        }

        let split = 1;
        let rect_size_w = w / split as f32;
        let rect_size_h = h / split as f32;

        for i in 0..split {
            for j in 0..split {
                let rect = skia::Rect::from_xywh(
                    i as f32 * rect_size_w,
                    j as f32 * rect_size_h,
                    rect_size_w,
                    rect_size_h,
                );
                canvas.draw_rect(rect, &paint);
            }
        }

        // let color = skia::Color4f::new(0.0, 0.0, 0.0, 1.0);
        // let paint = skia::Paint::new(color, None);
        // let mut font = skia::Font::default();
        // let font_size = 26.0;
        // font.set_size(font_size);
        // canvas.draw_str("test string string", (80.0, 100.0), &font, &paint);
        // canvas.draw_rect(skia::Rect::from_xywh(80.0, 100.0, 200.0, 100.0), &paint);
        skia::Rect::from_xywh(0.0, 0.0, w, h)
    };

    LayerTreeBuilder::default()
        .key("background_view")
        .opacity((
            1.0,
            Some(Transition {
                delay: 0.2,
                timing: TimingFunction::ease_out_quad(0.8),
            }),
        ))
        .border_corner_radius(BorderRadius::new_single(24.0))
        .content(Some(draw_container))
        .image_cache(true)
        .background_color(lay_rs::prelude::Color::new_rgba(0.0, 0.0, 0.0, 1.0))
        .pointer_events(false)
        .build()
        .unwrap()
}
