use lay_rs::skia::PathEffect;
use lay_rs::{prelude::*, types::Size};
use taffy::LengthPercentageAuto;

use crate::{
    config::Config, workspaces::{
        utils::{draw_balloon_rect, FONT_CACHE},
        Application,
    }
};

pub fn setup_app_icon(
    layer: &Layer,
    icon_layer: &Layer,
    application: Application,
    icon_width: f32,
) {
    let app_name = application
        .desktop_name
        .clone()
        .unwrap_or(application.identifier.clone());

    let draw_picture = draw_app_icon(&application);

    let container_tree = LayerTreeBuilder::default()
        .key(app_name)
        .layout_style(taffy::Style {
            display: taffy::Display::Flex,
            position: taffy::Position::Relative,
            overflow: taffy::geometry::Point {
                x: taffy::style::Overflow::Visible,
                y: taffy::style::Overflow::Visible,
            },
            ..Default::default()
        })
        .size((
            Size {
                width: taffy::Dimension::Length(icon_width),
                height: taffy::Dimension::Length(icon_width + 30.0),
            },
            Some(Transition::ease_in_quad(0.2)), // None
        ))
        .background_color(Color::new_rgba(1.0, 0.0, 0.0, 0.0))
        .build()
        .unwrap();
    layer.build_layer_tree(&container_tree);

    let icon_tree = LayerTreeBuilder::default()
        .key("icon")
        .layout_style(taffy::Style {
            display: taffy::Display::Block,
            position: taffy::Position::Relative,
            ..Default::default()
        })
        .size((
            Size {
                width: taffy::Dimension::Percent(1.0),
                height: taffy::Dimension::Percent(1.0),
            },
            None, // None
        ))
        .pointer_events(false)
        .background_color(Color::new_rgba(1.0, 0.0, 0.0, 0.0))
        .content(Some(draw_picture))
        .build()
        .unwrap();
    icon_layer.build_layer_tree(&icon_tree);
}

pub fn setup_miniwindow_icon(layer: &Layer, inner_layer: &Layer, _icon_width: f32) {
    let container_tree = LayerTreeBuilder::default()
        .key("miniwindow")
        .layout_style(taffy::Style {
            display: taffy::Display::Flex,
            ..Default::default()
        })
        .size((
            Size {
                width: taffy::Dimension::Length(0.0),
                height: taffy::Dimension::Percent(1.0),
            },
            Some(Transition::ease_in_quad(0.2)),
        ))
        .background_color(Color::new_rgba(1.0, 0.0, 0.0, 0.0))
        .build()
        .unwrap();
    layer.build_layer_tree(&container_tree);

    let inner_tree = LayerTreeBuilder::default()
        .key("mini_window_content")
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            ..Default::default()
        })
        .position(Point::default())
        .size((
            Size {
                width: taffy::Dimension::Percent(1.0),
                height: taffy::Dimension::Percent(1.0),
            },
            None,
        ))
        // fixme
        // .image_cache(true)
        .pointer_events(false)
        // .background_color(Color::new_rgba(0.0, 0.5, 0.0, 0.5))
        .build()
        .unwrap();
    inner_layer.build_layer_tree(&inner_tree);
}

pub fn setup_label(new_layer: &Layer, label_text: String) {
    let text_size = 26.0;
    let font_family = Config::with(|config| config.font_family.clone());
    let typeface = FONT_CACHE
        .with(|font_cache| {
            font_cache
                .font_mgr
                .match_family_style(font_family, lay_rs::skia::FontStyle::default())
        })
        .unwrap();
    let font = lay_rs::skia::Font::from_typeface_with_params(typeface, text_size, 1.0, 0.0);

    let text = label_text.clone();
    let paint = lay_rs::skia::Paint::default();
    let text_bounds = font.measure_str(label_text, Some(&paint));

    let text_bounds = text_bounds.1;
    let arrow_height = 20.0;
    let text_padding_h = 30.0;
    let text_padding_v = 14.0;
    let safe_margin = 100.0;
    let label_size_width = text_bounds.width() + text_padding_h * 2.0 + safe_margin * 2.0;
    let label_size_height =
        text_bounds.height() + arrow_height + text_padding_v * 2.0 + safe_margin * 2.0;

    let draw_label = move |canvas: &lay_rs::skia::Canvas, w: f32, h: f32| -> lay_rs::skia::Rect {
        // Tooltip parameters
        // let text = "This is a tooltip!";
        let text = text.clone();
        let rect_corner_radius = 10.0;
        let arrow_width = 25.0;
        let arrow_corner_radius = 3.0;

        // Paint for the tooltip background
        let mut paint = lay_rs::skia::Paint::default();
        paint.set_color(lay_rs::skia::Color::from_argb(230, 255, 255, 255)); // Light gray
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
        let shadow_color = lay_rs::skia::Color::from_argb(80, 0, 0, 0); // semi-transparent black
        let mut shadow_paint = lay_rs::skia::Paint::default();
        shadow_paint.set_color(shadow_color);
        shadow_paint.set_anti_alias(true);
        shadow_paint.set_mask_filter(lay_rs::skia::MaskFilter::blur(
            lay_rs::skia::BlurStyle::Normal,
            10.0,
            None,
        ));

        let mut shadow_path = arrow_path.clone();
        shadow_path.offset((-0.0, -0.0));
        canvas.draw_path(&shadow_path, &shadow_paint);

        // // Draw the arrow path (under the rectangle)
        canvas.draw_path(&arrow_path, &paint);

        // // Paint for the text
        let mut text_paint = lay_rs::skia::Paint::default();
        text_paint.set_color(lay_rs::skia::Color::BLACK);
        text_paint.set_anti_alias(true);

        // // Draw the text inside the tooltip
        let text_x = safe_margin + text_padding_h;
        let text_y = text_bounds.height() + text_padding_v + safe_margin - text_size * 0.2;
        canvas.draw_str(text.as_str(), (text_x, text_y), &font, &text_paint);
        lay_rs::skia::Rect::from_xywh(0.0, 0.0, w, h)
    };
    let label_tree = LayerTreeBuilder::default()
        .key(format!("{}_label", new_layer.key()))
        .layout_style(taffy::Style {
            position: taffy::Position::Absolute,
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
        .position(Point {
            x: -label_size_width / 2.0,
            y: -label_size_height - 10.0 + safe_margin,
        })
        .opacity((0.0, None))
        .pointer_events(false)
        .content(Some(draw_label))
        .build()
        .unwrap();

    new_layer.build_layer_tree(&label_tree);
}

pub fn draw_app_icon(application: &Application) -> ContentDrawFunction {
    let application = application.clone();
    let draw_picture = move |canvas: &lay_rs::skia::Canvas, w: f32, h: f32| -> lay_rs::skia::Rect {
        let icon_size = (w).max(0.0);
        let circle_radius = 6.0;
        let icon_y = (h - 20.0 - circle_radius * 2.0) / 2.0 - icon_size / 2.0;

        if let Some(image) = &application.icon.clone() {
            let mut paint =
                lay_rs::skia::Paint::new(lay_rs::skia::Color4f::new(1.0, 1.0, 1.0, 1.0), None);

            paint.set_style(lay_rs::skia::paint::Style::Fill);
            // draw image with shadow
            let shadow_color = lay_rs::skia::Color4f::new(0.0, 0.0, 0.0, 0.5);

            let mut shadow_paint = lay_rs::skia::Paint::new(shadow_color, None);
            let shadow_offset = lay_rs::skia::Vector::new(5.0, 5.0);
            let shadow_color = lay_rs::skia::Color::from_argb(128, 0, 0, 0); // semi-transparent black
            let shadow_blur_radius = 5.0;

            let shadow_filter = lay_rs::skia::image_filters::drop_shadow_only(
                (shadow_offset.x, shadow_offset.y),
                (shadow_blur_radius, shadow_blur_radius),
                shadow_color,
                None,
                None,
                lay_rs::skia::image_filters::CropRect::default(),
            );
            shadow_paint.set_image_filter(shadow_filter);

            canvas.draw_image_rect(
                image,
                None,
                lay_rs::skia::Rect::from_xywh(0.0, icon_y, icon_size, icon_size),
                &shadow_paint,
            );
            let resampler = lay_rs::skia::CubicResampler::catmull_rom();

            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                lay_rs::skia::Rect::from_xywh(0.0, icon_y, icon_size, icon_size),
                lay_rs::skia::SamplingOptions::from(resampler),
                &paint,
            );
        } else {
            let mut rect = lay_rs::skia::Rect::from_xywh(0.0, 0.0, icon_size, icon_size);
            rect.inset((10.0, 10.0));
            let rrect = lay_rs::skia::RRect::new_rect_xy(rect, 10.0, 10.0);
            let mut paint =
                lay_rs::skia::Paint::new(lay_rs::skia::Color4f::new(1.0, 1.0, 1.0, 0.2), None);
            canvas.draw_rrect(rrect, &paint);

            paint.set_stroke(true);
            paint.set_stroke_width(6.0);
            paint.set_color4f(lay_rs::skia::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            let intervals = [12.0, 6.0]; // Length of the dash and the gap
            let path_effect = PathEffect::dash(&intervals, 0.0);
            paint.set_path_effect(path_effect);
            canvas.draw_rrect(rrect, &paint);
        }
        let mut paint =
            lay_rs::skia::Paint::new(lay_rs::skia::Color4f::new(0.0, 0.0, 0.0, 0.5), None);
        paint.set_anti_alias(true);
        paint.set_style(lay_rs::skia::paint::Style::Fill);
        canvas.draw_circle((w / 2.0, h - (10.0 + circle_radius)), circle_radius, &paint);

        lay_rs::skia::Rect::from_xywh(0.0, 0.0, w, h)
    };

    draw_picture.into()
}
