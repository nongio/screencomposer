use layers::types::{Color, PaintColor};
use tracing::trace;

use super::Backend;

impl<BackendData: Backend> super::ScreenComposer<BackendData> {
    pub fn init_scene(&self, width: i32, height: i32) {
        let root_layer = self.engine.new_layer();

        root_layer.set_size(
            layers::types::Size {
                x: width as f32,
                y: height as f32,
            },
            None,
        );
        root_layer.set_scale((1.0, 1.0), None);
        root_layer.set_position(layers::types::Point { x: 0.0, y: 0.0 }, None);

        trace!("created root_layer {:?}", root_layer.id());

        root_layer.set_background_color(
            PaintColor::Solid {
                color: Color::new_rgba255(180, 180, 180, 255),
            },
            None,
        );
        root_layer.set_border_corner_radius(10.0, None);

        self.engine.scene_add_layer(root_layer);
    }
}
