use std::cell::RefCell;

use layers::{prelude::*, types::Size};
use taffy::FromLength;

use crate::config::Config;

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
