#![allow(dead_code)]

use core::marker::PhantomData;

/// Identifier for a compositor output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct OutputId(pub u64);

/// Metadata describing a rendered frame that can be tapped for screensharing.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameMeta {
    pub size: (u32, u32),
    pub stride: u32,
    pub fourcc: u32,
    pub time_ns: u64,
}

/// Placeholder for an RGBA buffer mapping.
#[derive(Debug)]
pub struct MappedImage<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<'a> MappedImage<'a> {
    pub fn empty() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Placeholder for a dmabuf handle.
#[derive(Debug)]
pub struct DmabufHandle<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<'a> DmabufHandle<'a> {
    pub fn empty() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Trait implemented by consumers interested in tapped frames.
pub trait FrameTap: Send + Sync {
    fn on_frame_rgba(&self, _out: OutputId, _buf: &MappedImage<'_>, _meta: &FrameMeta) {}
    fn on_frame_dmabuf(&self, _out: OutputId, _dmabuf: &DmabufHandle<'_>, _meta: &FrameMeta) {}
}

/// A no-op implementation used until a real tap is wired into the renderer.
#[derive(Debug, Default)]
pub struct NoopFrameTap;

impl FrameTap for NoopFrameTap {}
