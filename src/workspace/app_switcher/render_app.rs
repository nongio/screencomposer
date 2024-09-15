use layers::{
    prelude::{taffy, Color, LayerTree, LayerTreeBuilder, View},
    types::{BorderRadius, PaintColor, Size},
};

use crate::workspace::Application;

use super::model::AppSwitcherModel;

pub fn render_app_view(
    index: usize,
    state: Application,
    view: View<AppSwitcherModel>,
    icon_width: f32,
    padding: f32,
) -> LayerTree {
    let draw_picture = move |canvas: &skia_safe::Canvas, w: f32, h: f32| -> skia_safe::Rect {
        if let Some(image) = &state.icon {
            let mut paint =
                skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            // paint.set_anti_alias(true);
            paint.set_style(skia_safe::paint::Style::Fill);

            // draw image with shadow
            let shadow_color = skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.5);
            let mut shadow_paint = skia_safe::Paint::new(shadow_color, None);
            let shadow_offset = skia_safe::Vector::new(5.0, 5.0);
            let shadow_color = skia_safe::Color::from_argb(128, 0, 0, 0); // semi-transparent black
            let shadow_blur_radius = 5.0;

            let shadow_filter = skia_safe::image_filters::drop_shadow_only(
                (shadow_offset.x, shadow_offset.y),
                (shadow_blur_radius, shadow_blur_radius),
                shadow_color,
                None,
                skia_safe::image_filters::CropRect::default(),
            );
            shadow_paint.set_image_filter(shadow_filter);
            let icon_size = (w - padding * 2.0).max(0.0);
            canvas.draw_image_rect(
                image,
                None,
                skia_safe::Rect::from_xywh(padding, padding, icon_size, icon_size),
                &shadow_paint,
            );
            let resampler = skia_safe::CubicResampler::catmull_rom();
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                skia_safe::Rect::from_xywh(padding, padding, icon_size, icon_size),
                skia_safe::SamplingOptions::from(resampler),
                &paint,
            );
        }
        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
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
        .on_pointer_in(move |_, _x, _y| {
            view.update_state(&AppSwitcherModel {
                current_app: index,
                ..view.get_state()
            });
        })
        .build()
        .unwrap()
}
