use std::{
    borrow::{Borrow, BorrowMut},
    cell::RefCell,
    collections::HashMap,
    ffi::{c_char, CStr},
    rc::Rc,
    time::Duration,
};

use lay_rs::skia;

use smithay::{
    backend::{
        allocator::{
            dmabuf::{Dmabuf, WeakDmabuf},
            format::{has_alpha, FormatSet},
            Buffer as DmaBuffer, Fourcc,
        },
        egl::{
            self,
            display::{EGLBufferReader, EGLDisplayHandle},
            fence::EGLFence,
            ffi::egl::{
                types::{EGLImage, EGLSync},
                CreateSync, SYNC_FENCE,
            },
            wrap_egl_call, wrap_egl_call_ptr, EGLContext, EGLDisplay, EGLSurface,
        },
        renderer::{
            gles::{
                ffi::{
                    self,
                    types::{GLint, GLuint},
                },
                format::{fourcc_to_gl_formats, gl_internal_format_to_fourcc},
                Capability, GlesError, GlesRenderbuffer, GlesRenderer, GlesTexture,
            },
            sync::{Fence, Interrupted, SyncPoint},
            Bind, Color32F, DebugFlags, ExportMem, Frame, ImportDma, ImportDmaWl, ImportEgl,
            ImportMem, ImportMemWl, Offscreen, Renderer, Texture, TextureFilter, TextureMapping,
            Unbind,
        },
    },
    reexports::wayland_server::{protocol::wl_buffer::WlBuffer, DisplayHandle},
    utils::{Buffer, Physical, Rectangle, Size, Transform},
    wayland::compositor::SurfaceData,
};

#[derive(Clone)]
pub struct SkiaSurface {
    pub gr_context: skia::gpu::DirectContext,
    pub surface: skia::Surface,
}
impl SkiaSurface {
    pub fn canvas(&mut self) -> &skia::Canvas {
        self.surface.canvas()
    }
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
                fboid: fboid.into(),
                format: skia::gpu::gl::Format::RGBA8.into(),
                ..Default::default()
            }
        };
        let backend_render_target = skia::gpu::backend_render_targets::make_gl(
            (width.into(), height.into()),
            sample_count.into(),
            stencil_bits.into(),
            fb_info,
        );

        let mut gr_context: skia::gpu::DirectContext = if let Some(context) = context {
            context.clone()
        } else {
            let interface = skia::gpu::gl::Interface::new_native().unwrap();
            skia::gpu::direct_contexts::make_gl(interface, None).unwrap()
        };
        gr_context.reset(None);
        let surface = skia::gpu::surfaces::wrap_backend_render_target(
            &mut gr_context,
            &backend_render_target,
            origin,
            color_type,
            None,
            Some(&skia::SurfaceProps::new(
                Default::default(),
                skia::PixelGeometry::BGRH, // for font rendering optimisations
            )),
        )
        .unwrap();

        Self {
            gr_context,
            surface,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_texture(
        width: impl Into<i32>,
        height: impl Into<i32>,
        sample_cnt: impl Into<usize>,
        texid: impl Into<u32>,
        color_type: skia::ColorType,
        context: Option<&skia::gpu::DirectContext>,
        origin: skia::gpu::SurfaceOrigin,
    ) -> Self {
        let sample_cnt = sample_cnt.into();
        let gl_info = skia::gpu::gl::TextureInfo {
            target: ffi::TEXTURE_2D,
            id: texid.into(),
            format: skia::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };
        let backend_texture = unsafe {
            skia::gpu::backend_textures::make_gl(
                (width.into(), height.into()),
                skia::gpu::Mipmapped::No,
                gl_info,
                "",
            )
        };
        let mut gr_context: skia::gpu::DirectContext = if let Some(context) = context {
            context.clone()
        } else {
            let interface = skia::gpu::gl::Interface::new_native().unwrap();
            skia::gpu::direct_contexts::make_gl(interface, None).unwrap()
        };
        gr_context.reset(None);
        let surface = skia::gpu::surfaces::wrap_backend_texture(
            &mut gr_context,
            &backend_texture,
            origin,
            sample_cnt,
            color_type,
            None,
            Some(&skia::SurfaceProps::new(
                Default::default(),
                skia::PixelGeometry::BGRH, // for font rendering optimisations
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
    pub context: Option<skia::gpu::DirectContext>,

    dmabuf_cache: std::collections::HashMap<WeakDmabuf, SkiaTexture>,
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
        options.skip_gl_error_checks = skia::gpu::ganesh::context_options::Enable::Yes;
        // options.
        let interface = skia::gpu::gl::Interface::new_native().unwrap();
        let mut context = skia::gpu::direct_contexts::make_gl(interface, &options);

        let ctx = context.as_mut().unwrap();
        ctx.reset(None);

        SkiaRenderer {
            gl,
            gl_renderer,
            target_renderer: HashMap::new(),
            buffers: HashMap::new(),
            current_target: None,
            context,
            dmabuf_cache: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkiaTexture {
    pub texture: GlesTexture,
    pub image: skia::Image,
    pub has_alpha: bool,
    pub format: Option<Fourcc>,
    pub egl_images: Option<Vec<EGLImage>>,
    pub is_external: bool,
    pub damage: Option<Vec<Rectangle<i32, Buffer>>>,
}

unsafe impl Send for SkiaTexture {}

#[derive(Debug, Clone)]
pub struct SkiaTextureImage {
    pub image: skia::Image,
    pub has_alpha: bool,
    pub format: Option<Fourcc>,
    pub damage: Option<Vec<Rectangle<i32, Buffer>>>,
}

impl From<SkiaTexture> for SkiaTextureImage {
    fn from(value: SkiaTexture) -> Self {
        SkiaTextureImage {
            image: value.image,
            has_alpha: value.has_alpha,
            format: value.format,
            damage: value.damage,
        }
    }
}

pub struct SkiaFrame<'frame> {
    size: Size<i32, Physical>,
    pub skia_surface: SkiaSurface,
    renderer: &'frame mut SkiaRenderer,
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
    #[allow(private_interfaces)]
    EGLSurface(EGLSurfaceWrapper),
    Texture(ffi::types::GLuint),
    Renderbuffer(*const GlesRenderbuffer),
    Dmabuf(Dmabuf),
    Fbo(SkiaGLesFbo),
}
impl std::fmt::Debug for SkiaRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkiaRenderer").finish()
    }
}

// #[allow(dead_code)]
// fn save_surface(surface: &mut skia::Surface, name: &str) {
//     surface.flush_submit_and_sync_cpu();
//     let image = surface.image_snapshot();

//     save_image(&image, name);
// }
#[allow(dead_code)]
fn save_image(image: &skia::Image, name: &str) {
    use std::fs::File;
    use std::io::Write;
    #[allow(deprecated)]
    let data = image.encode_to_data(skia::EncodedImageFormat::PNG).unwrap();
    let bytes = data.as_bytes();
    let filename = format!("{}.png", name);
    let mut file = File::create(filename).unwrap();
    file.write_all(bytes).unwrap();
}

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
        let interface = skia::gpu::gl::Interface::new_native().unwrap();
        let context = skia::gpu::direct_contexts::make_gl(interface, None);

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

            // let gl_version = smithay::backend::renderer::gles::version::GlVersion::try_from(&gl).unwrap_or_else(|_| {
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
            dmabuf_cache: std::collections::HashMap::new(),
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
        let renderer = self
            .current_target
            .as_ref()
            .and_then(|current_target| self.target_renderer.get(current_target));
        renderer
    }
    #[profiling::function]
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

    fn import_dmabuf_internal(
        &mut self,
        dmabuf: &Dmabuf,
        damage: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        use smithay::backend::allocator::Buffer;
        // if !self.extensions.iter().any(|ext| ext == "GL_OES_EGL_image") {
        //     return Err(GlesError::GLExtensionNotSupported(&["GL_OES_EGL_image"]));
        // }

        // self.make_current()?;

        let texture = self
            .existing_dmabuf_texture(dmabuf)?
            .map(Ok)
            .unwrap_or_else(|| {
                // println!("importing dmabuf {:?}", dmabuf);
                let is_external = !self
                    .egl_context()
                    .dmabuf_render_formats()
                    .contains(&dmabuf.format());

                let egl_image = self
                    .egl_context()
                    .display()
                    .create_image_from_dmabuf(dmabuf)
                    .map_err(GlesError::BindBufferEGLError)?;

                let tex = self.import_egl_image(egl_image, is_external, None)?;
                let format = fourcc_to_gl_formats(dmabuf.format().code)
                    .map(|(internal, _, _)| internal)
                    .unwrap_or(ffi::RGBA8);
                let has_alpha = has_alpha(dmabuf.format().code);

                let gles_texture = unsafe {
                    GlesTexture::from_raw(
                        &self.gl_renderer,
                        Some(format),
                        !has_alpha,
                        tex,
                        dmabuf.size(),
                    )
                };
                let image = self
                    .import_skia_image_from_texture(&gles_texture)
                    .ok_or("")
                    .map_err(|_| GlesError::MappingError)?;

                let texture = SkiaTexture {
                    texture: gles_texture,
                    image,
                    has_alpha,
                    format: Some(dmabuf.format().code),
                    egl_images: Some(vec![egl_image]),
                    is_external,
                    damage: damage.map(|damage| damage.to_vec()),
                };

                self.dmabuf_cache.insert(dmabuf.weak(), texture.clone());
                // println!("importing dmabuf {} {:?}", image_id, damage);
                Ok(texture)
            });
        texture.map(|mut tex| {
            tex.damage = damage.map(|damage| damage.to_vec());
            tex
        })
    }
    #[profiling::function]
    fn existing_dmabuf_texture(&self, buffer: &Dmabuf) -> Result<Option<SkiaTexture>, GlesError> {
        // self.gl_renderer.import_dmabuf(dmabuf, damage)
        let existing_texture = self
            .dmabuf_cache
            .iter()
            .find(|(weak, _)| {
                weak.upgrade()
                    .map(|entry| &entry == buffer)
                    .unwrap_or(false)
            })
            .map(|(_, tex)| tex.clone());

        if let Some(texture) = existing_texture {
            // tracing::trace!("Re-using texture {:?} for {:?}", texture.0.texture, buffer);
            if let Some(egl_images) = texture.egl_images.as_ref() {
                if egl_images[0] == smithay::backend::egl::ffi::egl::NO_IMAGE_KHR {
                    return Ok(None);
                }
                let tex = Some(texture.texture.tex_id());
                self.import_egl_image(egl_images[0], texture.is_external, tex)?;
            }
            Ok(Some(texture))
        } else {
            Ok(None)
        }
    }

    #[profiling::function]
    fn import_egl_image(
        &self,
        image: smithay::backend::egl::ffi::egl::types::EGLImage,
        is_external: bool,
        tex: Option<u32>,
    ) -> Result<u32, GlesError> {
        let tex = tex.unwrap_or_else(|| unsafe {
            let mut tex = 0;
            self.gl.GenTextures(1, &mut tex);
            tex
        });
        let target = if is_external {
            ffi::TEXTURE_EXTERNAL_OES
        } else {
            ffi::TEXTURE_2D
        };
        unsafe {
            self.gl.BindTexture(target, tex);
            self.gl.EGLImageTargetTexture2DOES(target, image);
            self.gl.BindTexture(target, 0);
        }

        Ok(tex)
    }
}
impl Texture for SkiaTexture {
    fn width(&self) -> u32 {
        self.image.width() as u32
    }
    fn height(&self) -> u32 {
        self.image.height() as u32
    }
    fn format(&self) -> Option<Fourcc> {
        self.format
    }
}

impl<'frame> Frame for SkiaFrame<'frame> {
    type Error = GlesError;
    type TextureId = SkiaTexture;

    fn id(&self) -> usize {
        // self.renderer.id()
        self.id
    }
    fn clear(
        &mut self,
        color: Color32F,
        at: &[Rectangle<i32, Physical>],
    ) -> Result<(), Self::Error> {
        self.draw_solid(Rectangle::from_loc_and_size((0, 0), self.size), at, color)?;
        Ok(())
    }
    fn draw_solid(
        &mut self,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        color: Color32F,
    ) -> Result<(), Self::Error> {
        let dest_rect = skia::Rect::from_xywh(
            dst.loc.x as f32,
            dst.loc.y as f32,
            dst.size.w as f32,
            dst.size.h as f32,
        );
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
                    (dst.loc.x + rect.loc.x) as f32,
                    (dst.loc.y + rect.loc.y) as f32,
                    (rect.size.w) as f32,
                    (rect.size.h) as f32,
                )
            })
            .collect::<Vec<skia::Rect>>();
        let color = skia::Color4f::new(color.r(), color.g(), color.b(), color.a());
        // let red_color = skia::Color4f::new(1.0, 0.0, 0.0, 1.0);
        let mut paint = skia::Paint::new(color, None);
        paint.set_blend_mode(skia::BlendMode::Src);

        let mut surface = self.skia_surface.clone();

        let canvas = surface.canvas();
        let mut damage_rect = skia::Rect::default();
        for rect in instances.iter() {
            damage_rect.join(rect);
        }
        canvas.save();
        canvas.clip_rect(damage_rect, None, None);
        canvas.draw_rect(dest_rect, &paint);
        canvas.restore();

        Ok(())
    }
    #[profiling::function]
    fn render_texture_from_to(
        &mut self,
        texture: &Self::TextureId,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        src_transform: Transform,
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
        match src_transform {
            Transform::Normal => {
                matrix.pre_scale((scale_x, scale_y), None);
                matrix.pre_translate((
                    dst.loc.x as f32 / scale_x - (src.loc.x as f32),
                    dst.loc.y as f32 / scale_y - (src.loc.y as f32),
                ));
            }
            Transform::Flipped180 => {
                matrix.pre_scale((scale_x, -scale_y), None);
                matrix.pre_translate((
                    dst.loc.x as f32 / scale_x - src.loc.x as f32,
                    -dst.loc.y as f32 / scale_y + src.loc.y as f32,
                ));
            }
            Transform::Flipped90 => {
                panic!("unhandled transform {:?}", src_transform);
            }
            Transform::Flipped270 => {
                panic!("unhandled transform {:?}", src_transform);
            }
            _ => {
                panic!("unhandled transform {:?}", src_transform);
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
    #[profiling::function]
    fn finish(self) -> Result<SyncPoint, Self::Error> {
        let mut surface = self.skia_surface;

        let info = FlushInfo2 {
            num_semaphores: 0,
            signal_semaphores: std::ptr::null_mut(),
            finished_proc: Some(finished_proc),
            finished_context: std::ptr::null_mut(),
            submitted_proc: None,
            submitted_context: std::ptr::null_mut(),
        };

        // Transmute flushinfo2 into flushinfo
        let info = unsafe {
            let native = &*(&info as *const FlushInfo2 as *const sb::GrFlushInfo);
            &*(native as *const sb::GrFlushInfo as *const lay_rs::skia::gpu::FlushInfo)
        };

        FINISHED_PROC_STATE.store(false, Ordering::SeqCst);

        let semaphores = surface.gr_context.flush(info);

        let syncpoint = if semaphores == skia::gpu::SemaphoresSubmitted::Yes {
            profiling::scope!("FINISHED_PROC_STATE");
            let skia_sync = SkiaSync::create(self.renderer.egl_context().display())
                .map_err(|_err| GlesError::FramebufferBindingError)?;
            SyncPoint::from(skia_sync)
        } else {
            SyncPoint::signaled()
        };

        {
            profiling::scope!("context_submit");
            surface.gr_context.submit(None);
            // surface
            //     .gr_context
            //     .flush_and_submit_surface(&mut surface.surface, GrSyncCpu::Yes);
        }

        Ok(syncpoint)
    }

    fn wait(
        &mut self,
        sync: &smithay::backend::renderer::sync::SyncPoint,
    ) -> Result<(), Self::Error> {
        sync.wait()
            .map_err(|_| GlesError::FramebufferBindingError)?;
        Ok(())
    }
}

// this is a "hack" to expose finished_proc and submitted_proc
// until a PR is made to skia-bindings
use lay_rs::sb;

#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct FlushInfo2 {
    num_semaphores: usize,
    signal_semaphores: *mut sb::GrBackendSemaphore,
    pub finished_proc: sb::GrGpuFinishedProc,
    finished_context: sb::GrGpuFinishedContext,
    pub submitted_proc: sb::GrGpuSubmittedProc,
    submitted_context: sb::GrGpuSubmittedContext,
}

use std::sync::atomic::{AtomicBool, Ordering};

static FINISHED_PROC_STATE: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn finished_proc(_: *mut ::core::ffi::c_void) {
    FINISHED_PROC_STATE.store(true, Ordering::SeqCst);
}

#[derive(Debug, Clone)]
struct InnerSkiaFence {
    display_handle: std::sync::Arc<EGLDisplayHandle>,
    handle: EGLSync,
    // native: bool,
}

unsafe impl Send for InnerSkiaFence {}
unsafe impl Sync for InnerSkiaFence {}

#[derive(Debug, Clone)]
struct SkiaSync(std::sync::Arc<InnerSkiaFence>);

impl SkiaSync {
    pub fn create(display: &EGLDisplay) -> Result<Self, egl::Error> {
        let display_handle = display.get_display_handle();
        let handle = wrap_egl_call_ptr(|| unsafe {
            CreateSync(**display_handle, SYNC_FENCE, std::ptr::null())
        })
        .map_err(egl::Error::CreationFailed)?;

        Ok(Self(std::sync::Arc::new(InnerSkiaFence {
            display_handle,
            handle,
            // native: false,
        })))
    }
}
impl Fence for SkiaSync {
    fn export(&self) -> Option<std::os::unix::prelude::OwnedFd> {
        None
    }
    fn is_exportable(&self) -> bool {
        false
    }
    fn is_signaled(&self) -> bool {
        FINISHED_PROC_STATE.load(Ordering::SeqCst)
    }
    fn wait(&self) -> Result<(), Interrupted> {
        use smithay::backend::egl::ffi;

        let timeout = Some(Duration::from_millis(2))
            .map(|t| t.as_nanos() as ffi::egl::types::EGLuint64KHR)
            .unwrap_or(ffi::egl::FOREVER);

        let flush = false;
        let flags = if flush {
            ffi::egl::SYNC_FLUSH_COMMANDS_BIT as ffi::egl::types::EGLint
        } else {
            0
        };
        let _status = wrap_egl_call(
            || unsafe {
                ffi::egl::ClientWaitSync(**self.0.display_handle, self.0.handle, flags, timeout)
            },
            ffi::egl::FALSE as ffi::egl::types::EGLint,
        )
        .map_err(|err| {
            tracing::warn!(?err, "Waiting for fence was interrupted");
            Interrupted
        })?;

        Ok(())
        // while !self.is_signaled() {
        //     if start.elapsed() >=  {
        //         break;
        //     }
        // }
        // while !self.is_signaled() {}
    }
}
impl Renderer for SkiaRenderer {
    type Error = GlesError;
    type TextureId = SkiaTexture;
    type Frame<'a> = SkiaFrame<'a>;

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
    #[profiling::function]
    fn render(
        &mut self,
        output_size: Size<i32, Physical>,
        _dst_transform: Transform,
    ) -> Result<Self::Frame<'_>, Self::Error> {
        let id = self.id();
        let current_target = self.current_target.as_ref().unwrap();
        let buffer = self.buffers.get(current_target).unwrap();

        self.target_renderer
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
                // SkiaSurface::new_with_texture(
                //     output_size.w,
                //     output_size.h,
                //     0_usize,
                //     // 8_usize,
                //     buffer.tex_id,
                //     color_type,
                //     context,
                //     buffer.origin,
                // )
            });

        unsafe {
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, buffer.fbo);

            let status = self.gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);

            if status != ffi::FRAMEBUFFER_COMPLETE {
                println!("framebuffer incomplete");
                return Err(GlesError::FramebufferBindingError);
            }
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
        }
        let surface = self
            .target_renderer
            .get_mut(self.current_target.as_ref().unwrap())
            .unwrap();

        Ok(SkiaFrame {
            skia_surface: surface.clone(),
            size: output_size,
            renderer: self,
            id,
        })
    }

    fn wait(
        &mut self,
        sync: &smithay::backend::renderer::sync::SyncPoint,
    ) -> Result<(), Self::Error> {
        let display = self.egl_context().display();

        // if the sync point holds a EGLFence we can try
        // to directly insert it in our context
        if let Some(fence) = sync.get::<EGLFence>() {
            if fence.wait(display).is_ok() {
                return Ok(());
            }
        }

        // alternative we try to create a temporary fence
        // out of the native fence if available and try
        // to insert it in our context
        if let Some(native) = EGLFence::supports_importing(display)
            .then(|| sync.export())
            .flatten()
        {
            if let Ok(fence) = EGLFence::import(display, native) {
                if fence.wait(display).is_ok() {
                    return Ok(());
                }
            }
        }

        // if everything above failed we can only
        // block until the sync point has been reached
        sync.wait().map_err(|_| GlesError::SyncInterrupted)
    }
}

impl SkiaRenderer {
    #[profiling::function]
    fn import_skia_image_from_texture(&mut self, texture: &GlesTexture) -> Option<skia::Image> {
        #[cfg(feature = "profile-with-puffin")]
        profiling::scope!("import_skia_image_from_texture");
        let context = self.context.as_mut().unwrap();

        let target = ffi::TEXTURE_2D;

        let size = skia::Point {
            x: texture.width() as f32,
            y: texture.height() as f32,
        };
        unsafe {
            let gl_format = texture.format().map_or(ffi::RGBA8, |fourcc| {
                fourcc_to_gl_formats(fourcc).map_or(ffi::RGBA8, |(internal, _, _)| internal)
            });
            let skia_format = match gl_format {
                ffi::RGBA | ffi::RGBA8 => Some(skia::gpu::gl::Format::RGBA8),
                ffi::BGRA_EXT => Some(skia::gpu::gl::Format::BGRA8),
                ffi::RGB8 => Some(skia::gpu::gl::Format::RGB8),
                ffi::RGB10_A2 => Some(skia::gpu::gl::Format::RGB10_A2),
                ffi::RGBA16F => Some(skia::gpu::gl::Format::RGBA16F),
                _ => None,
            };
            let skia_color = match gl_format {
                ffi::RGBA | ffi::RGBA8 => Some(skia::ColorType::RGBA8888),
                ffi::BGRA_EXT => Some(skia::ColorType::BGRA8888),
                ffi::RGB8 => Some(skia::ColorType::RGB888x),
                ffi::RGB10_A2 => Some(skia::ColorType::RGBA1010102),
                ffi::RGBA16F => Some(skia::ColorType::RGBAF16),
                _ => None,
            };
            let texture_info = skia::gpu::gl::TextureInfo {
                target,
                id: texture.tex_id(),
                format: skia_format.unwrap().into(),
                ..Default::default()
            };

            let texture = skia::gpu::backend_textures::make_gl(
                (size.x as i32, size.y as i32),
                skia::gpu::Mipmapped::No,
                texture_info,
                "",
            );

            let image = skia::Image::from_texture(
                context,
                &texture,
                skia::gpu::SurfaceOrigin::TopLeft,
                skia_color.unwrap(),
                skia::AlphaType::Premul,
                None,
            )
            .unwrap();
            if let Some(surface) = self.current_skia_renderer() {
                let mut ctx = surface.gr_context.clone();
                ctx.flush_and_submit_image(&image);
                // ctx.flush_submit_and_sync_cpu();
                // println!("flush image");
            }
            Some(image)
        }
    }
}
impl ImportMemWl for SkiaRenderer {
    #[profiling::function]
    fn import_shm_buffer(
        &mut self,
        buffer: &WlBuffer,
        surface: Option<&SurfaceData>,
        damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let texture = self
            .gl_renderer
            .import_shm_buffer(buffer, surface, damage)?;
        let has_alpha = texture
            .format()
            .map_or(false, |fourcc: Fourcc| has_alpha(fourcc));
        let image = self
            .import_skia_image_from_texture(&texture)
            .ok_or("")
            .map_err(|_| GlesError::MappingError)?;

        let format = texture.format();
        Ok(SkiaTexture {
            texture,
            image,
            has_alpha,
            format,
            egl_images: None,
            is_external: false,
            damage: Some(damage.to_vec()),
        })
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
    #[profiling::function]
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

        let format = texture.format();
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
            format,
            egl_images: None,
            is_external: false,
            damage: Some(damage.to_vec()),
        })
    }
}

impl ImportDma for SkiaRenderer {
    #[profiling::function]
    fn import_dmabuf(
        &mut self,
        dmabuf: &Dmabuf,
        damage: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        // self.gl_renderer.import_dmabuf(dmabuf, damage)
        let texture = self.import_dmabuf_internal(dmabuf, damage)?;
        Ok(texture)
    }
    fn dmabuf_formats(&self) -> FormatSet {
        self.gl_renderer.dmabuf_formats()
    }
}

impl ImportDmaWl for SkiaRenderer {
    fn import_dma_buffer(
        &mut self,
        buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
        _surface: Option<&smithay::wayland::compositor::SurfaceData>,
        damage: &[Rectangle<i32, Buffer>],
    ) -> Result<<Self as Renderer>::TextureId, <Self as Renderer>::Error> {
        let dmabuf = smithay::wayland::dmabuf::get_dmabuf(buffer)
            .expect("import_dma_buffer without checking buffer type?");
        self.import_dmabuf(dmabuf, Some(damage))
    }
}

impl ImportMem for SkiaRenderer {
    #[profiling::function]
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
        let format = texture.format();
        Ok(SkiaTexture {
            texture,
            image,
            has_alpha,
            format,
            egl_images: None,
            is_external: false,
            damage: None,
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
        self.flipped
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
        // surface.flush_and_submit();
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
            let mut data_vec = data.borrow_mut();
            let byte_row = info.min_row_bytes();

            // let pixmap = skia::Pixmap::new(&info, &data_vec, byte_row);

            // if !surface.read_pixels_to_pixmap(&pixmap, (0, 0)) {
            // panic!("read_pixels_to_pixmap failed");
            // }
            if !surface.read_pixels(&info, &mut data_vec, byte_row, (0, 0)) {
                panic!("read_pixels failed");
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

    fn can_read_texture(&mut self, texture: &Self::TextureId) -> Result<bool, Self::Error> {
        self.gl_renderer.can_read_texture(&texture.texture)
    }
}

impl Bind<Rc<EGLSurface>> for SkiaRenderer {
    fn bind(&mut self, surface: Rc<EGLSurface>) -> Result<(), <Self as Renderer>::Error> {
        unsafe {
            self.egl_context().make_current_with_surface(&surface)?;
            self.gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
        }
        let egl_surface = EGLSurfaceWrapper(surface.clone());
        let render_target = SkiaTarget::EGLSurface(egl_surface);

        if !self.buffers.contains_key(&render_target) {
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
            self.buffers.insert(render_target.clone(), sfbo);
        }
        self.current_target = Some(render_target);

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
        #[allow(clippy::mutable_key_type)]
        let buffers = self.buffers.borrow_mut();
        buffers
            .entry(SkiaTarget::Dmabuf(dmabuf.clone()))
            .or_insert_with(|| {
                tracing::trace!("Creating EGLImage for Dmabuf: {:?}", dmabuf);
                let image = egl_display.create_image_from_dmabuf(&dmabuf).unwrap();
                let mut texture = 0;
                // .map_err(GlesError::BindBufferEGLError)?;
                let size = dmabuf.size();
                let _width = size.w;
                let _height = size.h;

                unsafe {
                    self.gl.GenTextures(1, &mut texture);
                    self.gl.BindTexture(ffi::TEXTURE_2D, texture);
                    self.gl.EGLImageTargetTexture2DOES(ffi::TEXTURE_2D, image);

                    let mut rbo = 0;
                    self.gl.GenRenderbuffers(1, &mut rbo as *mut _);
                    self.gl.BindRenderbuffer(ffi::RENDERBUFFER, rbo);
                    // self.gl.RenderbufferStorageMultisample(ffi::RENDERBUFFER, 2, ffi::RGBA8, width, height);
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
                        tex_id: texture,
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

impl<'a> AsRef<SkiaFrame<'a>> for SkiaFrame<'a> {
    fn as_ref(&self) -> &SkiaFrame<'a> {
        self
    }
}

impl<'a> AsMut<SkiaFrame<'a>> for SkiaFrame<'a> {
    fn as_mut(&mut self) -> &mut SkiaFrame<'a> {
        self
    }
}
