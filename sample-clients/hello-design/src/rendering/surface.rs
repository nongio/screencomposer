use wayland_client::{protocol::wl_surface};
use skia_safe::Canvas;

/// Individual renderable surface with its own EGL surface
/// Can be used for main windows, subsurfaces, or any other surface type
pub struct SkiaSurface {
    wl_surface: wl_surface::WlSurface,
    egl_surface: khronos_egl::Surface,
    wl_egl_surface: wayland_egl::WlEglSurface,
    width: i32,
    height: i32,
    // Cached Skia surface to avoid recreating on every draw
    cached_surface: Option<skia_safe::Surface>,
}

impl SkiaSurface {
    /// Create a SkiaSurface from already-created parts
    /// This is called internally by SkiaContext
    pub(crate) fn new_from_parts(
        wl_surface: wl_surface::WlSurface,
        egl_surface: khronos_egl::Surface,
        wl_egl_surface: wayland_egl::WlEglSurface,
        width: i32,
        height: i32,
    ) -> Self {
        println!("Creating SkiaSurface with dimensions: {}x{}", width, height);
        Self {
            wl_surface,
            egl_surface,
            wl_egl_surface,
            width,
            height,
            cached_surface: None,
        }
    }

    /// Resize the surface
    pub fn resize(&mut self, width: i32, height: i32) {
        println!("Resizing SkiaSurface from {}x{} to {}x{}", self.width, self.height, width, height);
        self.width = width;
        self.height = height;
        self.wl_egl_surface.resize(width, height, 0, 0);
    }

    /// Draw on this surface using the provided context and drawing function
    pub fn draw<F>(&mut self, ctx: &mut super::SkiaContext, draw_fn: F)
    where
        F: FnOnce(&Canvas),
    {
        unsafe {
            // Load EGL - this is cached internally after first load, so cheap
            let egl = khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required().unwrap();
            
            // Make this surface's EGL surface current
            egl.make_current(
                ctx.egl_display(),
                Some(self.egl_surface),
                Some(self.egl_surface),
                Some(ctx.egl_context()),
            ).ok();
            
            // Create or reuse cached Skia surface
            if self.cached_surface.is_none() {
                // Query framebuffer info
                let mut fboid: gl::types::GLint = 0;
                gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid);
                
                let stencil = 8;
                
                // Create Skia backend render target
                let fb_info = skia_safe::gpu::gl::FramebufferInfo {
                    fboid: fboid as u32,
                    format: skia_safe::gpu::gl::Format::RGBA8.into(),
                    protected: skia_safe::gpu::Protected::No,
                };
                
                let backend_render_target = skia_safe::gpu::backend_render_targets::make_gl(
                    (self.width, self.height),
                    0,
                    stencil as usize,
                    fb_info,
                );
                
                self.cached_surface = skia_safe::gpu::surfaces::wrap_backend_render_target(
                    ctx.skia_context(),
                    &backend_render_target,
                    skia_safe::gpu::SurfaceOrigin::BottomLeft,
                    skia_safe::ColorType::RGBA8888,
                    None,
                    None,
                );
            }
            
            if let Some(ref mut skia_surface) = self.cached_surface {
                let canvas = skia_surface.canvas();
                
                // Scale canvas by 2x for HiDPI rendering (buffers are 2x size)
                canvas.save();
                canvas.scale((2.0, 2.0));
                
                // Call user's drawing function
                draw_fn(canvas);
                
                // Restore canvas state to prevent scale accumulation
                canvas.restore();
            }
            
            // Flush to GPU
            ctx.skia_context().flush_and_submit();
            
            // Swap buffers
            egl.swap_buffers(ctx.egl_display(), self.egl_surface).ok();
        }
    }

    /// Commit the surface and mark damage
    pub fn commit(&self) {
        self.wl_surface.damage_buffer(0, 0, self.width, self.height);
        self.wl_surface.commit();

    }

    /// Get the underlying Wayland surface
    pub fn wl_surface(&self) -> &wl_surface::WlSurface {
        &self.wl_surface
    }

    /// Get current width
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Get current height
    pub fn height(&self) -> i32 {
        self.height
    }
}
