use std::{cell::RefCell, rc::Rc};

use lay_rs::{
    drawing::render_node_tree,
    engine::{LayersEngine, SceneNode},
};

use smithay::{
    backend::renderer::{
        element::{Element, Id, RenderElement},
        utils::{CommitCounter, DamageBag},
        Renderer,
    },
    utils::{Buffer, Physical, Point, Rectangle, Scale},
};

use crate::{skia_renderer::SkiaRenderer, udev::UdevRenderer};

#[derive(Clone)]
pub struct SceneElement {
    id: Id,
    commit_counter: CommitCounter,
    engine: LayersEngine,
    last_update: std::time::Instant,
    pub size: (f32, f32),
    damage: Rc<RefCell<DamageBag<i32, Physical>>>,
}

impl SceneElement {
    pub fn with_engine(engine: LayersEngine) -> Self {
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            engine,
            last_update: std::time::Instant::now(),
            size: (0.0, 0.0),
            damage: Rc::new(RefCell::new(DamageBag::new(5))),
        }
    }
    #[profiling::function]
    pub fn update(&mut self) {
        let dt = self.last_update.elapsed().as_secs_f32();
        if dt <= 0.01 {
            return;
        }
        self.last_update = std::time::Instant::now();
        if self.engine.update(dt) {
            self.commit_counter.increment();
            let scene_damage = self.engine.damage();
            if !scene_damage.is_empty() {
                self.commit_counter.increment();
                let safe = 0;
                let damage = Rectangle::from_loc_and_size(
                    (
                        scene_damage.x() as i32 - safe,
                        scene_damage.y() as i32 - safe,
                    ),
                    (
                        scene_damage.width() as i32 + safe * 2,
                        scene_damage.height() as i32 + safe * 2,
                    ),
                );
                self.damage.borrow_mut().add(vec![damage]);
            }
        }
    }
    pub fn root_layer(&self) -> Option<SceneNode> {
        self.engine.scene_root_layer()
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
            (bounds.x() as i32, bounds.y() as i32).into()
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
            Rectangle::from_loc_and_size(
                self.location(scale),
                (bounds.width() as i32, bounds.height() as i32),
            )
        } else {
            Rectangle::from_loc_and_size(self.location(scale), (0, 0))
        }
    }

    fn current_commit(&self) -> CommitCounter {
        self.damage.borrow().current_commit()
    }
    /// Get the damage since the provided commit relative to the element
    fn damage_since(
        &self,
        _scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> smithay::backend::renderer::utils::DamageSet<i32, Physical> {
        self.damage
            .borrow()
            .damage_since(commit)
            .unwrap_or_default()
    }
    fn alpha(&self) -> f32 {
        1.0
    }
}

impl<'renderer> RenderElement<UdevRenderer<'renderer>> for SceneElement {
    fn draw(
        &self,
        frame: &mut <UdevRenderer<'renderer> as Renderer>::Frame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), <UdevRenderer<'renderer> as Renderer>::Error> {
        RenderElement::<SkiaRenderer>::draw(self, frame.as_mut(), src, dst, damage, opaque_regions)
            .map_err(|e| e.into())
    }
}

impl RenderElement<SkiaRenderer> for SceneElement {
    fn draw(
        &self,
        frame: &mut <SkiaRenderer as Renderer>::Frame<'_>,
        _src: Rectangle<f64, Buffer>,
        _dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), <SkiaRenderer as Renderer>::Error> {
        #[cfg(feature = "profile-with-puffin")]
        profiling::puffin::profile_scope!("render_scene");
        let mut surface = frame.skia_surface.clone();

        let canvas = surface.canvas();

        // if self.engine.damage().is_empty() {
        // return Ok(());
        // } else {
        // println!("scene damage: {:?}", self.engine.damage());
        // }
        // println!("scene draw");
        // canvas.clear(lay_rs::skia::Color::from_argb(255, 0, 0, 0));

        let scene = self.engine.scene();
        let root_id = self.engine.scene_root();

        scene.with_arena(|arena| {
            // let scene_damage = self.engine.damage();
            // let damage_rect = lay_rs::skia::Rect::from_xywh(scene_damage.x, scene_damage.y, scene_damage.width, scene_damage.height);
            let mut damage_rect = lay_rs::skia::Rect::default();
            damage.iter().for_each(|d| {
                damage_rect.join(lay_rs::skia::Rect::from_xywh(
                    d.loc.x as f32,
                    d.loc.y as f32,
                    d.size.w as f32,
                    d.size.h as f32,
                ));
            });
            if let Some(root_id) = root_id {
                let save_point = canvas.save();

                // canvas.clear(lay_rs::skia::Color::TRANSPARENT);
                canvas.clip_rect(damage_rect, None, None);
                render_node_tree(root_id, arena, canvas, 1.0);

                // draw damage rect
                let mut paint = lay_rs::skia::Paint::default();
                paint.set_color(lay_rs::skia::Color::from_argb(255, 255, 0, 0));
                // paint.set_stroke(true);
                // paint.set_stroke_width(5.0);
                // // damage.iter().for_each(|d| {
                // //     canvas.draw_rect(lay_rs::skia::Rect::from_xywh(
                // //         d.loc.x as f32,
                // //         d.loc.y as f32,
                // //         d.size.w as f32,
                // //         d.size.h as f32,
                // //     ), &paint);
                // // });
                // canvas.draw_rect(damage_rect, &paint);
                // let typeface = crate::workspace::utils::FONT_CACHE
                // .with(|font_cache| {
                //     font_cache
                //         .font_mgr
                //         .match_family_style("Inter", lay_rs::skia::FontStyle::default())
                // })
                // .unwrap();
                // let font = lay_rs::skia::Font::from_typeface_with_params(typeface, 22.0, 1.0, 0.0);
                // let pos = self.engine.get_pointer_position();
                // canvas.draw_str(format!("{},{}", pos.x, pos.y), (50.0, 50.0), &font, &paint);
                // println!("scene draw damage: {},{} {}x{}", damage_rect.x(), damage_rect.y(), damage_rect.width(), damage_rect.height());

                canvas.restore_to_count(save_point);
            }
            self.engine.clear_damage();
        });
        Ok(())
    }
}
