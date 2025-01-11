use smithay::{
    backend::renderer::{
        element::{
            utils::{CropRenderElement, RelocateRenderElement, RescaleRenderElement},
            RenderElement, Wrap,
        },
        ImportAll, ImportMem,
    },
    desktop::space::SpaceRenderElements,
};

use crate::{shell::WindowRenderElement, skia_renderer::SkiaFrame};

use super::{
    scene_element::SceneElement, skia_element::SkiaElement,
    workspace_render_elements::WorkspaceRenderElements,
};

smithay::backend::renderer::element::render_elements! {
    pub OutputRenderElements<'frame, R, E> where
    R: ImportAll + ImportMem + 'frame,
    SceneElement: (RenderElement<R>);
    Window=Wrap<E>,
    Workspace=WorkspaceRenderElements<'frame, R>,
}
