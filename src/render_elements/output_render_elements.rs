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
    SkiaElement: (RenderElement<R>),
    SceneElement: (RenderElement<R>),
    <R as smithay::backend::renderer::Renderer>::Frame<'frame>: (AsMut<SkiaFrame<'frame>>),
    <R as smithay::backend::renderer::Renderer>::Error: (From<smithay::backend::renderer::gles::GlesError>);
    Space=SpaceRenderElements<R, E>,
    Window=Wrap<E>,
    Custom=WorkspaceRenderElements<'frame, R>,
    Preview=CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>,
}
