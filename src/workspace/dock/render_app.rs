use std::{cell::RefCell, hash::Hash};

use layers::{
    prelude::*,
    taffy::LengthPercentageAuto,
    types::{PaintColor, Size},
    view::RenderLayerTree,
};
use skia_safe::PathEffect;

use crate::workspace::Application;

#[allow(dead_code)]
struct FontCache {
    font_collection: skia_safe::textlayout::FontCollection,
    pub font_mgr: skia_safe::FontMgr,
    type_face_font_provider: RefCell<skia_safe::textlayout::TypefaceFontProvider>,
}

// source: slint ui
// https://github.com/slint-ui/slint/blob/64e7bb27d12dd8f884275292c2333d37f4e224d5/internal/renderers/skia/textlayout.rs#L31
thread_local! {
    static FONT_CACHE: FontCache = {
        let font_mgr = skia_safe::FontMgr::new();
        let type_face_font_provider = skia_safe::textlayout::TypefaceFontProvider::new();
        let mut font_collection = skia_safe::textlayout::FontCollection::new();
        font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
        font_collection.set_dynamic_font_manager(font_mgr.clone());
        FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
    };
}

#[derive(Clone)]
pub struct DockAppState {
    pub index: usize,
    pub application: Application,
    pub icon_width: f32,
}

impl Hash for DockAppState {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.application.hash(state);
        self.icon_width.to_bits().hash(state);
    }
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
) -> skia_safe::Path {
    let mut path = skia_safe::Path::new();

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

pub fn render_app_view(state: &DockAppState, view: &View<DockAppState>) -> LayerTree {
    let application = state.application.clone();
    // let index = state.index;
    let icon_width = state.icon_width;
    let show_label: bool = view.get_internal_state("show_label").unwrap_or(false);
    let pressed: bool = view.get_internal_state("pressed").unwrap_or(false);
    let app_name = state
        .application
        .desktop_name
        .clone()
        .unwrap_or(state.application.identifier.clone());
    let key = view.key();
    let label_opacity: f32 = if show_label { 1.0 } else { 0.0 };

    let text_size = 26.0;
    let typeface = FONT_CACHE
        .with(|font_cache| {
            font_cache
                .font_mgr
                .match_family_style("Inter", skia_safe::FontStyle::default())
        })
        .unwrap();
    let font = skia_safe::Font::from_typeface_with_params(typeface, text_size, 1.0, 0.0);
    let mut darken_factor = 255;
    if pressed {
        darken_factor = 150;
    }
    let draw_picture = move |canvas: &skia_safe::Canvas, w: f32, h: f32| -> skia_safe::Rect {
        let icon_size = (w).max(0.0);
        if let Some(image) = &application.icon.clone() {
            let mut paint =
            skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 1.0, 1.0, 1.0), None);

            paint.set_style(skia_safe::paint::Style::Fill);
            let color = skia_safe::Color::from_argb(255, darken_factor, darken_factor, darken_factor);
            let darken_filter = skia_safe::color_filters::blend(color, skia_safe::BlendMode::Modulate);

            paint.set_color_filter(darken_filter);
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

            canvas.draw_image_rect(
                image,
                None,
                skia_safe::Rect::from_xywh(0.0, 0.0, icon_size, icon_size),
                &shadow_paint,
            );
            let resampler = skia_safe::CubicResampler::catmull_rom();
            
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                skia_safe::Rect::from_xywh(0.0, 0.0, icon_size, icon_size),
                skia_safe::SamplingOptions::from(resampler),
                &paint,
            );
        } else {
            let mut rect = skia_safe::Rect::from_xywh(0.0, 0.0, icon_size, icon_size);
            rect.inset((10.0, 10.0));
            let rrect = skia_safe::RRect::new_rect_xy(rect, 10.0, 10.0);
            let mut paint =
                skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 1.0, 1.0, 0.2), None);
            canvas.draw_rrect(rrect, &paint);

            paint.set_stroke(true);
            paint.set_stroke_width(6.0);
            paint.set_color4f(skia_safe::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            let intervals = [12.0, 6.0]; // Length of the dash and the gap
            let path_effect = PathEffect::dash(&intervals, 0.0);
            paint.set_path_effect(path_effect);
            canvas.draw_rrect(rrect, &paint);
        }
        let mut paint = skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.5), None);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Fill);
        let circle_radius = 6.0;
        canvas.draw_circle((w / 2.0, h - (10.0 + circle_radius)), circle_radius, &paint);

        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
    };

    let text = app_name.clone();

    let paint = skia_safe::Paint::default();
    let text_bounds = font.measure_str(text, Some(&paint));

    let text_bounds = text_bounds.1;
    let arrow_height = 20.0;
    let text_padding_h = 30.0;
    let text_padding_v = 14.0;
    let safe_margin = 100.0;
    let draw_label = move |canvas: &skia_safe::Canvas, w: f32, h: f32| -> skia_safe::Rect {
        // Tooltip parameters
        // let text = "This is a tooltip!";
        let text = app_name.clone();
        let rect_corner_radius = 10.0;
        let arrow_width = 25.0;
        let arrow_corner_radius = 3.0;

        // Paint for the tooltip background
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(230, 255, 255, 255)); // Light gray
        paint.set_anti_alias(true);

        // Calculate tooltip dimensions
        let tooltip_width = w - safe_margin * 2.0;
        let tooltip_height = h - safe_margin * 2.0;

        let arrow_path = draw_balloon_rect(
            safe_margin,
            safe_margin,
            tooltip_width,
            tooltip_height,
            rect_corner_radius,
            arrow_width,
            arrow_height,
            0.5,
            arrow_corner_radius,
        );
        let shadow_color = skia_safe::Color::from_argb(80, 0, 0, 0); // semi-transparent black
        let mut shadow_paint = skia_safe::Paint::default();
        shadow_paint.set_color(shadow_color);
        shadow_paint.set_anti_alias(true);
        shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(
            skia_safe::BlurStyle::Normal,
            10.0,
            None,
        ));

        let mut shadow_path = arrow_path.clone();
        shadow_path.offset((-0.0, -0.0));
        canvas.draw_path(&shadow_path, &shadow_paint);

        // // Draw the arrow path (under the rectangle)
        canvas.draw_path(&arrow_path, &paint);

        // // Paint for the text
        let mut text_paint = skia_safe::Paint::default();
        text_paint.set_color(skia_safe::Color::BLACK);
        text_paint.set_anti_alias(true);

        // // Draw the text inside the tooltip
        let text_x = safe_margin + text_padding_h;
        let text_y = text_bounds.height() + text_padding_v + safe_margin - text_size * 0.2;
        canvas.draw_str(text.as_str(), (text_x, text_y), &font, &text_paint);
        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
    };

    let view_ref = view.clone();
    let view_ref2 = view.clone();
    let view_ref3 = view.clone();
    let view_ref4 = view.clone();

    let label_size_width = text_bounds.width() + text_padding_h * 2.0 + safe_margin * 2.0;
    let label_size_height =
        text_bounds.height() + arrow_height + text_padding_v * 2.0 + safe_margin * 2.0;

    LayerTreeBuilder::default()
        .key(key.clone())
        .layout_style(taffy::Style {
            display: taffy::Display::Block,
            position: taffy::Position::Relative,
            // align_content: Some(taffy::AlignContent::Center),
            // justify_content: Some(taffy::JustifyContent::Center),
            // align_items: Some(taffy::AlignItems::Center),
            // justify_items: Some(taffy::JustifyItems::Center),
            overflow: taffy::geometry::Point {
                x: taffy::style::Overflow::Visible,
                y: taffy::style::Overflow::Visible,
            },
            // min_size: taffy::Size::from_lengths(icon_width/2.0, icon_width + 30.0),
            ..Default::default()
        })
        .size((
            Size {
                width: taffy::Dimension::Length(icon_width),
                height: taffy::Dimension::Length(icon_width + 30.0),
            },
            Some(Transition {
                duration: 0.2,
                ..Default::default()
            }), // None
        ))
        .background_color(PaintColor::Solid {
            color: Color::new_rgba(1.0, 0.0, 0.0, 0.0),
        })
        .content(Some(draw_picture))
        
        .on_pointer_in(move |_layer: Layer, _x, _y| {
            println!("pointer in {:?}", _layer.id());
            view_ref.set_internal_state("show_label", &true);
        })
        .on_pointer_out(move |_layer: Layer, _x, _y| {
            println!("pointer out {:?}", _layer.id());
            view_ref2.set_internal_state("show_label", &false);
        })
        .on_pointer_press(move |_layer: Layer, _x, _y| {
            view_ref3.set_internal_state("pressed", &true);
            println!("pointer press {:?}", _layer.id());
        })
        .on_pointer_release(move |_layer: Layer, _x, _y| {
            view_ref4.set_internal_state("pressed", &false);
            println!("pointer release {:?}", _layer.id());
        })
        .children(vec![LayerTreeBuilder::default()
            .key(format!("{}_label", key))
            .layout_style(taffy::Style {
                position: taffy::Position::Relative,
                max_size: taffy::geometry::Size {
                    width: taffy::style::Dimension::Length(label_size_width),
                    height: taffy::style::Dimension::Length(label_size_height),
                },
                inset: taffy::geometry::Rect::<LengthPercentageAuto> {
                    top: LengthPercentageAuto::Auto,
                    right: LengthPercentageAuto::Auto,
                    bottom: LengthPercentageAuto::Auto,
                    left: LengthPercentageAuto::Percent(0.5),
                },
                ..Default::default()
            })
            .size(Size {
                width: taffy::Dimension::Length(label_size_width),
                height: taffy::Dimension::Length(label_size_height),
            })
            .position(layers::prelude::Point {
                x: -label_size_width / 2.0,
                y: -label_size_height - 10.0 + safe_margin,
            })
            .opacity((label_opacity, None))
            .pointer_events(false)
            .content(Some(draw_label))
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
