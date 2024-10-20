use layers::prelude::*;
use layers::skia::PathEffect;

use crate::workspace::Application;

pub fn draw_app_icon(application: &Application, pressed: bool) -> ContentDrawFunction {
    let mut darken_factor = 255;
    if pressed {
        darken_factor = 150;
    }
    let application = application.clone();
    let draw_picture = move |canvas: &layers::skia::Canvas, w: f32, h: f32| -> layers::skia::Rect {
        let icon_size = (w).max(0.0);
        if let Some(image) = &application.icon.clone() {
            let mut paint =
            layers::skia::Paint::new(layers::skia::Color4f::new(1.0, 1.0, 1.0, 1.0), None);

            paint.set_style(layers::skia::paint::Style::Fill);
            let color = layers::skia::Color::from_argb(255, darken_factor, darken_factor, darken_factor);
            let darken_filter = layers::skia::color_filters::blend(color, layers::skia::BlendMode::Modulate);

            paint.set_color_filter(darken_filter);
            // draw image with shadow
            let shadow_color = layers::skia::Color4f::new(0.0, 0.0, 0.0, 0.5);

            let mut shadow_paint = layers::skia::Paint::new(shadow_color, None);
            let shadow_offset = layers::skia::Vector::new(5.0, 5.0);
            let shadow_color = layers::skia::Color::from_argb(128, 0, 0, 0); // semi-transparent black
            let shadow_blur_radius = 5.0;

            let shadow_filter = layers::skia::image_filters::drop_shadow_only(
                (shadow_offset.x, shadow_offset.y),
                (shadow_blur_radius, shadow_blur_radius),
                shadow_color,
                None,
                None,
                layers::skia::image_filters::CropRect::default(),
            );
            shadow_paint.set_image_filter(shadow_filter);

            canvas.draw_image_rect(
                image,
                None,
                layers::skia::Rect::from_xywh(0.0, 0.0, icon_size, icon_size),
                &shadow_paint,
            );
            let resampler = layers::skia::CubicResampler::catmull_rom();
            
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                layers::skia::Rect::from_xywh(0.0, 0.0, icon_size, icon_size),
                layers::skia::SamplingOptions::from(resampler),
                &paint,
            );
        } else {
            let mut rect = layers::skia::Rect::from_xywh(0.0, 0.0, icon_size, icon_size);
            rect.inset((10.0, 10.0));
            let rrect = layers::skia::RRect::new_rect_xy(rect, 10.0, 10.0);
            let mut paint =
                layers::skia::Paint::new(layers::skia::Color4f::new(1.0, 1.0, 1.0, 0.2), None);
            canvas.draw_rrect(rrect, &paint);

            paint.set_stroke(true);
            paint.set_stroke_width(6.0);
            paint.set_color4f(layers::skia::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            let intervals = [12.0, 6.0]; // Length of the dash and the gap
            let path_effect = PathEffect::dash(&intervals, 0.0);
            paint.set_path_effect(path_effect);
            canvas.draw_rrect(rrect, &paint);
        }
        let mut paint = layers::skia::Paint::new(layers::skia::Color4f::new(0.0, 0.0, 0.0, 0.5), None);
        paint.set_anti_alias(true);
        paint.set_style(layers::skia::paint::Style::Fill);
        let circle_radius = 6.0;
        canvas.draw_circle((w / 2.0, h - (10.0 + circle_radius)), circle_radius, &paint);

        layers::skia::Rect::from_xywh(0.0, 0.0, w, h)
    };

    return draw_picture.into();
}