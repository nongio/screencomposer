use std::hash::{Hash, Hasher};

use layers::prelude::*;

use crate::utils::Observer;

use super::Workspace;

#[derive(Clone, Debug)]
pub struct WorkspaceSelectorViewState {}
impl Hash for WorkspaceSelectorViewState {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

pub struct WorkspaceSelectorView {
    // engine: layers::prelude::LayersEngine,
    pub layer: layers::prelude::Layer,
    pub view: layers::prelude::View<WorkspaceSelectorViewState>,
    // pub state: RwLock<BackgroundViewState>,
}

impl WorkspaceSelectorView {
    pub fn new(_layers_engine: LayersEngine, layer: Layer) -> Self {
        let state = WorkspaceSelectorViewState {};
        let view = View::new(
            "workspace_selector_view",
            state,
            render_window_selector_view,
        );

        layer.set_position((0.0, -200.0), None);
        Self {
            // engine: layers_engine,
            layer,
            view,
            // state: RwLock::new(state),
        }
    }
}

fn render_window_selector_view(
    _state: &WorkspaceSelectorViewState,
    _view: &View<WorkspaceSelectorViewState>,
) -> LayerTree {
    let _draw_container = move |canvas: &skia_safe::Canvas, w, h| {
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, w, 200.0);
        let color = skia_safe::Color4f::new(0.8, 0.8, 0.8, 0.3);
        let paint = skia_safe::Paint::new(color, None);

        canvas.draw_rect(rect, &paint);
        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
    };

    LayerTreeBuilder::default()
        .key("workspace_selector_view")
        .size((
            layers::types::Size {
                width: layers::taffy::style::Dimension::Percent(1.0),
                height: layers::taffy::style::Dimension::Length(200.0),
            },
            None,
        ))
        .background_color((
            PaintColor::Solid {
                color: Color::new_rgba(0.8, 0.8, 0.8, 0.6),
            },
            None,
        ))
        .blend_mode(BlendMode::BackgroundBlur)
        .shadow_color((Color::new_rgba(0.0, 0.0, 0.0, 0.2), None))
        .shadow_offset(((0.0, 0.0).into(), None))
        .shadow_radius((5.0, None))
        // .content(Some(draw_container))
        .build()
        .unwrap()
}

impl Observer<Workspace> for WorkspaceSelectorView {
    fn notify(&self, _event: &Workspace) {}
}
