use std::{cell::RefCell};

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
    pub icon: Option<skia_safe::Image>,
    pub is_selected: bool,
}
fn view_app_icon(state: AppIconState) -> ViewLayer {
    const ICON_SIZE: f32 = 200.0;
    const PADDING: f32 = 20.0;

    let mut selection_background_color = Color::new_hex("#00000000");
    let mut text_opacity = 0.0;
    if state.is_selected {
        selection_background_color = Color::new_rgba(0.0, 0.0, 0.0, 0.3);
        text_opacity = 1.0;
    }
    let text_picture = {
        let mut recorder = skia_safe::PictureRecorder::new();
        let canvas = recorder.begin_recording(skia_safe::Rect::from_wh(500.0, 500.0), None);

        let mut text_style = skia_safe::textlayout::TextStyle::new();
        text_style.set_font_size(26.0);
        let font_style = skia_safe::FontStyle::new(
            skia_safe::font_style::Weight::MEDIUM,
            skia_safe::font_style::Width::CONDENSED,
            skia_safe::font_style::Slant::Upright,
        );
        text_style.set_font_style(font_style);
        text_style.set_letter_spacing(-1.0);
        let foreground_paint = skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.5), None);
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
        let mut paragraph = builder.add_text(state.name).build();
        paragraph.layout(ICON_SIZE);
        paragraph.paint(canvas, (0.0, 8.0));
        recorder.finish_recording_as_picture(None)
    };
    let icon_picture = {
        let mut recorder = skia_safe::PictureRecorder::new();
        let canvas = recorder.begin_recording(skia_safe::Rect::from_wh(500.0, 500.0), None);
        if let Some(image) = &state.icon {
            let mut paint = skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 1.0), None);
            paint.set_anti_alias(true);
            paint.set_style(skia_safe::paint::Style::Fill);

            let resampler = skia_safe::CubicResampler::catmull_rom();
            // canvas.draw_image_rect_with_sampling_options(
            //     image,
            //     None,
            //     skia_safe::Rect::from_xywh(0.0, 0.0, ICON_SIZE, ICON_SIZE),
            //     skia_safe::SamplingOptions::from(resampler),
            //     &paint,
            // );

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
            canvas.draw_image_rect(
                image,
                None,
                skia_safe::Rect::from_xywh(0.0, 0.0, ICON_SIZE, ICON_SIZE),
                &shadow_paint,
            );
            let resampler = skia_safe::CubicResampler::catmull_rom();
            canvas.draw_image_rect_with_sampling_options(
                image,
                None,
                skia_safe::Rect::from_xywh(0.0, 0.0, ICON_SIZE, ICON_SIZE),
                skia_safe::SamplingOptions::from(resampler),
                &paint,
            );
        }
        recorder.finish_recording_as_picture(None)
    };
    ViewLayerBuilder::default()
        .size((
            Size::points(
                ICON_SIZE + PADDING * 2.0,
                 ICON_SIZE + PADDING * 2.0 + 50.0
            ),
            None,
        ))
        .background_color((
            PaintColor::Solid {
                color: Color::new_hex("#00000000"),
            },
            None,
        ))
        .layout_style(taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            justify_content: Some(taffy::JustifyContent::FlexStart),
            align_items: Some(taffy::AlignItems::Center),
            gap: taffy::Size {
                width: taffy::LengthPercentage::Points(0.0),
                height: taffy::LengthPercentage::Points(0.0),
            },
            ..Default::default()
        })
        .children(vec![
            ViewLayerBuilder::default()
                .layout_style(taffy::Style {
                    display: taffy::Display::Flex,
                    flex_direction: taffy::FlexDirection::Column,
                    justify_content: Some(taffy::JustifyContent::Center),
                    align_items: Some(taffy::AlignItems::Center),
                    ..Default::default()
                })
                .position((Point { x: 0.0, y: 0.0 }, None))
                .size((
                    Size::points(
                        ICON_SIZE + PADDING * 2.0,
                        ICON_SIZE + PADDING * 2.0
                    ),
                    None,
                ))
                .background_color((
                    PaintColor::Solid {
                        color: selection_background_color,
                    },
                    None,
                ))
                .border_corner_radius((BorderRadius::new_single(20.0), None))
                .children(vec![ViewLayerBuilder::default()
                    .size((
                        Size::points(ICON_SIZE, ICON_SIZE),
                        None,
                    ))
                    .background_color((
                        PaintColor::Solid {
                            color: Color::new_hex("#00000000"),
                        },
                        None,
                    ))
                    .content((icon_picture, None))
                    .build()
                    .unwrap()])
                .build()
                .unwrap(),
            ViewLayerBuilder::default()
                .size((
                    Size::points(ICON_SIZE, 50.0),
                    None,
                ))
                .background_color((
                    PaintColor::Solid {
                        color: Color::new_hex("#00000000"),
                    },
                    None,
                ))
                .opacity((text_opacity, None))
                .content((text_picture, None))
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap()
}
pub fn view_app_switcher(state: &AppSwitcher) -> ViewLayer {
    const ICON_SIZE: f32 = 150.0;
    const PADDING: f32 = 20.0;

    let background_color = Color::new_rgba(1.0, 1.0, 1.0, 0.2);

    ViewLayerBuilder::default()
        .opacity((0.0, None))
        .size((
            layers::types::Size {
                width: taffy::Dimension::Auto,
                height: taffy::Dimension::Points(ICON_SIZE + PADDING * 2.0 + 100.0),
            },
            None,
        ))
        .background_color((
            PaintColor::Solid {
                color: background_color,
            },
            None,
        ))
        .blend_mode(BlendMode::BackgroundBlur)
        .border_corner_radius((BorderRadius::new_single(40.0), None))
        .layout_style(taffy::Style {
            padding: taffy::Rect {
                left: taffy::LengthPercentage::Points(PADDING),
                right: taffy::LengthPercentage::Points(PADDING),
                top: taffy::LengthPercentage::Points(PADDING),
                bottom: taffy::LengthPercentage::Points(PADDING),
            },
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            gap: taffy::Size {
                width: taffy::LengthPercentage::Points(0.0),
                height: taffy::LengthPercentage::Points(PADDING),
            },
            min_size: taffy::Size {
                width: taffy::Dimension::Points(200.0),
                height: taffy::Dimension::Points(300.0),
            },
            ..Default::default()
        })
        .children(
            state
                .apps
                .iter()
                .enumerate()
                .map(|(i, (app, _))| {
                    let icon = state.preview_images.get(&app.name).cloned();
                    view_app_icon(AppIconState {
                        name: app.name.clone(),
                        icon,
                        is_selected: i == state.current_app,
                    })
                })
                .collect(),
        )
        .build()
        .unwrap()
}
