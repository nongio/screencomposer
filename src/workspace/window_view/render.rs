use layers::{prelude::*, types::Size};

use super::model::WindowViewBaseModel;

#[profiling::function]
pub fn view_window_shadow(
    state: &WindowViewBaseModel,
    _view: &View<WindowViewBaseModel>,
) -> LayerTree {
    let w = state.w;
    let h = state.h;

    const SAFE_AREA: f32 = 100.0;
    let draw_shadow = move |canvas: &skia_safe::Canvas, w: f32, h: f32| {
        // draw shadow
        // let window_corner_radius = 12.0;
        let rect = skia_safe::Rect::from_xywh(
            SAFE_AREA,
            SAFE_AREA,
            w - SAFE_AREA * 2.0,
            h - SAFE_AREA * 2.0,
        );

        canvas.clip_rect(rect, skia_safe::ClipOp::Difference, false);
        // let rrect = skia_safe::RRect::new_rect_xy(
        //     rect,
        //     window_corner_radius,
        //     window_corner_radius,
        // );

        let mut shadow_paint =
            skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.25), None);
        shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(
            skia_safe::BlurStyle::Normal,
            3.0,
            false,
        ));
        canvas.draw_rect(rect, &shadow_paint);

        let rect = skia_safe::Rect::from_xywh(
            SAFE_AREA,
            SAFE_AREA + 36.0,
            w - SAFE_AREA * 2.0,
            h - SAFE_AREA * 2.0,
        );

        shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(
            skia_safe::BlurStyle::Normal,
            30.0,
            false,
        ));
        shadow_paint.set_color4f(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.7), None);

        canvas.draw_rect(rect, &shadow_paint);
        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
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
            .image_cache(true)
            .pointer_events(false)
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
