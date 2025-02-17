use lay_rs::{prelude::*, types::Size};

use crate::config::Config;

use super::model::WindowViewBaseModel;

#[profiling::function]
pub fn view_window_shadow(
    state: &WindowViewBaseModel,
    _view: &View<WindowViewBaseModel>,
) -> LayerTree {
    let w = state.w;
    let h = state.h;
    const SAFE_AREA: f32 = 100.0;
    let draw_scale = Config::with(|config| config.screen_scale) as f32;
    let draw_shadow = move |canvas: &lay_rs::skia::Canvas, w: f32, h: f32| {
        // draw shadow
        let window_corner_radius = 24.0 * draw_scale;
        let rect = lay_rs::skia::Rect::from_xywh(
            SAFE_AREA,
            SAFE_AREA,
            w - SAFE_AREA * 2.0,
            h - SAFE_AREA * 2.0,
        );

        let rrect =
            lay_rs::skia::RRect::new_rect_xy(rect, window_corner_radius, window_corner_radius);
        canvas.clip_rrect(rrect, lay_rs::skia::ClipOp::Difference, false);
        // let canvas_mat= canvas.local_to_device_as_3x3();
        // let scale = (canvas_mat.scale_x(), canvas_mat.scale_y());
        let mut shadow_paint =
            lay_rs::skia::Paint::new(lay_rs::skia::Color4f::new(0.0, 0.0, 0.0, 0.25), None);
        shadow_paint.set_mask_filter(lay_rs::skia::MaskFilter::blur(
            lay_rs::skia::BlurStyle::Normal,
            3.0,
            false,
        ));
        canvas.draw_rrect(rrect, &shadow_paint);

        let rect = lay_rs::skia::Rect::from_xywh(
            SAFE_AREA,
            SAFE_AREA + 20.0 * draw_scale,
            w - SAFE_AREA * 2.0,
            h - SAFE_AREA * 2.0,
        );
        let rrect =
            lay_rs::skia::RRect::new_rect_xy(rect, window_corner_radius, window_corner_radius);
        shadow_paint.set_mask_filter(lay_rs::skia::MaskFilter::blur(
            lay_rs::skia::BlurStyle::Normal,
            30.0,
            false,
        ));
        shadow_paint.set_color4f(lay_rs::skia::Color4f::new(0.0, 0.0, 0.0, 0.5), None);

        canvas.draw_rrect(rrect, &shadow_paint);
        lay_rs::skia::Rect::from_xywh(0.0, 0.0, w, h)
    };
    LayerTreeBuilder::default()
        .key("window_shadow")
        .size((
            Size {
                width: taffy::Dimension::Length(w),
                height: taffy::Dimension::Length(h),
            },
            None,
        ))
        .pointer_events(false)
        .image_cache(true)
        .children(vec![LayerTreeBuilder::default()
            .key("window_shadow_inner")
            .layout_style(taffy::Style {
                position: taffy::Position::Absolute,
                ..Default::default()
            })
            .position((
                Point {
                    x: -SAFE_AREA,
                    y: -SAFE_AREA,
                },
                None,
            ))
            .size((
                Size {
                    width: taffy::Dimension::Length(w + SAFE_AREA * 2.0),
                    height: taffy::Dimension::Length(h + SAFE_AREA * 2.0),
                },
                None,
            ))
            .content(Some(draw_shadow))
            .pointer_events(false)
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
