use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
};

use layers::{prelude::*, types::Size};
use skia_safe::canvas::SrcRectConstraint;
use smithay::{
    backend::{renderer::utils::{RendererSurfaceStateUserData, SurfaceView, CommitCounter}, egl::surface}, desktop::space::SpaceElement,
    reexports::wayland_server::Resource, wayland::compositor::{self, with_states},
};

use crate::{shell::WindowElement, skia_renderer::SkiaRenderer};

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

pub struct BackgroundViewState {
    pub image: Option<skia_safe::Image>,
}
impl Hash for BackgroundViewState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(image) = self.image.as_ref() {
            image.unique_id().hash(state);
        }
    }
}
pub fn view_background(state: &BackgroundViewState) -> ViewLayer {
    let image = state.image.clone();
    let draw_container = move |canvas: &mut skia_safe::Canvas, w, h| {
        let color = skia_safe::Color4f::new(1.0, 1.0, 0.0, 1.0);
        let mut paint = skia_safe::Paint::new(color, None);

        let rrect =
            skia_safe::RRect::new_rect_xy(skia_safe::Rect::from_xywh(0.0, 0.0, w, h), 20.0, 20.0);

        if let Some(image) = image.as_ref() {
            let mut matrix = skia_safe::Matrix::new_identity();
            matrix.set_scale((w / image.width() as f32, h / image.height() as f32), None);
            paint.set_shader(image.to_shader(
                (skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat),
                skia_safe::SamplingOptions::default(),
                &matrix
            ));
        }
        canvas.draw_rrect(rrect, &paint);
    };

    ViewLayerBuilder::default()
        .id("background_view")
        .opacity((1.0, Some(Transition::default())))
        .content(Some(draw_container))
        .build()
        .unwrap()
}
