use std::hash::{Hash, Hasher};

use layers::prelude::*;

use crate::utils::Observer;

use super::Workspace;

#[derive(Clone, Debug)]
pub struct BackgroundViewState {
    pub image: Option<skia_safe::Image>,
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
    // engine: layers::prelude::LayersEngine,
    pub view: layers::prelude::View<BackgroundViewState>,
    // pub state: RwLock<BackgroundViewState>,
}

impl BackgroundView {
    pub fn new(_layers_engine: LayersEngine, layer: Layer) -> Self {
        let state = BackgroundViewState {
            image: None,
            debug_string: "Screen composer 0.1".to_string(),
        };
        let view = layers::prelude::View::new(layer, state, Box::new(view_background));

        Self {
            // engine: layers_engine,
            view,
            // state: RwLock::new(state),
        }
    }

    pub fn set_debug_text(&self, text: String) {
        self.view.update_state(BackgroundViewState {
            debug_string: text,
            ..self.view.get_state()
        });
    }

    pub fn set_image(&self, image: skia_safe::Image) {
        self.view.update_state(BackgroundViewState {
            image: Some(image),
            ..self.view.get_state()
        });
    }
}

// static mut COUNTER: f32 = 1.0;
pub fn view_background(
    state: &BackgroundViewState,
    _view: &View<BackgroundViewState>,
) -> ViewLayer {
    let image = state.image.clone();

    // state.debug_string.clone();

    let draw_container = move |canvas: &skia_safe::Canvas, w, h| {
        let color = skia_safe::Color4f::new(1.0, 1.0, 1.0, 1.0);
        let mut paint = skia_safe::Paint::new(color, None);

        if let Some(image) = image.as_ref() {
            let mut matrix = skia_safe::Matrix::new_identity();
            matrix.set_scale((w / image.width() as f32, h / image.height() as f32), None);
            // canvas.concat(&matrix);
            // canvas.draw_image_rect(image, None, rect, &paint);
            paint.set_shader(image.to_shader(
                (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                skia_safe::SamplingOptions::default(),
                &matrix,
            ));
        }

        let split = 1;
        let rect_size_w = w / split as f32;
        let rect_size_h = h / split as f32;

        for i in 0..split {
            for j in 0..split {
                let rect = skia_safe::Rect::from_xywh(
                    i as f32 * rect_size_w,
                    j as f32 * rect_size_h,
                    rect_size_w,
                    rect_size_h,
                );
                canvas.draw_rect(rect, &paint);
            }
        }

        // let color = skia_safe::Color4f::new(0.0, 0.0, 0.0, 1.0);
        // let paint = skia_safe::Paint::new(color, None);
        // let mut font = skia_safe::Font::default();
        // let font_size = 26.0;
        // font.set_size(font_size);
        // canvas.draw_str("test string string", (80.0, 100.0), &font, &paint);
        // canvas.draw_rect(skia_safe::Rect::from_xywh(80.0, 100.0, 200.0, 100.0), &paint);

        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
    };

    ViewLayerBuilder::default()
        .key("background_view")
        .opacity((
            1.0,
            Some(Transition {
                delay: 0.5,
                duration: 1.0,
                timing: TimingFunction::Easing(Easing::ease_out()),
                // ..Default::default()
            }),
        ))
        .content(Some(draw_container))
        .build()
        .unwrap()
}

impl Observer<Workspace> for BackgroundView {
    fn notify(&self, _event: &Workspace) {}
}
