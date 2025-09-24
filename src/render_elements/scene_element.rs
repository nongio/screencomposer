use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};

#[cfg(feature = "perf-counters")]
use std::time::Duration;

use lay_rs::{drawing::render_node_tree, engine::Engine, prelude::Layer};

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
    engine: Arc<Engine>,
    last_update: Instant,
    pub size: (f32, f32),
    damage: Rc<RefCell<DamageBag<i32, Physical>>>,
    #[cfg(feature = "perf-counters")]
    perf_stats: Rc<RefCell<ScenePerfStats>>,
}

impl SceneElement {
    pub fn with_engine(engine: Arc<Engine>) -> Self {
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            engine,
            last_update: Instant::now(),
            size: (0.0, 0.0),
            damage: Rc::new(RefCell::new(DamageBag::new(5))),
            #[cfg(feature = "perf-counters")]
            perf_stats: Rc::new(RefCell::new(ScenePerfStats::new())),
        }
    }
    #[profiling::function]
    pub fn update(&mut self) -> bool {
        let dt = self.last_update.elapsed().as_secs_f32();
        if dt <= 0.01 {
            return false;
        }
        self.last_update = Instant::now();

        #[cfg(feature = "perf-counters")]
        let mut stats = self.perf_stats.borrow_mut();
        #[cfg(feature = "perf-counters")]
        {
            stats.total_updates += 1;
        }

        let updated = self.engine.update(dt);
        if !updated {
            #[cfg(feature = "perf-counters")]
            stats.log_if_due();
            return false;
        }

        #[cfg(feature = "perf-counters")]
        {
            stats.updates_with_changes += 1;
        }

        self.commit_counter.increment();
        let scene_damage = self.engine.damage();
        let has_damage = !scene_damage.is_empty();

        #[cfg(feature = "perf-counters")]
        {
            if has_damage {
                stats.updates_with_damage += 1;
            }
            stats.log_if_due();
        }

        if has_damage {
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

        has_damage
    }
    pub fn root_layer(&self) -> Option<Layer> {
        self.engine
            .scene_root()
            .map(|id| self.engine.get_layer(&id))
            .flatten()
    }
    pub fn set_size(&mut self, width: f32, height: f32) {
        self.engine.scene_set_size(width, height);
        self.size = (width, height);
    }
}

#[cfg(feature = "perf-counters")]
#[derive(Debug)]
struct ScenePerfStats {
    total_updates: u64,
    updates_with_changes: u64,
    updates_with_damage: u64,
    last_log: Instant,
    prev_logged_updates: u64,
    prev_logged_changes: u64,
    prev_logged_damage: u64,
}

#[cfg(feature = "perf-counters")]
impl ScenePerfStats {
    fn new() -> Self {
        Self {
            total_updates: 0,
            updates_with_changes: 0,
            updates_with_damage: 0,
            last_log: Instant::now(),
            prev_logged_updates: 0,
            prev_logged_changes: 0,
            prev_logged_damage: 0,
        }
    }

    fn log_if_due(&mut self) {
        if self.last_log.elapsed() < Duration::from_secs(1) {
            return;
        }

        let delta_updates = self.total_updates - self.prev_logged_updates;
        let delta_changes = self.updates_with_changes - self.prev_logged_changes;
        let delta_damage = self.updates_with_damage - self.prev_logged_damage;
        let delta_no_change = delta_updates.saturating_sub(delta_changes);

        tracing::info!(
            target: "screen_composer::perf.scene",
            total_updates = self.total_updates,
            updates_per_sec = delta_updates,
            updates_with_scene_changes = delta_changes,
            updates_with_damage = delta_damage,
            updates_without_changes = delta_no_change,
        );

        self.prev_logged_updates = self.total_updates;
        self.prev_logged_changes = self.updates_with_changes;
        self.prev_logged_damage = self.updates_with_damage;
        self.last_log = Instant::now();
    }
}

impl Element for SceneElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn location(&self, _scale: Scale<f64>) -> Point<i32, Physical> {
        if let Some(root) = self.root_layer() {
            let bounds = root.render_bounds_transformed();
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
            let bounds = root.render_bounds_transformed();
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
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), <SkiaRenderer as Renderer>::Error> {
        #[cfg(feature = "profile-with-puffin")]
        profiling::puffin::profile_scope!("render_scene");
        let mut surface = frame.skia_surface.clone();

        let canvas = surface.canvas();

        let scene = self.engine.scene();
        let root_id = self.engine.scene_root();
        let save_point = canvas.save();

        scene.with_arena(|arena| {
            scene.with_renderable_arena(|renderable_arena| {
                // Clip drawing to the damaged region to avoid full-scene redraws
                let mut damage_rect = lay_rs::skia::Rect::default();
                for d in damage.iter() {
                    let r = lay_rs::skia::Rect::from_xywh(
                        d.loc.x as f32,
                        d.loc.y as f32,
                        d.size.w as f32,
                        d.size.h as f32,
                    );
                    damage_rect.join(r);
                }
                // if !damage_rect.is_empty() {
                //     canvas.clip_rect(damage_rect, Some(lay_rs::skia::ClipOp::Intersect), None);
                // }
                if let Some(root_id) = root_id {
                    // canvas.clear(lay_rs::skia::Color::TRANSPARENT);
                    // canvas.clip_rect(damage_rect, None, None);
                    render_node_tree(root_id, arena, renderable_arena, canvas, 1.0);
                    // Optional debug: outline damage rect
                    // let mut paint = lay_rs::skia::Paint::default();
                    // paint.set_color(lay_rs::skia::Color::from_argb(255, 255, 0, 0));
                    // paint.set_stroke(true);
                    // paint.set_stroke_width(5.0);
                    // if !damage_rect.is_empty() {
                    //     canvas.draw_rect(damage_rect, &paint);
                    // }
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
                }
                self.engine.clear_damage();
            });
        });
        canvas.restore_to_count(save_point);
        Ok(())
    }
}
