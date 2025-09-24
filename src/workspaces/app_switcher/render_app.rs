use lay_rs::{
    prelude::{taffy, Color, Layer, LayerTree, LayerTreeBuilder, View},
    skia,
    types::{BorderRadius, PaintColor, Size},
};

use crate::workspaces::Application;

use super::model::AppSwitcherModel;

pub fn render_app_view(
    index: usize,
    state: Application,
    view: View<AppSwitcherModel>,
    icon_width: f32,
    padding: f32,
) -> LayerTree {
    let draw_picture = move |canvas: &skia::Canvas, w: f32, h: f32| -> skia::Rect {
        if let Some(image) = &state.icon {
            let mut paint = skia::Paint::new(skia::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            // paint.set_anti_alias(true);
            paint.set_style(skia::paint::Style::Fill);

            // draw image with shadow
            let shadow_color = skia::Color4f::new(0.0, 0.0, 0.0, 0.5);
            let mut shadow_paint = skia::Paint::new(shadow_color, None);
            let shadow_offset = skia::Vector::new(5.0, 5.0);
            let shadow_color = skia::Color::from_argb(128, 0, 0, 0); // semi-transparent black
            let shadow_blur_radius = 5.0;

            let shadow_filter = skia::image_filters::drop_shadow_only(
                (shadow_offset.x, shadow_offset.y),
                (shadow_blur_radius, shadow_blur_radius),
                shadow_color,
                None,
                None,
                skia::image_filters::CropRect::default(),
            );
            shadow_paint.set_image_filter(shadow_filter);
            let icon_size = (w - padding * 2.0).max(0.0);
            canvas.draw_image_rect(
                image,
                None,
                skia::Rect::from_xywh(padding, padding, icon_size, icon_size),
                &shadow_paint,
            );
            let resampler = skia::CubicResampler::catmull_rom();
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                skia::Rect::from_xywh(padding, padding, icon_size, icon_size),
                skia::SamplingOptions::from(resampler),
                &paint,
            );
        }
        skia::Rect::from_xywh(0.0, 0.0, w, h)
    };
    LayerTreeBuilder::default()
        .key(format!("app_{}", state.identifier))
        .size((
            Size {
                width: taffy::Dimension::Length(icon_width + padding * 2.0),
                height: taffy::Dimension::Length(icon_width + padding * 2.0),
            },
            None,
        ))
        .background_color((
            PaintColor::Solid {
                color: Color::new_rgba(1.0, 0.0, 0.0, 0.0),
            },
            None,
        ))
        .border_corner_radius((BorderRadius::new_single(20.0), None))
        .content(Some(draw_picture))
        .on_pointer_in(move |_: &Layer, _x, _y| {
            view.update_state(&AppSwitcherModel {
                current_app: index,
                ..view.get_state()
            });
        })
        .build()
        .unwrap()
}
