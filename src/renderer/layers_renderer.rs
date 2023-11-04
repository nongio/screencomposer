use std::{
    borrow::{Borrow, BorrowMut},
    collections::HashSet,
};

use smithay::{
    backend::allocator::{dmabuf::Dmabuf, Format, Fourcc},
    backend::{
        egl,
        egl::{display::EGLBufferReader, EGLContext},
        renderer::{
            gles::{Capability, GlesError, GlesFrame, GlesMapping, GlesRenderer, GlesTexture},
            sync::SyncPoint,
            Bind, DebugFlags, ExportMem, Frame, ImportDma, ImportDmaWl, ImportEgl, ImportMem,
            ImportMemWl, Offscreen, Renderer, Texture, TextureFilter, Unbind,
        },
    },
    reexports::wayland_server::{protocol::wl_buffer::WlBuffer, DisplayHandle},
    utils::{Buffer, Physical, Rectangle, Size, Transform},
    wayland::compositor::SurfaceData,
};

#[derive(Debug)]
pub struct LayersRenderer {
    gl: GlesRenderer,
}

impl From<GlesRenderer> for LayersRenderer {
    fn from(mut renderer: GlesRenderer) -> LayersRenderer {
        LayersRenderer { gl: renderer }
    }
}

#[derive(Debug)]
pub struct LayersTexture {
    texture: GlesTexture,
}

#[derive(Debug)]
pub struct LayersFrame<'a> {
    frame: GlesFrame<'a>,
}

impl LayersRenderer {
    pub unsafe fn supported_capabilities(
        context: &EGLContext,
    ) -> Result<Vec<Capability>, GlesError> {
        GlesRenderer::supported_capabilities(context)
    }
    pub unsafe fn with_capabilities(
        context: EGLContext,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Result<LayersRenderer, GlesError> {
        let skia = {
            context.make_current()?;
        };
        let gl = GlesRenderer::with_capabilities(context, capabilities)?;

        Ok(LayersRenderer { gl })
    }
    pub unsafe fn new(context: EGLContext) -> Result<LayersRenderer, GlesError> {
        let supported_capabilities = Self::supported_capabilities(&context)?;
        Self::with_capabilities(context, supported_capabilities)
    }
    pub fn egl_context(&self) -> &EGLContext {
        self.gl.egl_context()
    }
}
impl Texture for LayersTexture {
    fn width(&self) -> u32 {
        self.texture.width()
    }
    fn height(&self) -> u32 {
        self.texture.height()
    }
    fn format(&self) -> Option<Fourcc> {
        self.texture.format()
    }
}

impl<'a> Frame for LayersFrame<'a> {
    type Error = GlesError;
    type TextureId = LayersTexture;

    fn id(&self) -> usize {
        self.frame.id()
    }
    fn clear(
        &mut self,
        color: [f32; 4],
        rect: &[Rectangle<i32, Physical>],
    ) -> Result<(), Self::Error> {
        self.frame.clear(color, rect)
    }
    fn draw_solid(
        &mut self,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        color: [f32; 4],
    ) -> Result<(), Self::Error> {
        tracing::debug!("TODO draw_solid");
        Ok(())
    }
    fn render_texture_from_to(
        &mut self,
        texture: &Self::TextureId,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        src_transform: Transform,
        alpha: f32,
    ) -> Result<(), Self::Error> {
        tracing::debug!("TODO render_texture_from_to");
        Frame::render_texture_from_to(
            &mut self.frame,
            &texture.texture,
            src,
            dst,
            damage,
            src_transform,
            alpha,
        )
    }
    fn transformation(&self) -> Transform {
        Transform::Normal
    }
    fn finish(self) -> Result<SyncPoint, Self::Error> {
        self.frame.finish()
    }
}

impl Renderer for LayersRenderer {
    type Error = GlesError;
    type TextureId = LayersTexture;
    type Frame<'a> = LayersFrame<'a>;

    fn id(&self) -> usize {
        self.gl.id()
    }
    fn downscale_filter(&mut self, filter: TextureFilter) -> Result<(), Self::Error> {
        self.gl.downscale_filter(filter)
    }
    fn upscale_filter(&mut self, filter: TextureFilter) -> Result<(), Self::Error> {
        self.gl.upscale_filter(filter)
    }
    fn set_debug_flags(&mut self, flags: DebugFlags) {
        self.gl.set_debug_flags(flags)
    }
    fn debug_flags(&self) -> DebugFlags {
        self.gl.debug_flags()
    }
    fn render(
        &mut self,
        output_size: Size<i32, Physical>,
        dst_transform: Transform,
    ) -> Result<Self::Frame<'_>, Self::Error> {
        // unsafe { self.egl_context().make_current()? };

        let frame = self.gl.render(output_size, dst_transform)?;

        Ok(LayersFrame { frame })
    }
}

impl ImportMemWl for LayersRenderer {
    fn import_shm_buffer(
        &mut self,
        buffer: &WlBuffer,
        surface: Option<&SurfaceData>,
        damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self.gl.import_shm_buffer(buffer, surface, damage)?;
        Ok(LayersTexture { texture })
    }
    fn shm_formats(
        &self,
    ) -> Box<dyn Iterator<Item = smithay::reexports::wayland_server::protocol::wl_shm::Format>>
    {
        self.gl.shm_formats()
    }
}

impl ImportEgl for LayersRenderer {
    fn bind_wl_display(&mut self, display: &DisplayHandle) -> Result<(), egl::Error> {
        self.gl.bind_wl_display(display)
    }

    fn unbind_wl_display(&mut self) {
        self.gl.unbind_wl_display()
    }

    fn egl_reader(&self) -> Option<&EGLBufferReader> {
        self.gl.egl_reader()
    }

    fn import_egl_buffer(
        &mut self,
        buffer: &WlBuffer,
        surface: Option<&SurfaceData>,
        damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self.gl.import_egl_buffer(buffer, surface, damage)?;
        Ok(LayersTexture { texture })
    }
}

impl ImportDma for LayersRenderer {
    fn import_dmabuf(
        &mut self,
        dmabuf: &Dmabuf,
        damage: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self.gl.import_dmabuf(dmabuf, damage)?;
        Ok(LayersTexture { texture })
    }
}

impl ImportDmaWl for LayersRenderer {}

impl ImportMem for LayersRenderer {
    fn import_memory(
        &mut self,
        data: &[u8],
        format: Fourcc,
        size: Size<i32, Buffer>,
        flipped: bool,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self.gl.import_memory(data, format, size, flipped)?;
        Ok(LayersTexture { texture })
    }
    fn mem_formats(&self) -> Box<dyn Iterator<Item = Fourcc>> {
        self.gl.mem_formats()
    }
    fn update_memory(
        &mut self,
        texture: &<Self as Renderer>::TextureId,
        data: &[u8],
        region: Rectangle<i32, Buffer>,
    ) -> Result<(), <Self as Renderer>::Error> {
        self.gl.update_memory(&texture.texture, data, region)
    }
}

impl Borrow<GlesRenderer> for LayersRenderer {
    fn borrow(&self) -> &GlesRenderer {
        &self.gl
    }
}

impl BorrowMut<GlesRenderer> for LayersRenderer {
    fn borrow_mut(&mut self) -> &mut GlesRenderer {
        &mut self.gl
    }
}

impl<'frame> Borrow<GlesFrame<'frame>> for LayersFrame<'frame> {
    fn borrow(&self) -> &GlesFrame<'frame> {
        &self.frame
    }
}

impl<'frame> BorrowMut<GlesFrame<'frame>> for LayersFrame<'frame> {
    fn borrow_mut(&mut self) -> &mut GlesFrame<'frame> {
        &mut self.frame
    }
}

impl ExportMem for LayersRenderer {
    type TextureMapping = GlesMapping;
    fn copy_framebuffer(
        &mut self,
        region: Rectangle<i32, Buffer>,
        format: Fourcc,
    ) -> Result<Self::TextureMapping, <Self as Renderer>::Error> {
        self.gl.copy_framebuffer(region, format)
    }
    fn copy_texture(
        &mut self,
        texture: &Self::TextureId,
        region: Rectangle<i32, Buffer>,
        format: Fourcc,
    ) -> Result<Self::TextureMapping, Self::Error> {
        self.gl.copy_texture(&texture.texture, region, format)
    }
    fn map_texture<'a>(
        &mut self,
        texture_mapping: &'a Self::TextureMapping,
    ) -> Result<&'a [u8], <Self as Renderer>::Error> {
        self.gl.map_texture(texture_mapping)
    }
}

impl<T> Bind<T> for LayersRenderer
where
    GlesRenderer: Bind<T>,
{
    #[profiling::function]
    fn bind(&mut self, target: T) -> Result<(), GlesError> {
        self.gl.bind(target)
    }
    fn supported_formats(&self) -> Option<HashSet<Format>> {
        self.gl.supported_formats()
    }
}

impl Unbind for LayersRenderer {
    fn unbind(&mut self) -> Result<(), <Self as Renderer>::Error> {
        self.gl.unbind()
    }
}

impl<T> Offscreen<T> for LayersRenderer
where
    GlesRenderer: Offscreen<T>,
{
    #[profiling::function]
    fn create_buffer(&mut self, format: Fourcc, size: Size<i32, Buffer>) -> Result<T, GlesError> {
        self.gl.create_buffer(format, size)
    }
}
