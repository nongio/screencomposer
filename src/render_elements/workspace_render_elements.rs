use smithay::{
    backend::renderer::{
        element::{Element, Id, RenderElement},
        utils::{CommitCounter, DamageSet},
        ImportAll, ImportMem,
    },
    utils::{Physical, Rectangle, Scale},
};

use crate::{
    drawing::{FpsElement, PointerRenderElement},
    skia_renderer::SkiaFrame,
};

use super::{scene_element::SceneElement, skia_element::SkiaElement};

smithay::backend::renderer::element::render_elements! {
    pub WorkspaceRenderElements<'a, R> where
        R: ImportAll + ImportMem + 'a,
        SkiaElement: (RenderElement<R>),
        SceneElement: (RenderElement<R>),
        <R as smithay::backend::renderer::Renderer>::Frame<'a>: (AsMut<SkiaFrame<'a>>),
        <R as smithay::backend::renderer::Renderer>::Error: (From<smithay::backend::renderer::gles::GlesError>);
    Pointer=PointerRenderElement<R>,
    Fps=FpsElement<<R as smithay::backend::renderer::Renderer>::TextureId>,
    Scene=SceneElement,
    // this is needed to make the macro work with a lifetime specifier in the where clauses
    PhantomElement=PhantomElement<'a>,
}

// this is needed to make the macro work with a lifetime specifier in the where clauses
pub struct PhantomElement<'a> {
    id: Id,
    commit_counter: CommitCounter,
    phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> Element for PhantomElement<'a> {
    fn id(&self) -> &Id {
        &self.id
    }

    fn location(
        &self,
        _scale: smithay::utils::Scale<f64>,
    ) -> smithay::utils::Point<i32, smithay::utils::Physical> {
        (0, 0).into()
    }

    fn src(&self) -> smithay::utils::Rectangle<f64, smithay::utils::Buffer> {
        smithay::utils::Rectangle::from_loc_and_size((0, 0), (0, 0)).to_f64()
    }

    fn geometry(
        &self,
        scale: smithay::utils::Scale<f64>,
    ) -> smithay::utils::Rectangle<i32, smithay::utils::Physical> {
        smithay::utils::Rectangle::from_loc_and_size(self.location(scale), (0, 0))
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit_counter
    }
    /// Get the damage since the provided commit relative to the element
    fn damage_since(
        &self,
        scale: Scale<f64>,
        _commit: Option<CommitCounter>,
    ) -> smithay::backend::renderer::utils::DamageSet<i32, Physical> {
        DamageSet::from_slice(&[Rectangle::from_loc_and_size(
            (0, 0),
            self.geometry(scale).size,
        )])
    }
    fn alpha(&self) -> f32 {
        0.0
    }
}

#[allow(clippy::extra_unused_lifetimes)]
impl<'renderer, 'alloc, R> RenderElement<R> for PhantomElement<'renderer>
where
    R: smithay::backend::renderer::Renderer + 'renderer,
{
    fn draw(
        &self,
        _frame: &mut <R as smithay::backend::renderer::Renderer>::Frame<'_>,
        _src: smithay::utils::Rectangle<f64, smithay::utils::Buffer>,
        _dst: smithay::utils::Rectangle<i32, smithay::utils::Physical>,
        _damage: &[smithay::utils::Rectangle<i32, smithay::utils::Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), <R as smithay::backend::renderer::Renderer>::Error> {
        Ok(())
    }
}
