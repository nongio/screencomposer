use smithay::backend::renderer::{
    element::surface::WaylandSurfaceRenderElement, ImportAll, ImportMem, Renderer,
};

smithay::backend::renderer::element::render_elements! {
    pub LayersRenderElements<R> where
    R: ImportAll + ImportMem;
    Surface=WaylandSurfaceRenderElement<R>,
}

impl<R: Renderer> std::fmt::Debug for LayersRenderElements<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(arg0) => f.debug_tuple("Surface").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}
