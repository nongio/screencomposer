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
    LayerTreeBuilder::default()
        .key("window_content")
        .size((
            Size {
                width: taffy::Dimension::Length(0.0),
                height: taffy::Dimension::Length(0.0),
            },
            None,
        ))
        .pointer_events(false)
        .children(
            render_elements
                .iter()
                .filter_map(|render_element| {
                    if render_element.phy_dst_w <= 0.0 || render_element.phy_dst_h <= 0.0 {
                        return None;
                    }
                    Some(render_element)
                })
                .enumerate()
                .map(|(index, wvs)| {
                    let draw_wvs = wvs.clone();

                    let draw_container = move |canvas: &lay_rs::skia::Canvas, w: f32, h: f32| {
                        if w == 0.0 || h == 0.0 {
                            return lay_rs::skia::Rect::default();
                        }
                        let tex = crate::textures_storage::get(&draw_wvs.id);
                        if tex.is_none() {
                            return lay_rs::skia::Rect::default();
                        }
                        let tex = tex.unwrap();
                        let mut damage = lay_rs::skia::Rect::default();
                        if let Some(tex_damage) = tex.damage {
                            tex_damage.iter().for_each(|bd| {
                                let r = lay_rs::skia::Rect::from_xywh(
                                    bd.loc.x as f32,
                                    bd.loc.y as f32,
                                    bd.size.w as f32,
                                    bd.size.h as f32,
                                );
                                damage.join(r);
                            });
                        }

                        let src_h = (draw_wvs.phy_src_h - draw_wvs.phy_src_y).max(1.0);
                        let src_w = (draw_wvs.phy_src_w - draw_wvs.phy_src_x).max(1.0);
                        let scale_y = draw_wvs.phy_dst_h / src_h;
                        let scale_x = draw_wvs.phy_dst_w / src_w;
                        let mut matrix = lay_rs::skia::Matrix::new_identity();
                        match draw_wvs.transform {
                            Transform::Normal => {
                                matrix.pre_translate((-draw_wvs.phy_src_x, -draw_wvs.phy_src_y));
                                matrix.pre_scale((scale_x, scale_y), None);
                            }
                            Transform::Flipped180 => {
                                matrix.pre_translate((draw_wvs.phy_src_x, draw_wvs.phy_src_y));
                                matrix.pre_scale((scale_x, -scale_y), None);
                            }
                            Transform::_90 => {}
                            Transform::_180 => {}
                            Transform::_270 => {}
                            Transform::Flipped => {}
                            Transform::Flipped90 => {}
                            Transform::Flipped270 => {}
                        }

                        let sampling = lay_rs::skia::SamplingOptions::from(
                            lay_rs::skia::CubicResampler::catmull_rom(),
                        );
                        let mut paint = lay_rs::skia::Paint::new(
                            lay_rs::skia::Color4f::new(1.0, 1.0, 1.0, 1.0),
                            None,
                        );
                        paint.set_shader(tex.image.to_shader(
                            (lay_rs::skia::TileMode::Clamp, lay_rs::skia::TileMode::Clamp),
                            sampling,
                            &matrix,
                        ));

                        let rect = lay_rs::skia::Rect::from_xywh(0.0, 0.0, w, h);
                        canvas.draw_rect(rect, &paint);
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
                        .size((
                            Size {
                                width: taffy::Dimension::Length(wvs.phy_dst_w),
                                height: taffy::Dimension::Length(wvs.phy_dst_h),
                            },
                            None,
                        ))
                        .content(Some(draw_container))
                        .pointer_events(false)
                        .picture_cached(false)
                        .build()
                        .unwrap()
                })
                .collect::<Vec<_>>(),
        )
        .build()
        .unwrap()
}
