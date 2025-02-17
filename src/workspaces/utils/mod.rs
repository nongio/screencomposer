use std::cell::RefCell;

use lay_rs::{
    prelude::{taffy, LayerTree, LayerTreeBuilder, View},
    types::{Point, Size},
};
use smithay::utils::Transform;

use super::WindowViewSurface;

#[allow(unused)]
pub struct FontCache {
    pub font_collection: lay_rs::skia::textlayout::FontCollection,
    pub font_mgr: lay_rs::skia::FontMgr,
    pub type_face_font_provider: RefCell<lay_rs::skia::textlayout::TypefaceFontProvider>,
}

thread_local! {
    pub static FONT_CACHE: FontCache = {
        let font_mgr = lay_rs::skia::FontMgr::new();
        let type_face_font_provider = lay_rs::skia::textlayout::TypefaceFontProvider::new();
        let mut font_collection = lay_rs::skia::textlayout::FontCollection::new();
        font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
        font_collection.set_dynamic_font_manager(font_mgr.clone());
        FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
    };
}

#[allow(clippy::too_many_arguments)]
pub fn draw_balloon_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
    arrow_width: f32,
    arrow_height: f32,
    arrow_position: f32, // Position of the arrow along the bottom edge (0.0 to 1.0)
    arrow_corner_radius: f32,
) -> lay_rs::skia::Path {
    let mut path = lay_rs::skia::Path::new();

    // Calculate the arrow tip position
    let arrow_tip_x = x + arrow_position * width;
    let arrow_base_left_x = arrow_tip_x - arrow_width / 2.0;
    let arrow_base_right_x = arrow_tip_x + arrow_width / 2.0;

    // Move to the starting point (top-left corner)
    path.move_to((x + corner_radius, y));

    // Top edge
    path.line_to((x + width - corner_radius, y));
    path.arc_to_tangent(
        (x + width, y),
        (x + width, y + corner_radius),
        corner_radius,
    );

    // Right edge
    path.line_to((x + width, y + height - corner_radius - arrow_height));
    path.arc_to_tangent(
        (x + width, y + height - arrow_height),
        (x + width - corner_radius, y + height - arrow_height),
        corner_radius,
    );

    // Arrow with rounded corners
    path.line_to((
        arrow_base_right_x, //- arrow_corner_radius,
        y + height - arrow_height,
    ));
    path.arc_to_tangent(
        (arrow_base_right_x, y + height - arrow_height),
        (arrow_tip_x, y + height),
        arrow_corner_radius,
    );
    path.arc_to_tangent(
        (arrow_tip_x, y + height),
        (arrow_base_left_x, y + height - arrow_height),
        arrow_corner_radius,
    );
    path.arc_to_tangent(
        (arrow_base_left_x, y + height - arrow_height),
        (x + corner_radius, y + height - arrow_height),
        arrow_corner_radius,
    );

    // Bottom edge
    path.line_to((x + corner_radius, y + height - arrow_height));
    path.arc_to_tangent(
        (x, y + height - arrow_height),
        (x, y + height - corner_radius - arrow_height),
        corner_radius,
    );

    // Left edge
    path.line_to((x, y + corner_radius));
    path.arc_to_tangent((x, y), (x + corner_radius, y), corner_radius);

    // Close the path
    path.close();
    path
}

#[allow(clippy::ptr_arg)]
#[profiling::function]
pub fn view_render_elements(
    render_elements: &Vec<WindowViewSurface>,
    _view: &View<Vec<WindowViewSurface>>,
) -> LayerTree {
    let resampler = lay_rs::skia::CubicResampler::catmull_rom();

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
                    let mut font = lay_rs::skia::Font::default();
                    let font_size = 26.0;
                    font.set_size(font_size);

                    let texture = wvs.texture.as_ref();
                    let image = texture.map(|t| t.image.clone());
                    // let image = image.as_ref();
                    let mut damage = lay_rs::skia::Rect::default();
                    let buffer_damages = texture.and_then(|t| t.damage.clone()).unwrap_or_default();

                    buffer_damages.iter().for_each(|bd| {
                        let r = lay_rs::skia::Rect::from_xywh(
                            bd.loc.x as f32,
                            bd.loc.y as f32,
                            bd.size.w as f32,
                            bd.size.h as f32,
                        );
                        damage.join(r);
                    });
                    let draw_container = move |canvas: &lay_rs::skia::Canvas, w, h| {
                        if w == 0.0 || h == 0.0 {
                            return damage;
                        }
                        // let rect = lay_rs::skia::Rect::from_xywh(0.0, 0.0, w, h);

                        if let Some(image) = image.as_ref() {
                            // let image_h = image.height() as f32;
                            // let image_w = image.width() as f32;
                            let src_h = wvs.phy_src_h - wvs.phy_src_y;
                            let src_w = wvs.phy_src_w - wvs.phy_src_x;
                            let scale_y = wvs.phy_dst_h / src_h;
                            let scale_x = wvs.phy_dst_w / src_w;
                            let mut matrix = lay_rs::skia::Matrix::new_identity();
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
                            let mut paint = lay_rs::skia::Paint::new(
                                lay_rs::skia::Color4f::new(1.0, 1.0, 1.0, 1.0),
                                None,
                            );
                            paint.set_shader(image.to_shader(
                                (lay_rs::skia::TileMode::Clamp, lay_rs::skia::TileMode::Clamp),
                                lay_rs::skia::SamplingOptions::from(resampler),
                                // lay_rs::skia::SamplingOptions::default(),
                                &matrix,
                            ));

                            let split = 1;
                            let rect_size_w = wvs.phy_dst_w / split as f32;
                            let rect_size_h = wvs.phy_dst_h / split as f32;

                            canvas.save();
                            // canvas.clip_rect(damage, None, None);
                            for i in 0..split {
                                for j in 0..split {
                                    let rect = lay_rs::skia::Rect::from_xywh(
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

                        // let mut paint = lay_rs::skia::Paint::new(lay_rs::skia::Color4f::new(1.0, 0.0, 0.0, 1.0), None);
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
