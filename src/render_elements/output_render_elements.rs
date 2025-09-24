use smithay::backend::renderer::{
    element::{RenderElement, Wrap},
    ImportAll, ImportMem,
};

use super::{scene_element::SceneElement, workspace_render_elements::WorkspaceRenderElements};

smithay::backend::renderer::element::render_elements! {
    pub OutputRenderElements<'frame, R, E> where
    R: ImportAll + ImportMem + 'frame,
    SceneElement: (RenderElement<R>);
    Window=Wrap<E>,
    Workspace=WorkspaceRenderElements<'frame, R>,
}
