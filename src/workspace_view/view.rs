use std::hash::{Hash, Hasher};
use layers::prelude::*;

// struct FontCache {
//     font_collection: skia_safe::textlayout::FontCollection,
//     font_mgr: skia_safe::FontMgr,
//     type_face_font_provider: RefCell<skia_safe::textlayout::TypefaceFontProvider>,
// }

// // source: slint ui
// // https://github.com/slint-ui/slint/blob/64e7bb27d12dd8f884275292c2333d37f4e224d5/internal/renderers/skia/textlayout.rs#L31
// thread_local! {
//     static FONT_CACHE: FontCache = {
//         let font_mgr = skia_safe::FontMgr::new();
//         let type_face_font_provider = skia_safe::textlayout::TypefaceFontProvider::new();
//         let mut font_collection = skia_safe::textlayout::FontCollection::new();
//         font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
//         font_collection.set_dynamic_font_manager(font_mgr.clone());
//         FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
//     };
// }

pub struct WorkspaceViewState {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub image: Option<skia_safe::Image>,
}
impl Hash for WorkspaceViewState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.x.hash(state);
        self.y.hash(state);
    }
}
pub fn view_workspace(_state: &WorkspaceViewState) -> ViewLayer {
    let draw_container = move |canvas: &mut skia_safe::Canvas, w, h| {
        let color = skia_safe::Color4f::new(1.0, 1.0, 1.0, 1.0);
        let paint = skia_safe::Paint::new(color, None);

        let rrect =
            skia_safe::RRect::new_rect_xy(skia_safe::Rect::from_xywh(0.0, 0.0, w, h), 20.0, 20.0);
        canvas.draw_rrect(rrect, &paint);

        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
    };

    ViewLayerBuilder::default()
        .id("workspace_view")
        .content(Some(draw_container))
        // .border_corner_radius((BorderRadius::new_single(50.0), None))
        .build()
        .unwrap()
}
