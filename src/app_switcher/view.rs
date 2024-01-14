use std::cell::RefCell;

use layers::{prelude::*, types::Size};

use super::AppSwitcher;

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
pub struct AppIconState {
    pub name: String,
    pub index: usize,
    pub icon: Option<skia_safe::Image>,
}

pub fn view_app_icon(state: AppIconState, icon_width: f32) -> ViewLayer {
    const PADDING: f32 = 20.0;

    let draw_picture = move |canvas: &mut skia_safe::Canvas, w: f32, h| {
        if let Some(image) = &state.icon {
            let mut paint =
                skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            paint.set_anti_alias(true);
            paint.set_style(skia_safe::paint::Style::Fill);

            // draw image with shadow
            let shadow_offset = skia_safe::Vector::new(10.0, 10.0);
            let shadow_color = skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.5);
            let shadow_blur_radius = 5.0;

            let mut shadow_paint = skia_safe::Paint::new(shadow_color, None);
            // shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, shadow_blur_radius, None));
            // let rect = skia_safe::Rect::from_xywh( shadow_offset.x,  shadow_offset.y, ICON_SIZE, ICON_SIZE);
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
            let icon_size = (w - PADDING * 2.0).max(0.0);
            canvas.draw_image_rect(
                image,
                None,
                skia_safe::Rect::from_xywh(PADDING, PADDING, icon_size, icon_size),
                &shadow_paint,
            );
            let resampler = skia_safe::CubicResampler::catmull_rom();
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                skia_safe::Rect::from_xywh(PADDING, PADDING, icon_size, icon_size),
                skia_safe::SamplingOptions::from(resampler),
                &paint,
            );
        }
    };
    ViewLayerBuilder::default()
        .id(format!("item_{}", state.name))
        .size((
            Size {
                width: taffy::Dimension::Points(icon_width + PADDING * 2.0),
                height: taffy::Dimension::Points(icon_width + PADDING * 2.0),
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
        .build()
        .unwrap()
}
pub fn view_app_switcher(state: &AppSwitcher) -> ViewLayer {
    const COMPONENT_PADDING_H: f32 = 30.0;
    const COMPONENT_PADDING_V: f32 = 50.0;
    const ICON_PADDING: f32 = 25.0;
    const GAP: f32 = 0.0;
    const ICON_SIZE: f32 = 200.0;
    const FONT_SIZE: f32 = 24.0;

    let available_width = state.width as f32;
    let apps_len = state.apps.len() as f32;
    let total_gaps = (apps_len - 1.0) * GAP; // gaps between items

    let total_padding = 2.0 * COMPONENT_PADDING_H + apps_len * ICON_PADDING * 2.0; // padding on both sides
    let available_icon_size =
        (available_width - total_padding - total_gaps) / state.apps.len() as f32;
    let icon_size = ICON_SIZE.min(available_icon_size);
    let component_width = apps_len * icon_size + total_gaps + total_padding;
    let component_height = icon_size + ICON_PADDING * 2.0 + COMPONENT_PADDING_V * 2.0;
    let background_color = Color::new_rgba(1.0, 1.0, 1.0, 0.4);
    let current_app = state.current_app as f32;
    let mut app_name = "".to_string();
    if !state.apps.is_empty() && state.current_app < state.apps.len() {
        app_name = state.apps[state.current_app].0.name.clone();
    }
    let draw_container = move |canvas: &mut skia_safe::Canvas, _w, h| {
        let color = skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.2);
        let paint = skia_safe::Paint::new(color, None);

        let available_icon_size = h - COMPONENT_PADDING_V * 2.0 - ICON_PADDING * 2.0;
        let icon_size = ICON_SIZE.min(available_icon_size);
        let selection_width = icon_size + ICON_PADDING * 2.0;
        let selection_height = selection_width;
        let selection_x = COMPONENT_PADDING_H
            + current_app * (icon_size + ICON_PADDING * 2.0)
            + GAP * current_app;
        let selection_y = h / 2.0 - selection_height / 2.0;
        let rrect = skia_safe::RRect::new_rect_xy(
            skia_safe::Rect::from_xywh(selection_x, selection_y, selection_width, selection_height),
            20.0,
            20.0,
        );
        if apps_len > 0.0 {
            canvas.draw_rrect(rrect, &paint);
            let mut text_style = skia_safe::textlayout::TextStyle::new();

            text_style.set_font_size(FONT_SIZE);
            let font_style = skia_safe::FontStyle::new(
                skia_safe::font_style::Weight::MEDIUM,
                skia_safe::font_style::Width::CONDENSED,
                skia_safe::font_style::Slant::Upright,
            );
            text_style.set_font_style(font_style);
            text_style.set_letter_spacing(-1.0);
            let foreground_paint =
                skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.5), None);
            text_style.set_foreground_color(&foreground_paint);
            text_style.set_font_families(&["Inter"]);

            let mut paragraph_style = skia_safe::textlayout::ParagraphStyle::new();
            paragraph_style.set_text_style(&text_style);
            paragraph_style.set_max_lines(1);
            paragraph_style.set_text_align(skia_safe::textlayout::TextAlign::Center);
            paragraph_style.set_text_direction(skia_safe::textlayout::TextDirection::LTR);
            paragraph_style.set_ellipsis("…");

            let mut builder = FONT_CACHE.with(|font_cache| {
                skia_safe::textlayout::ParagraphBuilder::new(
                    &paragraph_style,
                    font_cache.font_collection.clone(),
                )
            });
            let mut paragraph = builder.add_text(&app_name).build();
            paragraph.layout(selection_width);
            let text_x = selection_x;
            let text_y = selection_y + selection_height + FONT_SIZE * 0.2;
            paragraph.paint(canvas, (text_x, text_y));
            // };
        }
    };
    ViewLayerBuilder::default()
        .id("apps_switcher")
        .size((
            Size {
                width: taffy::Dimension::Points(component_width),
                height: taffy::Dimension::Points(component_height),
            },
            Some(Transition {
                duration: 1.0,
                ..Default::default()
            }),
        ))
        .blend_mode(BlendMode::BackgroundBlur)
        .background_color((
            PaintColor::Solid {
                color: background_color,
            },
            None,
        ))
        .content(Some(draw_container))
        .border_corner_radius((BorderRadius::new_single(50.0), None))
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            display: taffy::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        })
        .children(vec![ViewLayerBuilder::default()
            .id("apps_container")
            .size((
                Size {
                    width: taffy::Dimension::Auto,
                    height: taffy::Dimension::Auto,
                },
                Some(Transition {
                    duration: 2.0,
                    ..Default::default()
                }),
            ))
            .layout_style(taffy::Style {
                position: taffy::Position::Absolute,
                display: taffy::Display::Flex,
                justify_content: Some(taffy::JustifyContent::Center),
                justify_items: Some(taffy::JustifyItems::Center),
                align_items: Some(taffy::AlignItems::Baseline),
                ..Default::default()
            })
            .children(
                state
                    .apps
                    .iter()
                    .enumerate()
                    .map(|(i, (app, _))| {
                        let icon = state.preview_images.get(&app.name).cloned();
                        view_app_icon(
                            AppIconState {
                                name: app.name.clone(),
                                index: i,
                                icon,
                            },
                            icon_size,
                        )
                    })
                    .collect::<Vec<ViewLayer>>(),
            )
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
