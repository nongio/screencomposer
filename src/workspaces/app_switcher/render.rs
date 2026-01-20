use lay_rs::{prelude::*, types::Size};
use taffy::FromLength;

use crate::{config::Config, theme::theme_colors, workspaces::utils::FONT_CACHE};

use super::render_app::render_app_view;

use super::model::AppSwitcherModel;

#[allow(non_snake_case)]
pub fn render_appswitcher_view(
    state: &AppSwitcherModel,
    view: &View<AppSwitcherModel>,
) -> LayerTree {
    let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;

    // those are constant like values
    let available_width = state.width as f32 - 20.0 * draw_scale;
    let ICON_SIZE: f32 = 160.0 * draw_scale;
    let ICON_PADDING: f32 = available_width * 0.006 * draw_scale;
    let GAP: f32 = ICON_PADDING / 2.0;
    let apps_len = state.apps.len() as f32;
    let total_gaps = (apps_len - 1.0) * GAP; // gaps between items

    let total_padding = apps_len * ICON_PADDING * 2.0 + total_gaps; // padding on both sides
    let container_available_width = available_width - total_padding;
    let mut COMPONENT_PADDING_H: f32 = container_available_width * 0.03 * draw_scale;
    if COMPONENT_PADDING_H > 15.0 * draw_scale {
        COMPONENT_PADDING_H = 15.0 * draw_scale;
    }
    let mut COMPONENT_PADDING_V: f32 = container_available_width * 0.014 * draw_scale;
    if COMPONENT_PADDING_V > 50.0 * draw_scale {
        COMPONENT_PADDING_V = 50.0 * draw_scale;
    }
    let available_icon_size =
        (available_width - total_padding - COMPONENT_PADDING_H * 2.0) / state.apps.len() as f32;
    let available_icon_size = ICON_SIZE.min(available_icon_size);

    let FONT_SIZE: f32 = available_icon_size / 8.0;
    let component_width =
        apps_len * available_icon_size + total_padding + COMPONENT_PADDING_H * 2.0;
    let component_height = available_icon_size + ICON_PADDING * 2.0 + COMPONENT_PADDING_V * 2.0;
    let background_color = theme_colors().materials_thin;
    let current_app = state.current_app as f32;
    let mut app_name = "".to_string();
    if !state.apps.is_empty() && state.current_app < state.apps.len() {
        app_name = state.apps[state.current_app]
            .desktop_name()
            .clone()
            .unwrap_or("".to_string());
    }
    let draw_container = move |canvas: &lay_rs::skia::Canvas, w, h| {
        let selection_background_color = theme_colors().fills_primary.c4f();
        let paint = lay_rs::skia::Paint::new(selection_background_color, None);
        // let available_icon_size = h - COMPONENT_PADDING_V * 2.0 - ICON_PADDING * 2.0;
        // let icon_size = ICON_SIZE.min(available_icon_size);
        let selection_width = available_icon_size + ICON_PADDING * 2.0;
        let selection_height = selection_width;
        let total_gaps = (apps_len - 1.0) * GAP; // gaps between items
        let selection_x = COMPONENT_PADDING_H + total_gaps - GAP * current_app
            + current_app * (available_icon_size + ICON_PADDING * 2.0);
        let selection_y = h / 2.0 - selection_height / 2.0;
        let rrect: lay_rs::skia::RRect = lay_rs::skia::RRect::new_rect_xy(
            lay_rs::skia::Rect::from_xywh(
                selection_x,
                selection_y,
                selection_width,
                selection_height,
            ),
            selection_width / 10.0,
            selection_width / 10.0,
        );
        if apps_len > 0.0 {
            canvas.draw_rrect(rrect, &paint);

            // Create font with subpixel rendering
            let font_family = Config::with(|c| c.font_family.clone());
            let font_style = lay_rs::skia::FontStyle::new(
                lay_rs::skia::font_style::Weight::MEDIUM,
                lay_rs::skia::font_style::Width::CONDENSED,
                lay_rs::skia::font_style::Slant::Upright,
            );
            let font = FONT_CACHE.with(|font_cache| {
                font_cache.make_font_with_fallback(font_family, font_style, FONT_SIZE)
            });

            // Draw text with improved rendering
            let mut text_paint = lay_rs::skia::Paint::new(theme_colors().text_secondary.c4f(), None);
            text_paint.set_anti_alias(true);
            
            let text_bounds = font.measure_str(&app_name, Some(&text_paint)).1;
            let text_x = selection_x + (selection_width - text_bounds.width()) / 2.0;
            let text_y = selection_y + selection_height + FONT_SIZE * 1.2;
            
            canvas.draw_str(&app_name, (text_x, text_y), &font, &text_paint);
        }
        lay_rs::skia::Rect::from_xywh(0.0, 0.0, w, h)
    };
    LayerTreeBuilder::default()
        .key("apps_switcher")
        .size((
            Size {
                width: taffy::Dimension::Length(component_width),
                height: taffy::Dimension::Length(component_height),
            },
            Some(Transition::ease_out_quad(0.35)),
        ))
        .blend_mode(BlendMode::BackgroundBlur)
        .background_color((
            PaintColor::Solid {
                color: background_color,
            },
            None,
        ))
        .content(Some(draw_container))
        .border_corner_radius((BorderRadius::new_single(component_height / 8.0), None))
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            display: taffy::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        })
        .children(vec![LayerTreeBuilder::default()
            .key("apps_container")
            .size((
                Size {
                    width: taffy::Dimension::Auto,
                    height: taffy::Dimension::Auto,
                },
                Some(Transition::ease_out_quad(0.4)),
            ))
            .layout_style(taffy::Style {
                position: taffy::Position::Absolute,
                display: taffy::Display::Flex,
                justify_content: Some(taffy::JustifyContent::Center),
                justify_items: Some(taffy::JustifyItems::Center),
                align_items: Some(taffy::AlignItems::Baseline),
                gap: taffy::Size::<taffy::LengthPercentage>::from_length(GAP),
                ..Default::default()
            })
            .children(
                state
                    .apps
                    .iter()
                    .enumerate()
                    .map(|(index, app)| {
                        render_app_view(
                            index,
                            app.clone(),
                            view.clone(),
                            available_icon_size,
                            ICON_PADDING / 2.0,
                        )
                    })
                    .collect::<Vec<LayerTree>>(),
            )
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
