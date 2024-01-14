use core::fmt;
use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
};

use layers::{prelude::*, types::Size};
use smithay::{
    backend::{
        renderer::{
            element::{Element, RenderElement},
            utils::{CommitCounter, RendererSurfaceStateUserData, SurfaceView},
        },
    },
    desktop::space::SpaceElement,
    reexports::wayland_server::{backend::ObjectId, Resource},
    utils::Transform,
    wayland::compositor::{self, with_states},
};

use crate::{
    shell::WindowElement,
};

struct FontCache {
    font_collection: skia_safe::textlayout::FontCollection,
    font_mgr: skia_safe::FontMgr,
    type_face_font_provider: RefCell<skia_safe::textlayout::TypefaceFontProvider>,
}

// source: slint ui
// https://github.com/slint-ui/slint/blob/64e7bb27d12dd8f884275292c2333d37f4e224d5/internal/renderers/skia/textlayout.rs#L31
thread_local! {
    static FONT_CACHE: FontCache = {
        let font_mgr = skia_safe::FontMgr::new();
        let type_face_font_provider = skia_safe::textlayout::TypefaceFontProvider::new();
        let mut font_collection = skia_safe::textlayout::FontCollection::new();
        font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
        font_collection.set_dynamic_font_manager(font_mgr.clone());
        FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
    };
}

#[derive(Clone)]
pub struct WindowViewSurface {
    pub(crate) id: ObjectId,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) offset_x: f32,
    pub(crate) offset_y: f32,
    pub(crate) image: Option<skia_safe::Image>,
    pub(crate) commit: CommitCounter,
    pub(crate) transform: Transform,
}
impl fmt::Debug for WindowViewSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowViewSurface")
            .field("id", &self.id)
            .field("x", &self.x)
            .field("y", &self.y)
            .field("w", &self.w)
            .field("h", &self.h)
            .field("offset_x", &self.offset_x)
            .field("offset_y", &self.offset_y)
            .field("commit", &self.commit)
            .field("transform", &self.transform)
            .finish()
    }
}
pub struct WindowViewState {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub window_element: Option<WindowElement>,
    pub render_elements: Vec<WindowViewSurface>,
    pub title: String,
}
impl Hash for WindowViewState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // self.x.to_bits().hash(state);
        // self.y.to_bits().hash(state);
        self.w.to_bits().hash(state);
        self.h.to_bits().hash(state);

        let id = self
            .window_element
            .as_ref()
            .and_then(|we| we.wl_surface().map(|s| s.id()));
        id.hash(state);

        for wvs in self.render_elements.iter() {
            wvs.id.hash(state);
            let distance = wvs.commit.distance(Some(CommitCounter::default())).unwrap_or(0);
            if let Some(image) = wvs.image.as_ref() {
                image.unique_id().hash(state);
                distance.hash(state);
            }
            // println!("distance: {:?}", distance);
            wvs.x.to_bits().hash(state);
            wvs.y.to_bits().hash(state);
            wvs.w.to_bits().hash(state);
            wvs.h.to_bits().hash(state);
            wvs.offset_x.to_bits().hash(state);
            wvs.offset_y.to_bits().hash(state);
        }
    }
}
#[profiling::function]
pub fn view_window(state: &WindowViewState) -> ViewLayer {
    // let mut x = state.x as f32;
    // let mut y = state.y as f32;
    let mut w = 0.0;
    let mut h = 0.0;
    if let Some(re) = state.window_element.as_ref() {
        let bbox = re.bbox();
        let geometry = re.geometry();

        w = geometry.size.w as f32;
        h = geometry.size.h as f32;
    }
    let render_elements = state.render_elements.clone();
    let draw_container = move |canvas: &mut skia_safe::Canvas, w, h| {
        // let color = skia_safe::Color4f::new(1.0, 0.0, 0.0, 1.0);
        // let mut stroke_paint = skia_safe::Paint::new(color, None);
        // stroke_paint.set_stroke(true);
        // stroke_paint.set_anti_alias(true);
        let window_corner_radius = 12.0;
        let rrect =
            skia_safe::RRect::new_rect_xy(skia_safe::Rect::from_xywh(0.0, 0.0, w, h), window_corner_radius, window_corner_radius);
        // canvas.draw_rrect(rrect, &stroke_paint);

        for wvs in render_elements.iter() {
            if let Some(image) = wvs.image.as_ref() {
                let mut matrix = skia_safe::Matrix::new_identity();
                let offset_x = wvs.offset_x;
                let offset_y = wvs.offset_y;

                match wvs.transform {
                    Transform::Normal => {
                        matrix.pre_translate(((wvs.x - wvs.offset_x), (wvs.y - wvs.offset_x)));
                    }
                    Transform::Flipped180 => {
                        matrix.pre_scale((1.0, -1.0), None);
                        matrix.pre_translate(((wvs.x - wvs.offset_x), (-wvs.y + wvs.offset_y)));
                    }
                    Transform::_90 => {}
                    Transform::_180 => {}
                    Transform::_270 => {}
                    Transform::Flipped => {}
                    Transform::Flipped90 => {}
                    Transform::Flipped270 => {}
                }

                let mut paint =
                    skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 1.0, 1.0, 1.0), None);
                paint.set_blend_mode(skia_safe::BlendMode::SrcOver);
                paint.set_shader(image.to_shader(
                    (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                    skia_safe::SamplingOptions::default(),
                    &matrix,
                ));
                let rrect = skia_safe::RRect::new_rect_xy(
                    skia_safe::Rect::from_xywh(wvs.x, wvs.y, wvs.w, wvs.h),
                    window_corner_radius,
                    window_corner_radius,
                );

                canvas.draw_rrect(rrect, &paint);
            }
        }
        // draw shadow
        let mut shadow_paint =
        skia_safe::Paint::new(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.25), None);
        shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, 3.0, false));
        canvas.clip_rrect(rrect, Some(skia_safe::ClipOp::Difference), Some(true));
        canvas.draw_rrect(rrect, &shadow_paint);

        let rrect = skia_safe::RRect::new_rect_xy(skia_safe::Rect::from_xywh(0.0, 36.0, w, h), 20.0, 20.0);
        shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, 50.0, false));
        shadow_paint.set_color4f(skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.7), None);
        canvas.draw_rrect(rrect, &shadow_paint);
    };

    ViewLayerBuilder::default()
        .id("window_view")
        // .position((Point { x, y }, None))
        .size((
            Size {
                width: taffy::Dimension::Points(w),
                height: taffy::Dimension::Points(h),
            },
            None,
        ))
        .content(Some(draw_container))
        .build()
        .unwrap()
}
