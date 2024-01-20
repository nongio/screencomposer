
use std::sync::Arc;

use layers::{engine::{ NodeRef, node::SceneNode, LayersEngine}, drawing::scene::render_node_tree};

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
    engine:LayersEngine,
    last_update: std::time::Instant,
    pub size: (f32, f32),
}

impl SceneElement {
    pub fn with_engine(engine: LayersEngine) -> Self {
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            engine,
            last_update: std::time::Instant::now(),
            size: (0.0, 0.0),
        }
    }
    pub fn update(&mut self) {
        let dt = self.last_update.elapsed().as_secs_f32();
        self.last_update = std::time::Instant::now();
        if self.engine.update(dt) {
            self.commit_counter.increment();
        }
    }
    pub fn root_layer(&self) -> Option<SceneNode> {
        self.engine.root_layer()
    }
    pub fn set_size(&mut self, width: f32, height: f32) {
        self.engine.set_scene_size(width, height);       
        self.size = (width, height);
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
        let scene_damage = self.engine.damage();
        let safe = 50;
            vec![Rectangle::from_loc_and_size((scene_damage.x as i32 - safe, scene_damage.y as i32 -safe), (scene_damage.width as i32 + safe, scene_damage.height as i32 + safe))]
    }
    fn alpha(&self) -> f32 {
        1.0
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
        #[cfg(feature = "profile-with-puffin")]
        profiling::puffin::profile_scope!("render_scene");
        let mut surface = frame.skia_surface.clone();
        let canvas = surface.canvas();
        let scene = self.engine.scene();
        let root_id = self.engine.scene_root();
        let arena = scene.nodes.data();
        let arena = &*arena.read().unwrap();
        // let scene_damage = self.engine.damage();
        // let damage_rect = skia_safe::Rect::from_xywh(scene_damage.x, scene_damage.y, scene_damage.width, scene_damage.height);
        let mut damage_rect = skia_safe::Rect::default();
        damage.iter().for_each(|d| {
            damage_rect.join(skia_safe::Rect::from_xywh(d.loc.x as f32, d.loc.y as f32, d.size.w as f32, d.size.h as f32));
        });

        // println!("damage {:?}", damage_rect.width());
        if let Some(root_id) = root_id {
            let save_point= canvas.save();
            canvas.clip_rect(damage_rect, None, None);
            render_node_tree(root_id, arena, canvas, 1.0);
            canvas.restore_to_count(save_point);
        }
        // let mut paint = skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 0.0, 0.0, 1.0), None);
        // paint.set_stroke(true);
        // paint.set_stroke_width(1.0);
        // canvas.draw_rect(damage_rect, &paint);

        self.engine.clear_damage();
        Ok(())
    }
}

