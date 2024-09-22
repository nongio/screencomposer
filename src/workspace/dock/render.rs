use std::cell::RefCell;

use layers::{prelude::*, types::Size};
use skia_safe::PathEffect;
use taffy::FromLength;

use crate::{config::Config, workspace::Application};

use super::{
    render_app::{render_app_view, DockAppState},
    view::magnify_function,
};

use super::model::DockModel;

#[allow(dead_code)]
struct FontCache {
    font_collection: skia_safe::textlayout::FontCollection,
    font_mgr: skia_safe::FontMgr,
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

#[allow(non_snake_case)]
pub fn render_dock_view(state: &DockModel, view: &View<DockModel>) -> LayerTree {
    let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;

    // those are constant like values
    let available_width = state.width as f32 - 20.0 * draw_scale;
    let ICON_SIZE: f32 = 100.0 * draw_scale;

    let apps_len = state.running_apps.len() as f32;

    let mut COMPONENT_PADDING_H: f32 = ICON_SIZE * 0.09 * draw_scale;
    if COMPONENT_PADDING_H > 5.0 * draw_scale {
        COMPONENT_PADDING_H = 5.0 * draw_scale;
    }
    let mut COMPONENT_PADDING_V: f32 = ICON_SIZE * 0.09 * draw_scale;
    if COMPONENT_PADDING_V > 50.0 * draw_scale {
        COMPONENT_PADDING_V = 50.0 * draw_scale;
    }
    let available_icon_size =
        (available_width - COMPONENT_PADDING_H * 2.0) / state.running_apps.len() as f32;
    let available_icon_size = ICON_SIZE.min(available_icon_size);

    let component_width = apps_len * available_icon_size + COMPONENT_PADDING_H * 2.0;
    let component_height = available_icon_size + COMPONENT_PADDING_V * 2.0;
    let background_color = Color::new_rgba(0.94, 0.94, 0.94, 0.44);

    LayerTreeBuilder::default()
        .key(view.key())
        .size((
            Size {
                width: taffy::Dimension::Length(component_width * 2.0),
                height: taffy::Dimension::Length(component_height + 20.0),
            },
            Some(Transition {
                duration: 1.0,
                ..Default::default()
            }),
        ))
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            display: taffy::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::End),
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        })
        .children(vec![
            LayerTreeBuilder::default()
                .key("dock_bar")
                .position(((0.0, 0.0).into(), None))
                .size((
                    Size {
                        width: taffy::Dimension::Length(component_width),
                        height: taffy::Dimension::Length(component_height),
                    },
                    Some(Transition {
                        duration: 0.5,
                        ..Default::default()
                    }),
                ))
                .blend_mode(BlendMode::BackgroundBlur)
                .background_color(PaintColor::Solid {
                    color: background_color,
                })
                .border_width((2.5, None))
                .border_color(Color::new_rgba(1.0, 1.0, 1.0, 0.3))
                .shadow_color(Color::new_rgba(0.0, 0.0, 0.0, 0.2))
                .shadow_offset(((0.0, -5.0).into(), None))
                .shadow_radius((20.0, None))
                .border_corner_radius(BorderRadius::new_single(ICON_SIZE / 4.0))
                .layout_style(taffy::Style {
                    position: taffy::Position::Absolute,
                    ..Default::default()
                })
                .build()
                .unwrap(),
            LayerTreeBuilder::default()
                .key("dock_container")
                .size((
                    Size {
                        width: taffy::Dimension::Auto,
                        height: taffy::Dimension::Auto,
                    },
                    None,
                ))
                .position(((0.0, 0.0).into(), None))
                .layout_style(taffy::Style {
                    position: taffy::Position::Absolute,
                    display: taffy::Display::Flex,
                    justify_content: Some(taffy::JustifyContent::FlexEnd),
                    justify_items: Some(taffy::JustifyItems::FlexEnd),
                    align_items: Some(taffy::AlignItems::Baseline),
                    gap: taffy::Size::<taffy::LengthPercentage>::from_length(0.0),
                    ..Default::default()
                })
                .children(
                    state
                        .running_apps
                        .iter()
                        .enumerate()
                        .map(|(index, app)| {
                            let view_key = format!("app_{}", app.identifier);

                            let icon_pos = 1.0 / apps_len * index as f32 + 1.0 / (apps_len * 2.0);
                            let icon_focus = 1.0 + magnify_function(state.focus - icon_pos) * 0.2;
                            let available_icon_size = available_icon_size * icon_focus as f32;
                            let dock_app_state = DockAppState {
                                index,
                                application: app.clone(),
                                icon_width: available_icon_size,
                            };
                            View::new(&view_key, dock_app_state, render_app_view)
                        })
                        .collect::<Vec<View<_>>>(),
                )
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap()
}


pub fn draw_app_icon(application: &Application, pressed: bool) -> ContentDrawFunction {
    let mut darken_factor = 255;
    if pressed {
        darken_factor = 150;
    }
    let application = application.clone();
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

    return draw_picture.into();
}