use smithay::{
    backend::renderer::{
        element::{
            solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement,
            texture::TextureRenderElement, RenderElement, Wrap,
        },
        ImportAll, ImportMem, Renderer,
    },
    desktop::{space::SpaceRenderElements, Space, Window},
    output::Output,
};

pub mod layers_renderer;

use crate::debug::fps::FpsElement;

pub static CLEAR_COLOR: [f32; 4] = [0.8, 0.8, 0.9, 1.0];
pub static CLEAR_COLOR_FULLSCREEN: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
pub const HEADER_BAR_HEIGHT: i32 = 32;

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
smithay::backend::renderer::element::render_elements! {
    pub PointerRenderElement<R> where
        R: ImportAll;
    Surface=WaylandSurfaceRenderElement<R>,
    Texture=TextureRenderElement<<R as Renderer>::TextureId>,
}

impl<R: Renderer> std::fmt::Debug for PointerRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(arg0) => f.debug_tuple("Surface").field(arg0).finish(),
            Self::Texture(arg0) => f.debug_tuple("Texture").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}
smithay::backend::renderer::element::render_elements! {
    pub CustomRenderElements<R> where
        R: ImportAll + ImportMem;
    Pointer=PointerRenderElement<R>,
    Surface=WaylandSurfaceRenderElement<R>,
    #[cfg(feature = "debug")]
    // Note: We would like to borrow this element instead, but that would introduce
    // a feature-dependent lifetime, which introduces a lot more feature bounds
    // as the whole type changes and we can't have an unused lifetime (for when "debug" is disabled)
    // in the declaration.
    Fps=FpsElement<<R as Renderer>::TextureId>,
}

smithay::backend::renderer::element::render_elements! {
pub OutputRenderElements<R, E> where R: ImportAll + ImportMem;
Space=SpaceRenderElements<R, E>,
Window=Wrap<E>,
Custom=CustomRenderElements<R>,
// Preview=CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>,
}

impl<R: Renderer + ImportAll + ImportMem, E: RenderElement<R> + std::fmt::Debug> std::fmt::Debug
    for OutputRenderElements<R, E>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space(arg0) => f.debug_tuple("Space").field(arg0).finish(),
            Self::Window(arg0) => f.debug_tuple("Window").field(arg0).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").finish(),
            // Self::Preview(arg0) => f.debug_tuple("Preview").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

smithay::backend::renderer::element::render_elements!(
    pub WindowRenderElement<R> where R: ImportAll + ImportMem;
    Window=WaylandSurfaceRenderElement<R>,
    Decoration=SolidColorRenderElement,
);

impl<R: Renderer> std::fmt::Debug for WindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window(arg0) => f.debug_tuple("Window").field(arg0).finish(),
            Self::Decoration(arg0) => f.debug_tuple("Decoration").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

#[profiling::function]
pub fn output_elements<R>(
    output: &Output,
    space: &Space<Window>,
    custom_elements: impl IntoIterator<Item = CustomRenderElements<R>>,
    renderer: &mut R,
    // show_window_preview: bool,
) -> (
    Vec<OutputRenderElements<R, WaylandSurfaceRenderElement<R>>>,
    [f32; 4],
)
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    // if let Some(window) = output
    //     .user_data()
    //     .get::<FullscreenSurface>()
    //     .and_then(|f| f.get())
    // {
    //     let scale = output.current_scale().fractional_scale().into();
    //     let window_render_elements: Vec<WindowRenderElement<R>> =
    //         AsRenderElements::<R>::render_elements(&window, renderer, (0, 0).into(), scale, 1.0);

    //     let elements = custom_elements
    //         .into_iter()
    //         .map(OutputRenderElements::from)
    //         .chain(
    //             window_render_elements
    //                 .into_iter()
    //                 .map(|e| OutputRenderElements::Window(Wrap::from(e))),
    //         )
    //         .collect::<Vec<_>>();
    //     (elements, CLEAR_COLOR_FULLSCREEN)
    // } else {
    let mut output_render_elements = custom_elements
        .into_iter()
        .map(OutputRenderElements::from)
        .collect::<Vec<_>>();

    // if show_window_preview && space.elements_for_output(output).count() > 0 {
    //     output_render_elements.extend(space_preview_elements(renderer, space, output));
    // }

    let space_elements = smithay::desktop::space::space_render_elements::<_, Window, _>(
        renderer,
        [space],
        output,
        1.0,
    )
    .expect("output without mode?");
    output_render_elements.extend(space_elements.into_iter().map(OutputRenderElements::Space));

    (output_render_elements, CLEAR_COLOR)
    // }
}
