
use smithay::{
    backend::renderer::{
        element::{Element, Id, RenderElement},
        utils::CommitCounter,
        Renderer,
    },
    utils::{Buffer, Physical, Point, Rectangle, Scale},
};

use crate::{skia_renderer::{SkiaRenderer, SkiaFrame}, udev::UdevRenderer};

#[derive(Debug, Clone)]
pub struct SkiaElement {
    id: Id,
    commit_counter: CommitCounter,
}

impl SkiaElement {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self) {
        self.commit_counter.increment();
    }
}

impl Default for SkiaElement {
    fn default() -> Self {
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
        }
    }
}

impl Element for SkiaElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn location(&self, _scale: Scale<f64>) -> Point<i32, Physical> {
        (100, 100).into()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_loc_and_size((0, 0), (300, 600)).to_f64()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::from_loc_and_size(self.location(scale), (300, 600))
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

impl RenderElement<SkiaRenderer> for SkiaElement {
fn draw(
        &self,
        frame: &mut <SkiaRenderer as Renderer>::Frame<'_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), <SkiaRenderer as Renderer>::Error> {
        
        let mut canvas = frame.skia_surface.clone();
        let canvas = canvas.canvas();

        let instances = damage
            .iter()
            .map(|rect| {
                let dest_size = dst.size;

                let rect_constrained_loc = rect
                    .loc
                    .constrain(Rectangle::from_extemities((0, 0), dest_size.to_point()));
                let rect_clamped_size = rect.size.clamp(
                    (0, 0),
                    (dest_size.to_point() - rect_constrained_loc).to_size(),
                );

                let rect = Rectangle::from_loc_and_size(rect_constrained_loc, rect_clamped_size);
                skia_safe::Rect::from_xywh(
                    (dst.loc.x + rect.loc.x) as f32,
                    (dst.loc.y + rect.loc.y) as f32,
                    (rect.size.w) as f32,
                    (rect.size.h) as f32,
                )
            })
            .collect::<Vec<skia_safe::Rect>>();

            
        let scale = Scale::from(1.0);
        let location = self.location(scale);
        let geometry = self.geometry(scale).size;
        let bounds = skia_safe::Rect::from_xywh(location.x as f32, location.y as f32, geometry.w as f32, geometry.h as f32);
    
        let radius = 20.0;
        let rrect = skia_safe::RRect::new_rect_radii(
            bounds,
            &[
                skia_safe::Point::new(radius, radius),
                skia_safe::Point::new(radius, radius),
                skia_safe::Point::new(radius, radius),
                skia_safe::Point::new(radius, radius),
            ],
        );
        let background_color = skia_safe::Color4f::new(0.4, 0.4, 0.4, 0.3);
        let mut background_paint = skia_safe::Paint::new(background_color, None);
        background_paint.set_anti_alias(true);
        background_paint.set_style(skia_safe::PaintStyle::Fill);
    
        let mut save_layer_rec = skia_safe::canvas::SaveLayerRec::default();
        let blur = skia_safe::image_filters::blur(
            (20.0, 20.0),
            skia_safe::TileMode::Clamp,
            None,
            Some(skia_safe::image_filters::CropRect::from(bounds)),
        )
        .unwrap();
        
        let save_count = canvas.save();
        
        save_layer_rec = save_layer_rec.backdrop(&blur).bounds(&bounds);
        canvas.save_layer(&save_layer_rec);
        background_paint.set_blend_mode(skia_safe::BlendMode::SrcOver);
        canvas.clip_rrect(rrect, None, Some(true));
        for rect in instances.iter() {

            canvas.save();
            canvas.clip_rect(rect, skia_safe::ClipOp::Intersect, Some(true));
            canvas.draw_color(background_color, skia_safe::BlendMode::SrcOver);
            canvas.restore();
        }
        canvas.restore_to_count(save_count);
        Ok(())
    }
}


impl<'renderer, 'alloc> RenderElement<UdevRenderer<'renderer, 'alloc>> for SkiaElement
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