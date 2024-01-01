use smithay::{backend::renderer::{ImportAll, ImportMem, element::{Wrap, utils::{CropRenderElement, RelocateRenderElement, RescaleRenderElement}, RenderElement}}, desktop::space::SpaceRenderElements};

use crate::{shell::WindowRenderElement, skia_renderer::SkiaFrame};

use super::{custom_render_elements::CustomRenderElements, skia_element::SkiaElement};


smithay::backend::renderer::element::render_elements! {
    pub OutputRenderElements<'frame, R, E> where 
    R: ImportAll + ImportMem + 'frame,
    SkiaElement: (RenderElement<R>),
    <R as smithay::backend::renderer::Renderer>::Frame<'frame>: (AsMut<SkiaFrame>),
    <R as smithay::backend::renderer::Renderer>::Error: (From<smithay::backend::renderer::gles::GlesError>);
    Space=SpaceRenderElements<R, E>,
    Window=Wrap<E>,
    Custom=CustomRenderElements<'frame, R>,
    Preview=CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>,
}