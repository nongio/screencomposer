use core::fmt;
use std::hash::{Hash, Hasher};
use smithay::{
    backend::renderer::utils::CommitCounter,
    reexports::wayland_server::backend::ObjectId,
    utils::Transform,
};
use crate::skia_renderer::SkiaTexture;

#[derive(Clone)]
pub struct WindowViewSurface {
    pub(crate) id: ObjectId,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) offset_x: f32,
    pub(crate) offset_y: f32,
    pub(crate) texture: Option<SkiaTexture>,
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

#[derive(Clone)]
pub struct WindowViewBaseModel {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub title: String,
}

impl Hash for WindowViewBaseModel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.w.to_bits().hash(state);
        self.h.to_bits().hash(state);
    }
}
impl Hash for WindowViewSurface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let distance = self
            .commit
            .distance(Some(CommitCounter::default()))
            .unwrap_or(0);
        if let Some(image) = self.texture.as_ref().map(|t| t.image.as_ref()) {
            image.unique_id().hash(state);
            distance.hash(state);
        }
        self.id.hash(state);
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
        self.w.to_bits().hash(state);
        self.h.to_bits().hash(state);
        self.offset_x.to_bits().hash(state);
        self.offset_y.to_bits().hash(state);
    }
}