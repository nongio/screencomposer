use std::{
    borrow::{Borrow, BorrowMut},
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use layers::{
    engine::Engine,
    prelude::{Layer, LayersEngine},
    renderer::skia_fbo::SkiaFboRenderer,
};
use skia_safe::{
    gpu::{DirectContext, RecordingContext},
    Color4f, Paint, Rect,
};
use smithay::{
    backend::allocator::{dmabuf::Dmabuf, Buffer as AllocatorBuffer, Format, Fourcc},
    backend::{
        egl,
        egl::{display::EGLBufferReader, EGLContext},
        renderer::{
            gles::{ffi, Capability, GlesError, GlesFrame, GlesMapping, GlesRenderer, GlesTexture},
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
    engine: LayersEngine,
    skia_fbo: HashMap<i32, SkiaFboRenderer>,
}

impl From<GlesRenderer> for LayersRenderer {
    fn from(mut renderer: GlesRenderer) -> LayersRenderer {
        let engine = LayersEngine::new();
        LayersRenderer {
            gl: renderer,
            engine,
            skia_fbo: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct LayersTexture {
    texture: GlesTexture,
    image: skia_safe::Image,
}

#[derive(Debug)]
pub struct LayersFrame<'a> {
    // frame: GlesFrame<'a>,
    renderer: &'a mut LayersRenderer,
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
        let engine = LayersEngine::new();

        Ok(LayersRenderer {
            gl,
            engine,
            skia_fbo: HashMap::new(),
        })
    }
    pub unsafe fn new(context: EGLContext) -> Result<LayersRenderer, GlesError> {
        let supported_capabilities = Self::supported_capabilities(&context)?;
        Self::with_capabilities(context, supported_capabilities)
    }
    pub fn egl_context(&self) -> &EGLContext {
        self.gl.egl_context()
    }
    pub fn skia_renderer(&mut self) -> &mut SkiaFboRenderer {
        let mut fbo = 0;
        unsafe {
            self.gl.gl.GetIntegerv(ffi::FRAMEBUFFER_BINDING, &mut fbo);
        }
        self.skia_fbo.get_mut(&fbo).unwrap()
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
        self.renderer.id()
    }
    fn clear(
        &mut self,
        color: [f32; 4],
        rect: &[Rectangle<i32, Physical>],
    ) -> Result<(), Self::Error> {
        let skia = self.renderer.skia_renderer();
        let mut surface = skia.surface();
        let canvas = surface.canvas();
        let color = skia_safe::Color4f::new(color[0], color[1], color[2], color[3]);

        canvas.clear(color);
        Ok(())
    }
    fn draw_solid(
        &mut self,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        color: [f32; 4],
    ) -> Result<(), Self::Error> {
        let skia = self.renderer.skia_renderer();
        let mut surface = skia.surface();
        let canvas = surface.canvas();
        let color = skia_safe::Color4f::new(color[0], color[1], color[2], color[3]);
        let paint = Paint::new(color, None);
        let rect = Rect::from_xywh(
            dst.loc.x as f32,
            dst.loc.y as f32,
            dst.size.w as f32,
            dst.size.h as f32,
        );
        canvas.draw_rect(rect, &paint);
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
        // tracing::debug!("TODO render_texture_from_to");

        let skia = self.renderer.skia_renderer();
        let mut surface = skia.surface();
        let canvas = surface.canvas();
        let mut paint: Paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(skia_safe::Color::WHITE);
        let src = Rect::from_xywh(
            src.loc.x as f32,
            src.loc.y as f32,
            src.size.w as f32,
            src.size.h as f32,
        );
        let dst = Rect::from_xywh(
            dst.loc.x as f32,
            dst.loc.y as f32,
            dst.size.w as f32,
            dst.size.h as f32,
        );
        canvas.draw_image_rect(&texture.image, None, dst, &paint);
        // canvas.draw_circle((dst.loc.x, dst.loc.y), dst.size.w as f32, &paint);
        // let rect = Rect::from_xywh(
        //     dst.loc.x as f32,
        //     dst.loc.y as f32,
        //     dst.size.w as f32,
        //     dst.size.h as f32,
        // );
        // canvas.draw_rect(rect, &paint);

        Ok(())
    }
    fn transformation(&self) -> Transform {
        Transform::Normal
    }
    fn finish(self) -> Result<SyncPoint, Self::Error> {
        // self.frame.finish()
        // let mut surface = self.skia_frame.surface();
        let skia = self.renderer.skia_renderer();
        let mut surface = skia.surface();
        surface.flush_and_submit();

        Ok(SyncPoint::default())
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
        self.gl.render(output_size, dst_transform)?;

        let mut fbo = 0;
        unsafe {
            self.gl.gl.GetIntegerv(ffi::FRAMEBUFFER_BINDING, &mut fbo);
        }
        // let target = self.gl.target.as_ref().unwrap();
        if !self.skia_fbo.contains_key(&fbo) {
            self.skia_fbo.insert(
                fbo,
                SkiaFboRenderer::new(output_size.w, output_size.h, 0_usize, 8_usize, fbo as u32),
            );
        }

        Ok(LayersFrame {
            // frame,
            renderer: self,
        })
    }
}
impl LayersRenderer {
    fn import_into_skia_image(&mut self, texture: &GlesTexture) -> Option<skia_safe::Image> {
        let skia = self.skia_renderer();
        let mut context = skia.surface().recording_context().unwrap();

        let target = gl_rs::TEXTURE_2D;

        let size = layers::prelude::Point {
            x: texture.width() as f32,
            y: texture.height() as f32,
        };
        unsafe {
            let texture_info = skia_safe::gpu::gl::TextureInfo {
                target,
                id: texture.tex_id(),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
            };

            let texture = skia_safe::gpu::BackendTexture::new_gl(
                (size.x as i32, size.y as i32),
                skia_safe::gpu::MipMapped::No,
                texture_info,
            );

            skia_safe::Image::from_texture(
                &mut context,
                &texture,
                skia_safe::gpu::SurfaceOrigin::TopLeft,
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Premul,
                None,
            )
        }
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
        let image = self
            .import_into_skia_image(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;
        Ok(LayersTexture { texture, image })
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
        let image = self
            .import_into_skia_image(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;
        Ok(LayersTexture { texture, image })
    }
}

impl ImportDma for LayersRenderer {
    fn import_dmabuf(
        &mut self,
        dmabuf: &Dmabuf,
        damage: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self.gl.import_dmabuf(dmabuf, damage)?;
        let image = self
            .import_into_skia_image(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;
        Ok(LayersTexture { texture, image })
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
        let image = self
            .import_into_skia_image(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;
        Ok(LayersTexture { texture, image })
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

// impl<'frame> Borrow<GlesFrame<'frame>> for LayersFrame<'frame> {
//     fn borrow(&self) -> &GlesFrame<'frame> {
//         &self.frame
//     }
// }

// impl<'frame> BorrowMut<GlesFrame<'frame>> for LayersFrame<'frame> {
//     fn borrow_mut(&mut self) -> &mut GlesFrame<'frame> {
//         &mut self.frame
//     }
// }

impl ExportMem for LayersRenderer {
    type TextureMapping = GlesMapping;

    // Copies a region of the framebuffer into a texture and returns a TextureMapping
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
        // let frame = self.gl.render(output_size, dst_transform)?;
        // let sample_count = 0_usize; //pixel_format.multisampling.unwrap_or(0);
        // let stencil_bits = 8_usize; //pixel_format.stencil_bits;
        // let skia_frame = SkiaFboRenderer::new(size.w, size.h, sample_count, stencil_bits, 0_u32);
        self.gl.create_buffer(format, size)
    }
}
