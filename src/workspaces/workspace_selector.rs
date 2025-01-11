use std::{
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};

use lay_rs::prelude::*;
use smithay::{
    backend::input::ButtonState,
    input::pointer::{CursorIcon, CursorImageStatus},
};

use crate::{
    config::Config,
    interactive_view::ViewInteractions,
    theme::{self, theme_colors},
    utils::{
        button_press_filter, button_release_filter, draw_named_icon, draw_text_content, Observer,
    },
};

use super::WorkspacesModel;

#[derive(Clone, Debug, Hash)]
pub struct WorkspaceViewState {
    name: String,
    index: usize,
    workspace_node: Option<NodeRef>,
}

#[derive(Clone, Debug)]
pub struct WorkspaceSelectorViewState {
    workspaces: Vec<WorkspaceViewState>,
    current: usize,
}

impl Hash for WorkspaceSelectorViewState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.workspaces.hash(state);
        self.current.hash(state);
    }
}

#[derive(Clone)]
pub struct WorkspaceSelectorView {
    pub layer: lay_rs::prelude::Layer,
    pub view: lay_rs::prelude::View<WorkspaceSelectorViewState>,
    pub cursor_location: Arc<RwLock<Point>>,
}

/// # WorkspaceSelectorView Layer Structure
///
/// ```diagram
/// WorkspaceSelectorView
/// ├── layer (view(render_workspace_selector_view))
/// │   ├── workspace_selector_view_content
/// │   │   └── workspace_selector_desktop_{x}
/// │   │       └── workspace_selector_desktop_content_{x}
/// │   │           ├── workspace_desktop_content_preview_{x}
/// │   │           └── workspace_selector_desktop_remove_{x}
/// │   └── workspace_selector_add
/// ```
///
/// - `layer`: The root layer for the window selector view.
///
///
impl WorkspaceSelectorView {
    pub fn new(_layers_engine: LayersEngine, layer: Layer) -> Self {
        let state = WorkspaceSelectorViewState {
            workspaces: Vec::new(),
            current: 0,
        };
        let view = View::new(
            "workspace_selector_view",
            state,
            render_workspace_selector_view,
        );
        layer.set_pointer_events(false);
        layer.set_position((0.0, -250.0), None);
        layer.set_opacity(0.0, None);
        view.set_layer(layer.clone());

        Self {
            // engine: layers_engine,
            layer,
            view,
            cursor_location: Arc::new(RwLock::new(Point::default())), // state: RwLock::new(state),
        }
    }
}

fn render_workspace_selector_view(
    state: &WorkspaceSelectorViewState,
    _view: &View<WorkspaceSelectorViewState>,
) -> LayerTree {
    LayerTreeBuilder::with_key("workspace_selector_view")
        .layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            display: taffy::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            ..Default::default()
        })
        .size((
            lay_rs::types::Size {
                width: lay_rs::taffy::style::Dimension::Percent(1.0),
                height: lay_rs::taffy::style::Dimension::Auto,
            },
            None,
        ))
        .background_color(theme_colors().materials_medium)
        .blend_mode(BlendMode::BackgroundBlur)
        .shadow_color((Color::new_rgba(0.0, 0.0, 0.0, 0.2), None))
        .shadow_offset(((0.0, 0.0).into(), None))
        .shadow_radius((5.0, None))
        // .content(Some(draw_container))
        .children(vec![
            LayerTreeBuilder::with_key("workspace_selector_view_content")
                .layout_style(taffy::Style {
                    display: taffy::Display::Flex,
                    flex_direction: taffy::FlexDirection::Row,
                    align_items: Some(taffy::AlignItems::Center),
                    justify_content: Some(taffy::AlignContent::Center),
                    gap: taffy::length(50.0),
                    padding: taffy::Rect {
                        bottom: taffy::length(20.0),
                        top: taffy::length(30.0),
                        left: taffy::length(10.0),
                        right: taffy::length(10.0),
                    },
                    ..Default::default()
                })
                .size((
                    lay_rs::types::Size {
                        width: lay_rs::taffy::style::Dimension::Percent(1.0),
                        height: lay_rs::taffy::style::Dimension::Auto,
                    },
                    None,
                ))
                .children(
                    state
                        .workspaces
                        .iter()
                        .enumerate()
                        .map(|(i, w)| {
                            let current = i == state.current;
                            let mut border_width = 0.0;
                            if current {
                                border_width = 8.0;
                            }
                            // FIXME: hardcoded values
                            let workspace_width = 1280.0 * 2.0;
                            let workspace_height = 900.0 * 2.0;
                            let preview_width = 300.0;
                            let scale = preview_width / workspace_width;
                            let preview_height = workspace_height * scale;

                            LayerTreeBuilder::with_key(format!(
                                "workspace_selector_desktop_{}",
                                w.index
                            ))
                            .layout_style(taffy::Style {
                                display: taffy::Display::Flex,
                                position: taffy::Position::Relative,
                                flex_direction: taffy::FlexDirection::Column,
                                align_items: Some(taffy::AlignItems::Center),
                                justify_content: Some(taffy::AlignContent::Center),
                                gap: taffy::Size::length(20.0),
                                ..Default::default()
                            })
                            .size((
                                lay_rs::types::Size {
                                    width: lay_rs::taffy::style::Dimension::Length(preview_width),
                                    height: lay_rs::taffy::style::Dimension::Auto,
                                },
                                None,
                            ))
                            .children(vec![
                                LayerTreeBuilder::with_key(format!(
                                    "workspace_selector_desktop_content_{}",
                                    w.index
                                ))
                                .layout_style(taffy::Style {
                                    position: taffy::Position::Relative,
                                    ..Default::default()
                                })
                                .size((
                                    lay_rs::types::Size {
                                        width: lay_rs::taffy::style::Dimension::Length(
                                            preview_width,
                                        ),
                                        height: lay_rs::taffy::style::Dimension::Length(
                                            preview_height,
                                        ),
                                    },
                                    None,
                                ))
                                .children(vec![
                                    LayerTreeBuilder::with_key(format!(
                                        "workspace_desktop_content_preview_{}",
                                        w.index
                                    ))
                                    .layout_style(taffy::Style {
                                        position: taffy::Position::Absolute,
                                        ..Default::default()
                                    })
                                    .size((
                                        lay_rs::types::Size {
                                            width: lay_rs::taffy::style::Dimension::Length(
                                                workspace_width,
                                            ),
                                            height: lay_rs::taffy::style::Dimension::Length(
                                                workspace_height,
                                            ),
                                        },
                                        None,
                                    ))
                                    .scale(Point::new(scale, scale))
                                    .replicate_node(w.workspace_node)
                                    .on_pointer_press(button_press_filter())
                                    .on_pointer_release(button_release_filter())
                                    .border_corner_radius(BorderRadius::new_single(10.0 / scale))
                                    .clip_children(true)
                                    .clip_content(true)
                                    .build()
                                    .unwrap(),
                                    LayerTreeBuilder::with_key(format!(
                                        "workspace_selector_desktop_border_{}",
                                        w.index
                                    ))
                                    .layout_style(taffy::Style {
                                        position: taffy::Position::Absolute,
                                        ..Default::default()
                                    })
                                    .position(Point::new(0.0, 0.0))
                                    .size((
                                        lay_rs::types::Size {
                                            width: lay_rs::taffy::style::Dimension::Percent(1.0),
                                            height: lay_rs::taffy::style::Dimension::Percent(1.0),
                                        },
                                        None,
                                    ))
                                    .border_width((border_width, None))
                                    .border_color(theme_colors().accents_blue)
                                    .border_corner_radius(BorderRadius::new_single(10.0))
                                    .build()
                                    .unwrap(),
                                    LayerTreeBuilder::with_key(format!(
                                        "workspace_selector_desktop_remove_{}",
                                        w.index
                                    ))
                                    .layout_style(taffy::Style {
                                        position: taffy::Position::Absolute,
                                        ..Default::default()
                                    })
                                    .anchor_point(Point::new(0.5, 0.5))
                                    .position(Point::new(preview_width, 0.0))
                                    .size((
                                        lay_rs::types::Size {
                                            width: lay_rs::taffy::style::Dimension::Length(50.0),
                                            height: lay_rs::taffy::style::Dimension::Length(50.0),
                                        },
                                        None,
                                    ))
                                    .background_color(theme_colors().materials_ultrathick)
                                    .border_corner_radius(BorderRadius::new_single(25.0))
                                    .content(draw_named_icon("close-symbolic"))
                                    .shadow_color((Color::new_rgba(0.0, 0.0, 0.0, 0.2), None))
                                    .shadow_offset(((0.0, 0.0).into(), None))
                                    .shadow_radius((5.0, None))
                                    .image_cache(true)
                                    .on_pointer_press(button_press_filter())
                                    .on_pointer_release(button_release_filter())
                                    .build()
                                    .unwrap(),
                                ])
                                .build()
                                .unwrap(),
                                LayerTreeBuilder::with_key(format!(
                                    "workspace_selector_desktop_label_{}",
                                    w.index
                                ))
                                .layout_style(taffy::Style {
                                    position: taffy::Position::Relative,
                                    ..Default::default()
                                })
                                .size((
                                    lay_rs::types::Size {
                                        width: lay_rs::taffy::style::Dimension::Percent(1.0),
                                        height: lay_rs::taffy::style::Dimension::Length(40.0),
                                    },
                                    None,
                                ))
                                // .background_color(theme_colors().accents_purple)
                                .content(draw_text_content(
                                    format!("Bench {}", i + 1),
                                    theme::text_styles::title_3_regular(),
                                    lay_rs::skia::textlayout::TextAlign::Center,
                                ))
                                .build()
                                .unwrap(),
                            ])
                            .build()
                            .unwrap()
                        })
                        .collect(),
                )
                .build()
                .unwrap(),
            LayerTreeBuilder::default()
                .key("workspace_selector_desktop_add")
                .layout_style(taffy::Style {
                    ..Default::default()
                })
                .size((
                    lay_rs::types::Size {
                        width: lay_rs::taffy::style::Dimension::Length(80.0),
                        height: lay_rs::taffy::style::Dimension::Length(80.0),
                    },
                    None,
                ))
                .content(draw_named_icon("plus-symbolic"))
                .image_cache(true)
                .on_pointer_press(button_press_filter())
                .on_pointer_release(button_release_filter())
                .build()
                .unwrap(),
        ])
        .build()
        .unwrap()
}

impl Observer<WorkspacesModel> for WorkspaceSelectorView {
    fn notify(&self, model: &WorkspacesModel) {
        let mut state = self.view.get_state();
        state.workspaces = model
            .workspaces
            .iter()
            .enumerate()
            .map(|(i, w)| WorkspaceViewState {
                name: format!("Bench {}", i),
                index: w.index,
                workspace_node: w.workspace_layer.id(),
            })
            .collect();
        state.current = model.current_workspace;
        self.view.update_state(&state);
    }
}

impl<Backend: crate::state::Backend> ViewInteractions<Backend> for WorkspaceSelectorView {
    fn id(&self) -> Option<usize> {
        self.view
            .layer
            .read()
            .unwrap()
            .as_ref()
            .and_then(|l| l.id())
            .map(|id| id.0.into())
    }

    fn is_alive(&self) -> bool {
        !self
            .view
            .layer
            .read()
            .unwrap()
            .as_ref()
            .map(|l| l.hidden())
            .unwrap_or(true)
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        let state = self.view.get_state().clone();
        let screen_scale = Config::with(|config| config.screen_scale);
        let location = event.location.to_physical(screen_scale);
        let location = lay_rs::types::Point::new(location.x as f32, location.y as f32);
        let mut hover = false;
        if self
            .view
            .hover_layer("workspace_selector_desktop_add", &location)
        {
            hover = true;
        }
        for w in state.workspaces.iter() {
            if self.view.hover_layer(
                &format!("workspace_selector_desktop_{}", w.index),
                &location,
            ) {
                hover = true;
                break;
            }
            if self.view.hover_layer(
                &format!("workspace_selector_desktop_remove_{}", w.index),
                &location,
            ) {
                hover = true;
                break;
            }
        }

        if hover {
            let cursor = CursorImageStatus::Named(CursorIcon::Pointer);
            data.set_cursor(&cursor);
        } else {
            let cursor = CursorImageStatus::Named(CursorIcon::default());
            data.set_cursor(&cursor);
        }
        let mut cursor_location = self.cursor_location.write().unwrap();
        *cursor_location = location;
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        screencomposer: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::ButtonEvent,
    ) {
        let location = self.cursor_location.read().unwrap();
        let state = self.view.get_state().clone();
        match event.state {
            ButtonState::Pressed => {}
            ButtonState::Released => {
                if self
                    .view
                    .hover_layer("workspace_selector_desktop_add", &location)
                {
                    // hover = true;
                    screencomposer.workspaces.add_workspace();
                }
                for (i, w) in state.workspaces.iter().enumerate() {
                    if self.view.hover_layer(
                        &format!("workspace_selector_desktop_{}", w.index),
                        &location,
                    ) {
                        // hover = true;
                        screencomposer
                            .workspaces
                            .set_current_workspace_index(i, None);
                        break;
                    }
                    if self.view.hover_layer(
                        &format!("workspace_selector_desktop_remove_{}", w.index),
                        &location,
                    ) {
                        // hover = true;
                        screencomposer.workspaces.remove_workspace_at(i);
                        break;
                    }
                }
            }
        }
    }
}
