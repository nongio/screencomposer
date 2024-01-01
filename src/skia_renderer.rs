use std::{
    borrow::{Borrow, BorrowMut},
    cell::RefCell,
    collections::HashMap,
    ffi::{c_char, CStr},
    rc::Rc,
};

use skia_safe as skia;

use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            Fourcc, format::has_alpha,
        },
        egl,
        egl::{display::EGLBufferReader, EGLContext, EGLSurface},
        renderer::{
            gles::{
                ffi::{self, types::{GLuint, GLint}},
                format::{fourcc_to_gl_formats, gl_internal_format_to_fourcc},
                Capability, GlesError, GlesRenderbuffer, GlesRenderer, GlesTexture,
            },
            sync::SyncPoint,
            Bind, DebugFlags, ExportMem, Frame, ImportDma, ImportDmaWl, ImportEgl, ImportMem,
            ImportMemWl, Offscreen, Renderer, Texture, TextureFilter, TextureMapping, Unbind,
        },
    },
    reexports::wayland_server::{protocol::wl_buffer::WlBuffer, DisplayHandle},
    utils::{Buffer, Physical, Rectangle, Size, Transform},
    wayland::{
        compositor::SurfaceData,
        shm::{shm_format_to_fourcc, with_buffer_contents},
    },
};

#[derive(Clone)]
pub struct SkiaSurface {
    pub gr_context: skia::gpu::DirectContext,
    pub surface: skia::Surface,
}

impl SkiaSurface {
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_fbo(
        width: impl Into<i32>,
        height: impl Into<i32>,
        sample_count: impl Into<usize>,
        stencil_bits: impl Into<usize>,
        fboid: impl Into<u32>,
        color_type: skia::ColorType,
        context: Option<&skia::gpu::DirectContext>,
        origin: skia::gpu::SurfaceOrigin,
    ) -> Self {
        let fb_info = {
            skia::gpu::gl::FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: skia::gpu::gl::Format::RGBA8.into(),
            }
        };
        let backend_render_target = skia::gpu::BackendRenderTarget::new_gl(
            (width.into(), height.into()),
            sample_count.into(),
            stencil_bits.into(),
            fb_info,
        );

        let mut gr_context: skia::gpu::DirectContext = if let Some(context) = context {
            context.clone()
        } else {
            skia::gpu::DirectContext::new_gl(None, None).unwrap()
        };
        gr_context.reset(None);
        let surface = skia::Surface::from_backend_render_target(
            &mut gr_context,
            &backend_render_target,
            origin,
            color_type,
            None,
            Some(&skia::SurfaceProps::new(
                Default::default(),
                skia::PixelGeometry::Unknown, // for font rendering optimisations
            )),
        )
        .unwrap();

        Self {
            gr_context,
            surface,
        }
    }

    pub fn surface(&self) -> skia::Surface {
        self.surface.clone()
    }
}
pub struct SkiaRenderer {
    gl_renderer: GlesRenderer,
    gl: ffi::Gles2,

    target_renderer: HashMap<SkiaTarget, SkiaSurface>,
    current_target: Option<SkiaTarget>,
    buffers: HashMap<SkiaTarget, SkiaGLesFbo>,
    context: Option<skia::gpu::DirectContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkiaGLesFbo {
    pub fbo: u32,
    pub tex_id: u32,
    pub format: Fourcc,
    pub origin: skia::gpu::SurfaceOrigin,
}

impl From<GlesRenderer> for SkiaRenderer {
    fn from(gl_renderer: GlesRenderer) -> SkiaRenderer {
        let egl = gl_renderer.egl_context();

        let gl = unsafe {
            let res = egl.make_current();
            if res.is_err() {
                panic!("Failed to make current");
            }
            SkiaRenderer::load_gl()
        };

        let mut options = skia::gpu::ContextOptions::default();
        options.skip_gl_error_checks = skia::gpu::context_options::Enable::Yes;
        // options.

        let mut context = skia::gpu::DirectContext::new_gl(None, &options);

        let ctx = context.as_mut().unwrap();
        ctx.reset(None);

        SkiaRenderer {
            gl,
            gl_renderer,
            target_renderer: HashMap::new(),
            buffers: HashMap::new(),
            current_target: None,
            context,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkiaTexture {
    pub texture: GlesTexture,
    pub image: skia::Image,
    pub has_alpha: bool,
}


#[derive(Clone)]
pub struct SkiaFrame {
    size: Size<i32, Physical>,
    pub skia_surface: skia::Surface,
    id: usize,
}

#[derive(Debug, Clone)]
pub struct SkiaTextureMapping {
    pub fourcc_format: Fourcc,
    pub flipped: bool,
    pub width: u32,
    pub height: u32,
    pub image: skia::Image,
    pub data: RefCell<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct EGLSurfaceWrapper(pub Rc<EGLSurface>);

impl PartialEq for EGLSurfaceWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.get_surface_handle() == other.0.get_surface_handle()
    }
}
impl std::cmp::Eq for EGLSurfaceWrapper {}

impl std::hash::Hash for EGLSurfaceWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.get_surface_handle().hash(state);
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SkiaTarget {
    // EGLSurface(smithay::backend::egl::ffi::egl::types::EGLSurface),
    EGLSurface(EGLSurfaceWrapper),
    Texture(ffi::types::GLuint),
    Renderbuffer(*const GlesRenderbuffer),
    Dmabuf(Dmabuf),
    Fbo(SkiaGLesFbo),
}
impl std::fmt::Debug for SkiaRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkiaRenderer")
            .finish()
    }
}

#[allow(dead_code)]
fn save_surface(surface: &mut skia::Surface, name: &str) {
    surface.flush_submit_and_sync_cpu();
    let image = surface.image_snapshot();

    save_image(&image, name);
}
#[allow(dead_code)]
fn save_image(image: &skia::Image, name: &str) {
    use std::fs::File;
    use std::io::Write;

    let data = image.encode_to_data(skia::EncodedImageFormat::PNG).unwrap();
    let bytes = data.as_bytes();
    let filename = format!("{}.png", name);
    let mut file = File::create(filename).unwrap();
    file.write_all(bytes).unwrap();
}

pub trait VectorRenderer {}
impl VectorRenderer for SkiaRenderer {}


impl SkiaRenderer {
    /// # Safety
    ///
    /// This operation will cause undefined behavior if the given EGLContext is active in another thread.
    pub unsafe fn supported_capabilities(
        context: &EGLContext,
    ) -> Result<Vec<Capability>, GlesError> {
        GlesRenderer::supported_capabilities(context)
    }

    /// # Safety
    ///
    /// This operation will cause undefined behavior if the given EGLContext is active in another thread.
    pub unsafe fn load_gl() -> ffi::Gles2 {
        ffi::Gles2::load_with(|s| smithay::backend::egl::get_proc_address(s) as *const _)
    }
    /// # Safety
    ///
    /// This operation will cause undefined behavior if the given EGLContext is active in another thread.
    pub unsafe fn with_capabilities(
        egl: EGLContext,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Result<SkiaRenderer, GlesError> {
        egl.make_current()?;
        let gl_renderer = GlesRenderer::with_capabilities(egl, capabilities)?;
        // let egl = gl_renderer.egl_context();

        let context = skia::gpu::DirectContext::new_gl(None, None);

        let (gl, _exts) = {
            let gl =
                ffi::Gles2::load_with(|s| smithay::backend::egl::get_proc_address(s) as *const _);
            let ext_ptr = gl.GetString(ffi::EXTENSIONS) as *const c_char;
            if ext_ptr.is_null() {
                return Err(GlesError::GLFunctionLoaderError);
            }

            let exts = {
                let p = CStr::from_ptr(ext_ptr);
                let list =
                    String::from_utf8(p.to_bytes().to_vec()).unwrap_or_else(|_| String::new());
                list.split(' ').map(|e| e.to_string()).collect::<Vec<_>>()
            };

            tracing::info!("Initializing OpenGL ES Renderer");
            tracing::info!(
                "GL Version: {:?}",
                CStr::from_ptr(gl.GetString(ffi::VERSION) as *const c_char)
            );
            tracing::info!(
                "GL Vendor: {:?}",
                CStr::from_ptr(gl.GetString(ffi::VENDOR) as *const c_char)
            );
            tracing::info!(
                "GL Renderer: {:?}",
                CStr::from_ptr(gl.GetString(ffi::RENDERER) as *const c_char)
            );
            tracing::info!("Supported GL Extensions: {:?}", exts);

            // let gl_version = version::GlVersion::try_from(&gl).unwrap_or_else(|_| {
            //     tracing::warn!("Failed to detect GLES version, defaulting to 2.0");
            //     version::GLES_2_0
            // });

            // required for the manditory wl_shm formats
            if !exts
                .iter()
                .any(|ext| ext == "GL_EXT_texture_format_BGRA8888")
            {
                return Err(GlesError::GLExtensionNotSupported(&[
                    "GL_EXT_texture_format_BGRA8888",
                ]));
            }

            // required for buffers without linear memory layout
            // if gl_version < version::GLES_3_0
            //     && !exts.iter().any(|ext| ext == "GL_EXT_unpack_subimage")
            // {
            //     return Err(GlesError::GLExtensionNotSupported(&[
            //         "GL_EXT_unpack_subimage",
            //     ]));
            // }

            // let gl_debug_span = if requested_capabilities.contains(&Capability::Debug) {
            //     gl.Enable(ffi::DEBUG_OUTPUT);
            //     gl.Enable(ffi::DEBUG_OUTPUT_SYNCHRONOUS);
            //     let span = Box::into_raw(Box::new(span.clone()));
            //     gl.DebugMessageCallback(Some(gl_debug_log), span as *mut _);
            //     Some(span)
            // } else {
            //     None
            // };

            // (gl, gl_version, exts, requested_capabilities, gl_debug_span)
            (gl, exts)
        };

        Ok(SkiaRenderer {
            gl_renderer,
            gl,
            target_renderer: HashMap::new(),
            buffers: HashMap::new(),
            current_target: None,
            context,
        })
    }

    /// # Safety
    ///
    /// This operation will cause undefined behavior if the given EGLContext is active in another thread.
    pub unsafe fn new(context: EGLContext) -> Result<SkiaRenderer, GlesError> {
        let supported_capabilities = Self::supported_capabilities(&context)?;
        Self::with_capabilities(context, supported_capabilities)
    }
    pub fn egl_context(&self) -> &EGLContext {
        self.gl_renderer.egl_context()
    }
    pub fn current_skia_renderer(&mut self) -> Option<&SkiaSurface> {
        let current_target = self.current_target.as_ref().unwrap();

        let renderer = self.target_renderer.get(&current_target);
        renderer
    }
    fn create_texture_and_framebuffer(
        &self,
        width: i32,
        height: i32,
        format: Fourcc,
    ) -> SkiaGLesFbo {
        let mut texture: GLuint = 0;
        let mut framebuffer: GLuint = 0;

        let (internal_format, read_format, read_type) = fourcc_to_gl_formats(format).unwrap();
        unsafe {
            // Generate and bind the texture
            self.gl.GenTextures(1, &mut texture);
            self.gl.BindTexture(ffi::TEXTURE_2D, texture);

            // Set the texture parameters
            self.gl.TexParameteri(
                ffi::TEXTURE_2D,
                ffi::TEXTURE_MIN_FILTER,
                ffi::LINEAR as GLint,
            );
            self.gl.TexParameteri(
                ffi::TEXTURE_2D,
                ffi::TEXTURE_MAG_FILTER,
                ffi::LINEAR as GLint,
            );
            self.gl.TexParameteri(
                ffi::TEXTURE_2D,
                ffi::TEXTURE_WRAP_S,
                ffi::MIRRORED_REPEAT as GLint,
            );
            self.gl.TexParameteri(
                ffi::TEXTURE_2D,
                ffi::TEXTURE_WRAP_T,
                ffi::MIRRORED_REPEAT as GLint,
            );

            // Allocate the texture storage
            self.gl.TexImage2D(
                ffi::TEXTURE_2D,
                0,
                internal_format as GLint,
                width,
                height,
                0,
                read_format,
                read_type,
                std::ptr::null(),
            );

            // Generate and bind the framebuffer
            self.gl.GenFramebuffers(1, &mut framebuffer);
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, framebuffer);

            // Attach the texture to the framebuffer
            self.gl.FramebufferTexture2D(
                ffi::FRAMEBUFFER,
                ffi::COLOR_ATTACHMENT0,
                ffi::TEXTURE_2D,
                texture,
                0,
            );

            // Check that the framebuffer is complete
            if self.gl.CheckFramebufferStatus(ffi::FRAMEBUFFER) != ffi::FRAMEBUFFER_COMPLETE {
                panic!("Failed to create complete framebuffer");
            }

            // Unbind the framebuffer
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
        }

        SkiaGLesFbo {
            fbo: framebuffer,
            tex_id: texture,
            format,
            origin: skia::gpu::SurfaceOrigin::TopLeft,
        }
    }
}
impl Texture for SkiaTexture {
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

impl<'a> Frame for SkiaFrame {
    type Error = GlesError;
    type TextureId = SkiaTexture;

    fn id(&self) -> usize {
        // self.renderer.id()
        self.id
    }
    fn clear(
        &mut self,
        color: [f32; 4],
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), Self::Error> {
        self.draw_solid(
            Rectangle::from_loc_and_size((0, 0), self.size),
            damage,
            color,
        )?;
        self.skia_surface.flush_submit_and_sync_cpu();
        Ok(())
    }
    fn draw_solid(
        &mut self,
        dest: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        color: [f32; 4],
    ) -> Result<(), Self::Error> {
        let instances = damage
            .iter()
            .map(|rect| {
                let dest_size = dest.size;

                let rect_constrained_loc = rect
                    .loc
                    .constrain(Rectangle::from_extemities((0, 0), dest_size.to_point()));
                let rect_clamped_size = rect.size.clamp(
                    (0, 0),
                    (dest_size.to_point() - rect_constrained_loc).to_size(),
                );

                let rect = Rectangle::from_loc_and_size(rect_constrained_loc, rect_clamped_size);
                skia::Rect::from_xywh(
                    (dest.loc.x + rect.loc.x) as f32,
                    (dest.loc.y + rect.loc.y) as f32,
                    (rect.size.w) as f32,
                    (rect.size.h) as f32,
                )
            })
            .collect::<Vec<skia::Rect>>();
        let color = skia::Color4f::new(color[0], color[1], color[2], color[3]);
        // let red_color = skia::Color4f::new(1.0, 0.0, 0.0, 1.0);
        let mut paint = skia::Paint::new(color, None);
        paint.set_blend_mode(skia::BlendMode::Src);

        let mut surface = self.skia_surface.clone();

        for rect in instances.iter() {
            let canvas = surface.canvas();
            canvas.draw_rect(*rect, &paint);
        }

        Ok(())
    }
    fn render_texture_from_to(
        &mut self,
        texture: &Self::TextureId,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        transform: Transform,
        alpha: f32,
    ) -> Result<(), Self::Error> {

        
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
                skia::Rect::from_xywh(
                    rect.loc.x as f32,
                    rect.loc.y as f32,
                    rect.size.w as f32,
                    rect.size.h as f32,
                )
            })
            .collect::<Vec<skia::Rect>>();

        let image = &texture.image;

        let mut paint = skia::Paint::new(skia::Color4f::new(1.0, 1.0, 1.0, alpha), None);
        paint.set_blend_mode(skia::BlendMode::SrcOver);

        let mut matrix = skia::Matrix::new_identity();

        
        let mut surface = self.skia_surface.clone();

        let canvas = surface.canvas();
        let scale_x = dst.size.w as f32 / src.size.w as f32;
        let scale_y = dst.size.h as f32 / src.size.h as f32;

        match transform {
            Transform::Normal => {
                matrix.pre_scale((scale_x, scale_y), None);
                matrix.pre_translate((
                    (dst.loc.x as f32 - src.loc.x as f32) / scale_x,
                    (dst.loc.y as f32 - src.loc.y as f32) / scale_y,
                ));
            }
            Transform::Flipped180 => {
                matrix.pre_scale((scale_x, -scale_y), None);
                matrix.pre_translate((
                    (dst.loc.x as f32 - src.loc.x as f32) / scale_x,
                    (-dst.loc.y as f32 + src.loc.y as f32) / scale_y,
                ));
            }
            Transform::Flipped90 => {

                panic!("unhandled transform {:?}", transform);
            }
            Transform::Flipped270 => {

                panic!("unhandled transform {:?}", transform);
            }
            _ => {
                panic!("unhandled transform {:?}", transform);
            }
        }

        for rect in instances.iter() {
            let dst_rect = skia::Rect::from_xywh(
                dst.loc.x as f32 + rect.x(),
                dst.loc.y as f32 + rect.y(),
                rect.width(),
                rect.height(),
            );

            paint.set_shader(image.to_shader(
                (skia::TileMode::Repeat, skia::TileMode::Repeat),
                skia::SamplingOptions::default(),
                &matrix,
            ));

            canvas.draw_rect(dst_rect, &paint);
        }

        Ok(())
    }
    fn transformation(&self) -> Transform {
        // self.frame.transformation()
        Transform::Normal
    }
    fn finish(self) -> Result<SyncPoint, Self::Error> {
        let mut surface = self.skia_surface;
        surface.flush_submit_and_sync_cpu();

        Ok(SyncPoint::signaled())
    }
}

impl Renderer for SkiaRenderer {
    type Error = GlesError;
    type TextureId = SkiaTexture;
    type Frame<'a> = SkiaFrame;

    fn id(&self) -> usize {
        99
    }
    fn downscale_filter(&mut self, filter: TextureFilter) -> Result<(), Self::Error> {
        self.gl_renderer.downscale_filter(filter)
    }
    fn upscale_filter(&mut self, filter: TextureFilter) -> Result<(), Self::Error> {
        self.gl_renderer.upscale_filter(filter)
    }
    fn set_debug_flags(&mut self, flags: DebugFlags) {
        self.gl_renderer.set_debug_flags(flags)
    }
    fn debug_flags(&self) -> DebugFlags {
        self.gl_renderer.debug_flags()
    }

    fn render(
        &mut self,
        output_size: Size<i32, Physical>,
        _dst_transform: Transform,
    ) -> Result<Self::Frame<'_>, Self::Error> {
        let id = self.id();
        let current_target = self.current_target.as_ref().unwrap();
        let buffer = self.buffers.get(current_target).unwrap();

        self
            .target_renderer
            .entry(current_target.clone())
            .or_insert_with(|| {
                {
                    let ctx = self.context.as_mut().unwrap();
                    ctx.reset(None);
                }
                let context = self.context.as_ref();
                let color_type = match buffer.format {
                    Fourcc::Argb8888 => skia::ColorType::RGBA8888,
                    Fourcc::Abgr2101010 => skia::ColorType::RGBA8888,
                    _ => skia::ColorType::RGBA8888,
                };
                SkiaSurface::new_with_fbo(
                    output_size.w,
                    output_size.h,
                    0_usize,
                    8_usize,
                    buffer.fbo,
                    color_type,
                    context,
                    buffer.origin,
                )
            });

        unsafe {
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, buffer.fbo);

            let status = self.gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);

            if status != ffi::FRAMEBUFFER_COMPLETE {
                println!("framebuffer incomplete");
                // return Err(GlesError::FramebufferBindingError);
            }
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
        }
        let surface = self.target_renderer.get_mut(&self.current_target.as_ref().unwrap()).unwrap();

        Ok(SkiaFrame {
            skia_surface: surface.surface(),
            size: output_size,
            id,
        })
    }
}

pub fn import_into_skia_image(
    texture: &GlesTexture,
    context: &mut skia::gpu::DirectContext,
) -> Option<skia::Image> {
    let target = ffi::TEXTURE_2D;

    let size = skia::Point {
        x: texture.width() as f32,
        y: texture.height() as f32,
    };
    unsafe {
        let texture_info = skia::gpu::gl::TextureInfo {
            target,
            id: texture.tex_id(),
            format: skia::gpu::gl::Format::RGBA8.into(),
        };

        let texture = skia::gpu::BackendTexture::new_gl(
            (size.x as i32, size.y as i32),
            skia::gpu::MipMapped::No,
            texture_info,
        );

        skia::Image::from_texture(
            context,
            &texture,
            skia::gpu::SurfaceOrigin::TopLeft,
            skia::ColorType::RGBA8888,
            skia::AlphaType::Premul,
            None,
        )
    }
}
impl SkiaRenderer {
    fn import_skia_image_from_texture(&mut self, texture: &GlesTexture) -> Option<skia::Image> {
        let context = self.context.as_mut().unwrap();

        let target = ffi::TEXTURE_2D;

        let size = skia::Point {
            x: texture.width() as f32,
            y: texture.height() as f32,
        };
        unsafe {
            let texture_info = skia::gpu::gl::TextureInfo {
                target,
                id: texture.tex_id(),
                format: skia::gpu::gl::Format::RGBA8.into(),
            };

            let texture = skia::gpu::BackendTexture::new_gl(
                (size.x as i32, size.y as i32),
                skia::gpu::MipMapped::No,
                texture_info,
            );

            skia::Image::from_texture(
                context,
                &texture,
                skia::gpu::SurfaceOrigin::TopLeft,
                skia::ColorType::RGBA8888,
                skia::AlphaType::Premul,
                None,
            )
        }
    }
    fn import_skia_image_from_raster_data(
        &mut self,
        data: &[u8],
        size: Size<i32, Buffer>,
        format: Fourcc,
    ) -> Option<skia::Image> {
        let size = skia::ISize::new(size.w, size.h);

        let color_type = match format {
            Fourcc::Argb8888 => skia::ColorType::BGRA8888,
            Fourcc::Abgr8888 => skia::ColorType::RGBA8888,
            Fourcc::Abgr2101010 => skia::ColorType::RGBA1010102,
            _ => skia::ColorType::RGBA8888,
        };
        let info = skia::ImageInfo::new(size, color_type, skia::AlphaType::Premul, None);
        let pixmap = skia::Pixmap::new(&info, data, size.width as usize * 4);
        let context = self.context.as_mut().unwrap();

        let image =
            skia::Image::new_cross_context_from_pixmap(context, &pixmap, true, Some(true))
                .unwrap();
        image.clone().flush_and_submit(context);

        image.new_texture_image(context, skia::gpu::MipMapped::Yes)
    }
}
impl ImportMemWl for SkiaRenderer {
    fn import_shm_buffer(
        &mut self,
        buffer: &WlBuffer,
        _surface: Option<&SurfaceData>,
        _damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        with_buffer_contents(
            buffer,
            |ptr, len, data: smithay::wayland::shm::BufferData| {
                let offset = data.offset;
                let width = data.width;
                let height = data.height;
                // let stride = data.stride;

                let size = Size::<i32, Buffer>::from((width, height));

                let fourcc = shm_format_to_fourcc(data.format)
                    .ok_or(GlesError::UnsupportedWlPixelFormat(data.format))?;

                let has_alpha = has_alpha(fourcc);

                let ptr = unsafe { ptr.offset(offset as isize) };
                let data_slice = unsafe { std::slice::from_raw_parts(ptr, len) };

                let image = self
                    .import_skia_image_from_raster_data(data_slice, size, fourcc)
                    .ok_or("")
                    .map_err(|_| GlesError::MappingError)?;

                let (texture, _) = image.backend_texture(true).unwrap();
                let texture = unsafe {
                    let info = texture.gl_texture_info().unwrap();
                    GlesTexture::from_raw(
                        &self.gl_renderer,
                        Some(info.format),
                        has_alpha,
                        info.id,
                        Size::<i32, Buffer>::from((texture.width(), texture.height())),
                    )
                };
                Ok(SkiaTexture {
                    texture,
                    image,
                    has_alpha,
                })
            },
        )
        .map_err(GlesError::BufferAccessError)?
    }
    fn shm_formats(
        &self,
    ) -> Box<dyn Iterator<Item = smithay::reexports::wayland_server::protocol::wl_shm::Format>>
    {
        self.gl_renderer.shm_formats()
    }
}

impl ImportEgl for SkiaRenderer {
    fn bind_wl_display(&mut self, display: &DisplayHandle) -> Result<(), egl::Error> {
        self.gl_renderer.bind_wl_display(display)
    }

    fn unbind_wl_display(&mut self) {
        self.gl_renderer.unbind_wl_display()
    }

    fn egl_reader(&self) -> Option<&EGLBufferReader> {
        self.gl_renderer.egl_reader()
    }

    fn import_egl_buffer(
        &mut self,
        buffer: &WlBuffer,
        surface: Option<&SurfaceData>,
        damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self
            .gl_renderer
            .import_egl_buffer(buffer, surface, damage)?;
        let has_alpha = texture
            .format()
            .map_or(false, |fourcc: Fourcc| has_alpha(fourcc));
        let image = self
            .import_skia_image_from_texture(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;

        if let SkiaTarget::EGLSurface(EGLSurfaceWrapper(surface)) =
            self.current_target.as_ref().unwrap()
        {
            unsafe {
                self.egl_context().make_current_with_surface(surface)?;
                self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
            }
        }

        Ok(SkiaTexture {
            texture,
            image,
            has_alpha,
        })
    }
}

impl ImportDma for SkiaRenderer {
    fn import_dmabuf(
        &mut self,
        dmabuf: &Dmabuf,
        damage: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self.gl_renderer.import_dmabuf(dmabuf, damage)?;
        let has_alpha = texture
            .format()
            .map_or(false, |fourcc: Fourcc| has_alpha(fourcc));
        let image = self
            .import_skia_image_from_texture(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;
        Ok(SkiaTexture {
            texture,
            image,
            has_alpha,
        })
    }
}

impl ImportDmaWl for SkiaRenderer {}

impl ImportMem for SkiaRenderer {
    fn import_memory(
        &mut self,
        data: &[u8],
        format: Fourcc,
        size: Size<i32, Buffer>,
        flipped: bool,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self
            .gl_renderer
            .import_memory(data, format, size, flipped)?;
        let image = self
            .import_skia_image_from_texture(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;
        let has_alpha = has_alpha(format);

        if let Some(SkiaTarget::EGLSurface(EGLSurfaceWrapper(surface))) =
            self.current_target.as_ref()
        {
            unsafe {
                self.egl_context().make_current_with_surface(surface)?;
                self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
            }
        }

        Ok(SkiaTexture {
            texture,
            image,
            has_alpha,
        })
    }
    fn mem_formats(&self) -> Box<dyn Iterator<Item = Fourcc>> {
        self.gl_renderer.mem_formats()
    }
    fn update_memory(
        &mut self,
        texture: &<Self as Renderer>::TextureId,
        data: &[u8],
        region: Rectangle<i32, Buffer>,
    ) -> Result<(), <Self as Renderer>::Error> {
        self.gl_renderer
            .update_memory(&texture.texture, data, region)
    }
}

impl Borrow<GlesRenderer> for SkiaRenderer {
    fn borrow(&self) -> &GlesRenderer {
        &self.gl_renderer
    }
}

impl BorrowMut<GlesRenderer> for SkiaRenderer {
    fn borrow_mut(&mut self) -> &mut GlesRenderer {
        &mut self.gl_renderer
    }
}

impl TextureMapping for SkiaTextureMapping {
    fn flipped(&self) -> bool {
        false
    }
    fn format(&self) -> Fourcc {
        self.fourcc_format
    }
}

impl Texture for SkiaTextureMapping {
    fn width(&self) -> u32 {
        self.image.width() as u32
    }
    fn height(&self) -> u32 {
        self.image.height() as u32
    }
    fn format(&self) -> Option<Fourcc> {
        Some(self.fourcc_format)
    }
}
impl ExportMem for SkiaRenderer {
    type TextureMapping = SkiaTextureMapping;

    // Copies a region of the framebuffer into a texture and returns a TextureMapping
    fn copy_framebuffer(
        &mut self,
        region: Rectangle<i32, Buffer>,
        fourcc: Fourcc,
    ) -> Result<Self::TextureMapping, <Self as Renderer>::Error> {
        tracing::trace!("copy_framebuffer {:?} {:?}", region, fourcc);

        let renderer = self.current_skia_renderer().unwrap();

        let mut surface = renderer.surface();
        surface.flush_submit_and_sync_cpu();
        let image = surface
            .image_snapshot_with_bounds(skia::IRect::from_xywh(
                region.loc.x,
                region.loc.y,
                region.size.w,
                region.size.h,
            ))
            .unwrap();
        let len = region.size.w * region.size.h * 4;
        let (_, read_format, _) =
            fourcc_to_gl_formats(fourcc).ok_or(GlesError::UnknownPixelFormat)?;
        let color_type = match read_format {
            ffi::BGRA_EXT => skia::ColorType::BGRA8888,
            _ => skia::ColorType::RGBA8888,
        };
        let info = skia::ImageInfo::new(
            skia::ISize::new(region.size.w, region.size.h),
            color_type,
            skia::AlphaType::Premul,
            None,
        );
        let data = RefCell::new(vec![0; len as usize]);
        {
            let data_vec = data.borrow_mut();
            let byte_row = info.min_row_bytes();

            let pixmap = skia::Pixmap::new(&info, &data_vec, byte_row);

            if !surface.read_pixels_to_pixmap(&pixmap, (0, 0)) {
                panic!("read_pixels_to_pixmap failed");
            }
        }
        Ok(Self::TextureMapping {
            fourcc_format: fourcc,
            flipped: false,
            width: region.size.w as u32,
            height: region.size.h as u32,
            image,
            data,
        })
    }
    fn copy_texture(
        &mut self,
        _texture: &Self::TextureId,
        region: Rectangle<i32, Buffer>,
        fourcc: Fourcc,
    ) -> Result<Self::TextureMapping, Self::Error> {
        tracing::trace!("copy_texture {:?} {:?}", region, fourcc);

        unimplemented!("copy_texture")

        
    }

    fn map_texture<'a>(
        &mut self,
        texture_mapping: &'a Self::TextureMapping,
    ) -> Result<&'a [u8], <Self as Renderer>::Error> {
        let data = texture_mapping.data.borrow_mut();
        let len = data.len();
        
        let ptr = data.as_ptr();
        unsafe { Ok(std::slice::from_raw_parts(ptr, len)) }
    }
}

impl Bind<Rc<EGLSurface>> for SkiaRenderer {
    fn bind(&mut self, surface: Rc<EGLSurface>) -> Result<(), <Self as Renderer>::Error> {
        unsafe {
            self.egl_context().make_current_with_surface(&surface)?;
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
        }
        let format = surface.pixel_format();
        let format = match (format.color_bits, format.alpha_bits) {
            (24, 8) => ffi::RGB8,
            (30, 2) => ffi::RGB10_A2,
            (48, 16) => ffi::RGB16F,
            _ => ffi::RGB8,
        };
        let sfbo = SkiaGLesFbo {
            fbo: 0,
            tex_id: 0,
            format: gl_internal_format_to_fourcc(format).unwrap(),
            origin: skia::gpu::SurfaceOrigin::BottomLeft,
        };

        let egl_surface = EGLSurfaceWrapper(surface.clone());

        let render_target = SkiaTarget::EGLSurface(egl_surface);
        self.current_target = Some(render_target.clone());

        self.buffers.insert(render_target, sfbo);
        Ok(())
    }
}

impl Bind<SkiaGLesFbo> for SkiaRenderer {
    fn bind(&mut self, texture: SkiaGLesFbo) -> Result<(), <Self as Renderer>::Error> {
        self.current_target = Some(SkiaTarget::Fbo(texture.clone()));
        self.buffers
            .insert(SkiaTarget::Fbo(texture.clone()), texture);

        Ok(())
    }
}

impl Bind<GlesTexture> for SkiaRenderer {
    fn bind(&mut self, texture: GlesTexture) -> Result<(), <Self as Renderer>::Error> {
        self.current_target = Some(SkiaTarget::Texture(texture.tex_id()));
        // let res = self.gl_renderer.bind(texture);
        unimplemented!("bind GlesTexture")

        // Ok(())
    }
}

impl Bind<GlesRenderbuffer> for SkiaRenderer {
    fn bind(&mut self, target: GlesRenderbuffer) -> Result<(), <Self as Renderer>::Error> {
        self.current_target = Some(SkiaTarget::Renderbuffer(&target));
        // let res = self.gl_renderer.bind(target);
        unimplemented!("bind GlesRenderbuffer")
    }
}

impl Bind<Dmabuf> for SkiaRenderer {
    fn bind(&mut self, dmabuf: Dmabuf) -> Result<(), <Self as Renderer>::Error> {
        let target = SkiaTarget::Dmabuf(dmabuf.clone());
        self.current_target = Some(target);
        let egl_display = self.egl_context().display().clone();
        let buffers = self.buffers.borrow_mut();
        buffers
            .entry(SkiaTarget::Dmabuf(dmabuf.clone()))
            .or_insert_with(|| {
                tracing::trace!("Creating EGLImage for Dmabuf: {:?}", dmabuf);
                let image = egl_display.create_image_from_dmabuf(&dmabuf).unwrap();
                // .map_err(GlesError::BindBufferEGLError)?;

                unsafe {
                    let mut rbo = 0;
                    self.gl.GenRenderbuffers(1, &mut rbo as *mut _);
                    self.gl.BindRenderbuffer(ffi::RENDERBUFFER, rbo);
                    self.gl
                        .EGLImageTargetRenderbufferStorageOES(ffi::RENDERBUFFER, image);
                    self.gl.BindRenderbuffer(ffi::RENDERBUFFER, 0);

                    let mut fbo = 0;
                    self.gl.GenFramebuffers(1, &mut fbo as *mut _);
                    self.gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
                    self.gl.FramebufferRenderbuffer(
                        ffi::FRAMEBUFFER,
                        ffi::COLOR_ATTACHMENT0,
                        ffi::RENDERBUFFER,
                        rbo,
                    );
                    let status = self.gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
                    self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);

                    if status != ffi::FRAMEBUFFER_COMPLETE {
                        //TODO wrap image and drop here
                        println!("framebuffer incomplete");
                        // return Err(GlesError::FramebufferBindingError);
                    }
                    SkiaGLesFbo {
                        fbo,
                        tex_id: 0,
                        format: Fourcc::Abgr8888,
                        origin: skia::gpu::SurfaceOrigin::TopLeft,
                    }
                }
            });
        Ok(())
    }
}

impl Unbind for SkiaRenderer {
    fn unbind(&mut self) -> Result<(), <Self as Renderer>::Error> {
        self.current_target = None;
        self.egl_context().unbind()?;
        Ok(())
    }
}

impl Offscreen<SkiaGLesFbo> for SkiaRenderer {
    #[profiling::function]
    fn create_buffer(
        &mut self,
        format: Fourcc,
        size: Size<i32, Buffer>,
    ) -> Result<SkiaGLesFbo, GlesError> {
        let lfbo = self.create_texture_and_framebuffer(size.w, size.h, format);
        Ok(lfbo)
    }
}

impl AsRef<SkiaFrame> for SkiaFrame {
    fn as_ref(&self) -> &SkiaFrame {
        self
    }
}

impl AsMut<SkiaFrame> for SkiaFrame {
    fn as_mut(&mut self) -> &mut SkiaFrame {
        self
    }
}
