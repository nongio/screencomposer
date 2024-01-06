
use std::sync::Arc;

use layers::engine::{ NodeRef, node::SceneNode};
use layers::prelude::{draw_node_children, render_node};
use smithay::{
    backend::renderer::{
        element::{Element, Id, RenderElement},
        utils::CommitCounter,
        Renderer,
    },
    utils::{Buffer, Physical, Point, Rectangle, Scale},
};

use crate::{skia_renderer::SkiaRenderer, udev::UdevRenderer};

#[derive(Clone)]
pub struct SceneElement {
    id: Id,
    commit_counter: CommitCounter,
    scene: Arc<layers::engine::scene::Scene>,
    root_id: Option<layers::prelude::NodeRef>,
}

impl SceneElement {
    pub fn with_scene(scene: Arc<layers::engine::scene::Scene>, root_id: Option<NodeRef>) -> Self {
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            scene,
            root_id,
        }
    }
    pub fn update(&mut self) {
        self.commit_counter.increment();
    }
    pub fn root_layer(&self) -> Option<SceneNode> {
        let root_id = self.root_id?;
        let node = self.scene.get_node(root_id)?;
        Some(node.get().clone())
    }
}

impl Element for SceneElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn location(&self, _scale: Scale<f64>) -> Point<i32, Physical> {
        if let Some(root) = self.root_layer() {
            let bounds = root.bounds();
            (bounds.x as i32, bounds.y as i32).into()
        } else {
            (0, 0).into()
        }
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_loc_and_size((0, 0), (100, 100)).to_f64()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        if let Some(root) = self.root_layer() {
            let bounds = root.bounds();
            Rectangle::from_loc_and_size(self.location(scale), (bounds.width as i32, bounds.height as i32))
        } else {
            Rectangle::from_loc_and_size(self.location(scale), (0, 0))
        }
        
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit_counter
    }
    /// Get the damage since the provided commit relative to the element
    fn damage_since(
        &self,
        scale: Scale<f64>,
        _commit: Option<CommitCounter>,
    ) -> Vec<Rectangle<i32, Physical>> {
            vec![Rectangle::from_loc_and_size((0, 0), self.geometry(scale).size)]
    }
    fn alpha(&self) -> f32 {
        0.5
    }

}

impl<'renderer, 'alloc> RenderElement<UdevRenderer<'renderer, 'alloc>> for SceneElement
{
    fn draw(
        &self,
        frame: &mut <UdevRenderer<'renderer, 'alloc> as Renderer>::Frame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), <UdevRenderer<'renderer, 'alloc> as Renderer>::Error>
    
    {
        RenderElement::<SkiaRenderer>::draw(self, frame.as_mut(), src, dst, damage)
        .map_err(|e| {
            e.into()
        })
    }
}

impl RenderElement<SkiaRenderer> for SceneElement {
fn draw(
        &self,
        frame: &mut <SkiaRenderer as Renderer>::Frame<'_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), <SkiaRenderer as Renderer>::Error> {
        
        let mut surface = frame.skia_surface.clone();
        let canvas = surface.canvas();
        let scene = &self.scene;
        let arena = scene.nodes.data();
        let arena = &*arena.read().unwrap();
        if let Some(root_id) = self.root_id {
            if let Some(_root) = scene.get_node(root_id) {
                let root = arena.get(root_id.into()).unwrap().get();
                render_node(root, canvas);
                let matrix = root.transform();
                let sc = canvas.save();
                canvas.concat(&matrix);
    
                draw_node_children(root_id, arena, canvas, 1.0);
                canvas.restore_to_count(sc);
            }
        }
        
        Ok(())
    }
}

