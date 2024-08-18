

use layers::{prelude::*, types::Size};
use smithay::utils::Transform;

use super::model::{WindowViewBaseModel, WindowViewSurface};



// struct FontCache {
//     font_collection: skia_safe::textlayout::FontCollection,
//     font_mgr: skia_safe::FontMgr,
//     type_face_font_provider: RefCell<skia_safe::textlayout::TypefaceFontProvider>,
// }

// // source: slint ui
// // https://github.com/slint-ui/slint/blob/64e7bb27d12dd8f884275292c2333d37f4e224d5/internal/renderers/skia/textlayout.rs#L31
// thread_local! {
//     static FONT_CACHE: FontCache = {
//         let font_mgr = skia_safe::FontMgr::new();
//         let type_face_font_provider = skia_safe::textlayout::TypefaceFontProvider::new();
//         let mut font_collection = skia_safe::textlayout::FontCollection::new();
//         font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
//         font_collection.set_dynamic_font_manager(font_mgr.clone());
//         FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
//     };
// }


#[profiling::function]
pub fn view_base_window(state: &WindowViewBaseModel, _view: &View<WindowViewBaseModel>) -> ViewLayer {
    let w = state.w;
    let h = state.h;

    println!("view_base_window render");
    
    const SAFE_AREA: f32 = 100.0;
    let draw_shadow = move |canvas: &skia_safe::Canvas, w: f32, h: f32| {
        println!("drop shadow render");

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
        shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, 3.0, false));
        canvas.draw_rect(rect, &shadow_paint);


        let rect= skia_safe::Rect::from_xywh(
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
    ViewLayerBuilder::default()
        .key("window_view")
        .size((
            Size {
                width: taffy::Dimension::Points(w),
                height: taffy::Dimension::Points(h),
            },
            None,
        ))
        .children(vec![
            ViewLayerBuilder::default()
                .key("window_view_shadow")
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
                        width: taffy::Dimension::Points(w + SAFE_AREA * 2.0),
                        height: taffy::Dimension::Points(h + SAFE_AREA * 2.0),
                    },
                    None,
                ))
                .content(Some(draw_shadow))
                .image_cache(true)
                .build()
                .unwrap()
        ])
        .build()
        .unwrap()
}

#[allow(clippy::ptr_arg)]
#[profiling::function]
pub fn view_content_window(render_elements: &Vec<WindowViewSurface>, _view: &View<Vec<WindowViewSurface>>) -> ViewLayer {
    // let w = state.w;
    // let h = state.h;

    // let render_elements = state.render_elements.clone();
    let resampler = skia_safe::CubicResampler::catmull_rom();

    
    ViewLayerBuilder::default()
        .key("window_view_content")
        .size((
            Size {
                width: taffy::Dimension::Points(0.0),
                height: taffy::Dimension::Points(0.0),
            },
            None,
        ))
        // .border_width((10.0, None))
        // .border_color((
        //     layers::types::Color::new_hex("34aeebff"),
        //     None,
        // ))
        .children(
            render_elements
                .iter()
                .enumerate()
                .filter(|(_, render_element)| render_element.w > 0.0 && render_element.h > 0.0)
                .map(|(index, render_element)| {
                    let wvs = render_element.clone();
                    let mut font = skia_safe::Font::default();
                    let font_size = 26.0;
                    font.set_size(font_size);
                    
                    let texture = wvs.texture.as_ref();
                    let image =  texture.map(|t| t.image.clone());
                    // let image = image.as_ref();
                    let mut damage = skia_safe::Rect::default();
                    let buffer_damages = texture.and_then(|t| t.damage.clone()).unwrap_or_default();
                    // if let Some(tex) = texture {
                    //     let image_id = tex.image.unique_id();
                    //     // println!("render dmabuf {} {:?}", image_id, damage);
                    // }
                    

                    buffer_damages.iter().for_each(|bd| {
                        let r = skia_safe::Rect::from_xywh(bd.loc.x as f32, bd.loc.y as f32, bd.size.w as f32, bd.size.h as f32);
                        damage.join(r);
                    });
                    let draw_container = move |canvas: &skia_safe::Canvas, w, h| {
                        if w == 0.0 || h == 0.0 {
                            return damage;
                        }
                        // let rect = skia_safe::Rect::from_xywh(0.0, 0.0, w, h);

                        if let Some(image) = image.as_ref() {
                            let scale = 1.0;//wvs.h / image.height() as f32;

                            let mut matrix = skia_safe::Matrix::new_identity();
                            match wvs.transform {
                                Transform::Normal => {
                                    // matrix.pre_translate(((-wvs.offset_x), (-wvs.offset_x)));
                                    matrix.pre_scale((scale, scale), None);
                                }
                                Transform::Flipped180 => {
                                    matrix.pre_scale((scale, -scale), None);
                                    // matrix.pre_translate((0.0, (-wvs.h)));
                                }
                                Transform::_90 => {}
                                Transform::_180 => {}
                                Transform::_270 => {}
                                Transform::Flipped => {}
                                Transform::Flipped90 => {}
                                Transform::Flipped270 => {}
                            }
                            let mut paint = skia_safe::Paint::new(
                                skia_safe::Color4f::new(1.0, 1.0, 1.0, 1.0),
                                None,
                            );
                            paint.set_shader(image.to_shader(
                                (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                                skia_safe::SamplingOptions::from(resampler),
                                // skia_safe::SamplingOptions::default(),
                                &matrix,
                            ));

                            let split = 1;
                            let rect_size_w = w / split as f32;
                            let rect_size_h = h / split as f32;

                            canvas.save();
                            // canvas.clip_rect(damage, None, None);
                            for i in 0..split {
                                for j in 0..split {
                                    let rect = skia_safe::Rect::from_xywh(i as f32 * rect_size_w, j as f32 * rect_size_h, rect_size_w, rect_size_h);
                                    // if rect.intersect(damage) {
                                        canvas.draw_rect(rect, &paint);
                                    // }
                                }
                            }
                            // canvas.restore();
                            // canvas.draw_rect(rect, &paint);
                            // canvas.concat(&matrix);
                            // canvas.draw_image(image, (0, 0), Some(&paint));                      
                        }

                        // let mut paint = skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 0.0, 0.0, 1.0), None);
                        // paint.set_stroke(true);
                        // canvas.draw_rrect(rrect, &paint);
                        damage
                    };
                    ViewLayerBuilder::default()
                        .key(format!("window_view_content_{}", index))
                        .layout_style(taffy::Style {
                            position: taffy::Position::Absolute,
                            ..Default::default()
                        })
                        .position((
                            Point {
                                x: wvs.x + wvs.offset_x,
                                y: wvs.y + wvs.offset_y,
                            },
                            None,
                        ))
                        .size((
                            Size {
                                width: taffy::Dimension::Points(wvs.w),
                                height: taffy::Dimension::Points(wvs.h),
                            },
                            None,
                        ))
                        .content(Some(draw_container))
                        .build()
                        .unwrap()
                })
                .collect::<Vec<_>>(),
        )
        .build()
        .unwrap()
}
