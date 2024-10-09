use std::cell::RefCell;

use layers::{
    prelude::{taffy, LayerTree, LayerTreeBuilder, View},
    types::{Point, Size},
};
use smithay::utils::Transform;

use super::WindowViewSurface;


pub struct FontCache {
    pub font_collection: layers::skia::textlayout::FontCollection,
    pub font_mgr: layers::skia::FontMgr,
    pub type_face_font_provider: RefCell<layers::skia::textlayout::TypefaceFontProvider>,
}


thread_local! {
    pub static FONT_CACHE: FontCache = {
        let font_mgr = layers::skia::FontMgr::new();
        let type_face_font_provider = layers::skia::textlayout::TypefaceFontProvider::new();
        let mut font_collection = layers::skia::textlayout::FontCollection::new();
        font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
        font_collection.set_dynamic_font_manager(font_mgr.clone());
        FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
    };
}

#[allow(clippy::ptr_arg)]
#[profiling::function]
pub fn view_render_elements(
    render_elements: &Vec<WindowViewSurface>,
    _view: &View<Vec<WindowViewSurface>>,
) -> LayerTree {
    let resampler = layers::skia::CubicResampler::catmull_rom();

    LayerTreeBuilder::default()
        .key("window_content")
        .size((
            Size {
                width: taffy::Dimension::Length(0.0),
                height: taffy::Dimension::Length(0.0),
            },
            None,
        ))
        .children(
            render_elements
                .iter()
                .enumerate()
                .filter(|(_, render_element)| {
                    render_element.phy_dst_w > 0.0 && render_element.phy_dst_h > 0.0
                })
                .map(|(index, render_element)| {
                    let wvs = render_element.clone();
                    let mut font = layers::skia::Font::default();
                    let font_size = 26.0;
                    font.set_size(font_size);

                    let texture = wvs.texture.as_ref();
                    let image = texture.map(|t| t.image.clone());
                    // let image = image.as_ref();
                    let mut damage = layers::skia::Rect::default();
                    let buffer_damages = texture.and_then(|t| t.damage.clone()).unwrap_or_default();

                    buffer_damages.iter().for_each(|bd| {
                        let r = layers::skia::Rect::from_xywh(
                            bd.loc.x as f32,
                            bd.loc.y as f32,
                            bd.size.w as f32,
                            bd.size.h as f32,
                        );
                        damage.join(r);
                    });
                    let draw_container = move |canvas: &layers::skia::Canvas, w, h| {
                        if w == 0.0 || h == 0.0 {
                            return damage;
                        }
                        // let rect = layers::skia::Rect::from_xywh(0.0, 0.0, w, h);

                        if let Some(image) = image.as_ref() {
                            // let image_h = image.height() as f32;
                            // let image_w = image.width() as f32;
                            let src_h = wvs.phy_src_h - wvs.phy_src_y;
                            let src_w = wvs.phy_src_w - wvs.phy_src_x;
                            let scale_y = wvs.phy_dst_h / src_h;
                            let scale_x = wvs.phy_dst_w / src_w;
                            let mut matrix = layers::skia::Matrix::new_identity();
                            // if scale_x != 1.0 || scale_y != 1.0 {
                            match wvs.transform {
                                Transform::Normal => {
                                    // println!("Normal");
                                    matrix.pre_translate((-wvs.phy_src_x, -wvs.phy_src_y));
                                    matrix.pre_scale((scale_x, scale_y), None);
                                }
                                Transform::Flipped180 => {
                                    // println!("Flipped180");
                                    matrix.pre_translate((wvs.phy_src_x, wvs.phy_src_y));
                                    matrix.pre_scale((scale_x, -scale_y), None);
                                }
                                Transform::_90 => {}
                                Transform::_180 => {}
                                Transform::_270 => {}
                                Transform::Flipped => {}
                                Transform::Flipped90 => {}
                                Transform::Flipped270 => {}
                            }
                            // }
                            // println!("texture size ({}x{}) scale: {} from:[{}, {} - {}x{}] to:[{}, {} - {}x{}] -> scale: {}x{}", image.width(), image.height(), wvs.scale, wvs.src_x, wvs.src_y, wvs.src_w, wvs.src_h, wvs.offset_x, wvs.offset_x, wvs.dst_w, wvs.dst_h, scale_x, scale_y);
                            // println!("Matrix {:?}", matrix);
                            let mut paint = layers::skia::Paint::new(
                                layers::skia::Color4f::new(1.0, 1.0, 1.0, 1.0),
                                None,
                            );
                            paint.set_shader(image.to_shader(
                                (layers::skia::TileMode::Clamp, layers::skia::TileMode::Clamp),
                                layers::skia::SamplingOptions::from(resampler),
                                // layers::skia::SamplingOptions::default(),
                                &matrix,
                            ));

                            let split = 1;
                            let rect_size_w = wvs.phy_dst_w / split as f32;
                            let rect_size_h = wvs.phy_dst_h / split as f32;

                            canvas.save();
                            // canvas.clip_rect(damage, None, None);
                            for i in 0..split {
                                for j in 0..split {
                                    let rect = layers::skia::Rect::from_xywh(
                                        i as f32 * rect_size_w,
                                        j as f32 * rect_size_h,
                                        rect_size_w,
                                        rect_size_h,
                                    );
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

                        // let mut paint = layers::skia::Paint::new(layers::skia::Color4f::new(1.0, 0.0, 0.0, 1.0), None);
                        // paint.set_stroke(true);
                        // canvas.draw_rrect(rrect, &paint);
                        damage
                    };
                    LayerTreeBuilder::default()
                        .key(format!("window_content_{}", index))
                        .layout_style(taffy::Style {
                            position: taffy::Position::Absolute,
                            ..Default::default()
                        })
                        .position((
                            Point {
                                x: wvs.phy_dst_x + wvs.log_offset_x,
                                y: wvs.phy_dst_y + wvs.log_offset_y,
                            },
                            None,
                        ))
                        // .border_width((1.0, None))
                        // .border_color((
                        //     Color::new_rgba(1.0, 0.0, 0.0, 1.0),
                        //     None,
                        // ))
                        .size((
                            Size {
                                width: taffy::Dimension::Length(wvs.phy_dst_w),
                                height: taffy::Dimension::Length(wvs.phy_dst_h),
                            },
                            None,
                        ))
                        .content(Some(draw_container))
                        .pointer_events(false)
                        .build()
                        .unwrap()
                })
                .collect::<Vec<_>>(),
        )
        .build()
        .unwrap()
}
