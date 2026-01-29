use std::{collections::hash_map::HashMap, sync::Arc};

use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            gbm::{GbmAllocator, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::DrmCompositor, DrmDevice, DrmDeviceFd, DrmNode, DrmSurface,
            GbmBufferedSurface,
        },
        renderer::{
            damage::OutputDamageTracker,
            element::{RenderElement, RenderElementStates},
            multigpu::{gbm::GbmGlesBackend, GpuManager, MultiRenderer},
            sync::SyncPoint,
            Bind, DebugFlags, ExportMem, Offscreen, Renderer,
        },
        session::libseat::LibSeatSession,
        SwapBuffersError,
    },
    desktop::utils::OutputPresentationFeedback,
    reexports::{
        calloop::RegistrationToken,
        drm::control::{connector, crtc},
        wayland_server::{backend::GlobalId, DisplayHandle},
    },
    utils::{Physical, Rectangle},
    wayland::{
        dmabuf::{DmabufFeedback, DmabufGlobal, DmabufState},
        drm_lease::DrmLeaseState,
    },
};
use smithay_drm_extras::drm_scanner::DrmScanner;

use crate::skia_renderer::SkiaRenderer;

// Supported pixel formats for rendering
// We pick ARGB2101010 (10-bit) or ARGB8888 (8-bit) as they are widely supported.
pub const SUPPORTED_FORMATS: &[Fourcc] = &[
    Fourcc::Abgr2101010,
    Fourcc::Argb2101010,
    Fourcc::Abgr8888,
    Fourcc::Argb8888,
];

pub const SUPPORTED_FORMATS_8BIT_ONLY: &[Fourcc] = &[Fourcc::Abgr8888, Fourcc::Argb8888];

/// Multi-GPU renderer type for udev backend
pub type UdevRenderer<'a> = MultiRenderer<
    'a,
    'a,
    GbmGlesBackend<SkiaRenderer, DrmDeviceFd>,
    GbmGlesBackend<SkiaRenderer, DrmDeviceFd>,
>;

/// GBM-backed DRM surface for rendering
pub type RenderSurface =
    GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, Option<OutputPresentationFeedback>>;

/// DRM compositor using GBM allocation
pub type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

/// Unique identifier for a udev output (device + CRTC)
#[derive(Debug, PartialEq)]
pub struct UdevOutputId {
    pub device_id: DrmNode,
    pub crtc: crtc::Handle,
}

/// Main udev backend data
pub struct UdevData {
    pub session: LibSeatSession,
    pub(super) dh: DisplayHandle,
    pub(super) dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    pub(super) primary_gpu: DrmNode,
    pub(super) gpus: GpuManager<GbmGlesBackend<SkiaRenderer, DrmDeviceFd>>,
    pub(super) backends: HashMap<DrmNode, BackendData>,
    #[cfg(feature = "fps_ticker")]
    pub(super) fps_texture: Option<smithay::backend::renderer::multigpu::MultiTexture>,
    pub(super) debug_flags: DebugFlags,
}

impl UdevData {
    pub fn set_debug_flags(&mut self, flags: DebugFlags) {
        if self.debug_flags != flags {
            self.debug_flags = flags;

            for (_, backend) in self.backends.iter_mut() {
                for (_, surface) in backend.surfaces.iter_mut() {
                    surface.compositor.set_debug_flags(flags);
                }
            }
        }
    }

    pub fn debug_flags(&self) -> DebugFlags {
        self.debug_flags
    }
}

/// Per-device backend data
pub struct BackendData {
    pub(super) surfaces: HashMap<crtc::Handle, SurfaceData>,
    pub(super) non_desktop_connectors: Vec<(connector::Handle, crtc::Handle)>,
    pub(super) leasing_global: Option<DrmLeaseState>,
    pub(super) active_leases: Vec<smithay::wayland::drm_lease::DrmLease>,
    pub(super) gbm: GbmDevice<DrmDeviceFd>,
    pub(super) drm: DrmDevice,
    pub(super) drm_scanner: DrmScanner,
    pub(super) render_node: DrmNode,
    pub(super) registration_token: RegistrationToken,
}

/// Per-surface rendering data
pub struct SurfaceData {
    pub(super) dh: DisplayHandle,
    pub(super) device_id: DrmNode,
    pub(super) render_node: DrmNode,
    pub(super) global: Option<GlobalId>,
    pub(super) compositor: SurfaceComposition,
    #[cfg(feature = "fps_ticker")]
    pub(super) fps: fps_ticker::Fps,
    #[cfg(feature = "fps_ticker")]
    pub(super) fps_element:
        Option<crate::drawing::FpsElement<smithay::backend::renderer::multigpu::MultiTexture>>,
    pub(super) dmabuf_feedback: Option<DrmSurfaceDmabufFeedback>,
    /// Track whether we were in direct scanout mode on the previous frame
    /// Used to reset buffers when transitioning between modes
    pub(super) was_direct_scanout: bool,
    /// Rendering metrics
    pub(super) render_metrics: Option<Arc<crate::render_metrics::RenderMetrics>>,
}

impl Drop for SurfaceData {
    fn drop(&mut self) {
        if let Some(global) = self.global.take() {
            self.dh
                .remove_global::<crate::state::Otto<UdevData>>(global);
        }
    }
}

/// Surface composition strategy (direct scanout vs compositor)
pub enum SurfaceComposition {
    Surface {
        surface: RenderSurface,
        damage_tracker: OutputDamageTracker,
        debug_flags: DebugFlags,
    },
    Compositor(GbmDrmCompositor),
}

impl SurfaceComposition {
    #[profiling::function]
    pub fn frame_submitted(
        &mut self,
    ) -> Result<Option<Option<OutputPresentationFeedback>>, SwapBuffersError> {
        match self {
            SurfaceComposition::Compositor(c) => {
                c.frame_submitted().map_err(Into::<SwapBuffersError>::into)
            }
            SurfaceComposition::Surface { surface, .. } => surface
                .frame_submitted()
                .map_err(Into::<SwapBuffersError>::into),
        }
    }

    pub fn format(&self) -> smithay::reexports::gbm::Format {
        match self {
            SurfaceComposition::Compositor(c) => c.format(),
            SurfaceComposition::Surface { surface, .. } => surface.format(),
        }
    }

    pub fn surface(&self) -> &DrmSurface {
        match self {
            SurfaceComposition::Compositor(c) => c.surface(),
            SurfaceComposition::Surface { surface, .. } => surface.surface(),
        }
    }

    pub fn reset_buffers(&mut self) {
        match self {
            SurfaceComposition::Compositor(c) => c.reset_buffers(),
            SurfaceComposition::Surface { surface, .. } => surface.reset_buffers(),
        }
    }

    #[profiling::function]
    pub fn queue_frame(
        &mut self,
        sync: Option<SyncPoint>,
        damage: Option<Vec<Rectangle<i32, Physical>>>,
        user_data: Option<OutputPresentationFeedback>,
    ) -> Result<(), SwapBuffersError> {
        match self {
            SurfaceComposition::Surface { surface, .. } => surface
                .queue_buffer(sync, damage, user_data)
                .map_err(Into::<SwapBuffersError>::into),
            SurfaceComposition::Compositor(c) => c
                .queue_frame(user_data)
                .map_err(Into::<SwapBuffersError>::into),
        }
    }

    #[profiling::function]
    pub fn render_frame<R, E, Target>(
        &mut self,
        renderer: &mut R,
        elements: &[E],
        clear_color: [f32; 4],
    ) -> Result<SurfaceCompositorRenderResult, SwapBuffersError>
    where
        R: Renderer + Bind<Dmabuf> + Bind<Target> + Offscreen<Target> + ExportMem,
        <R as Renderer>::TextureId: 'static,
        <R as Renderer>::Error: Into<SwapBuffersError>,
        E: RenderElement<R>,
    {
        match self {
            SurfaceComposition::Surface {
                surface,
                damage_tracker,
                debug_flags,
            } => {
                let (dmabuf, age) = surface
                    .next_buffer()
                    .map_err(Into::<SwapBuffersError>::into)?;
                renderer
                    .bind(dmabuf)
                    .map_err(Into::<SwapBuffersError>::into)?;
                let current_debug_flags = renderer.debug_flags();
                renderer.set_debug_flags(*debug_flags);
                let res = damage_tracker
                    .render_output(renderer, age.into(), elements, clear_color)
                    .map(|res| {
                        #[cfg(feature = "renderer_sync")]
                        let _ = res.sync.wait();
                        let rendered = res.damage.is_some();
                        SurfaceCompositorRenderResult {
                            rendered,
                            damage: res.damage,
                            states: res.states,
                            sync: rendered.then_some(res.sync),
                        }
                    })
                    .map_err(|err| match err {
                        smithay::backend::renderer::damage::Error::Rendering(err) => err.into(),
                        other => {
                            tracing::error!("Unexpected damage tracker error: {:?}", other);
                            SwapBuffersError::ContextLost(Box::new(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("Damage tracker error: {:?}", other),
                            )))
                        }
                    });
                renderer.set_debug_flags(current_debug_flags);
                res
            }
            SurfaceComposition::Compositor(compositor) => compositor
                .render_frame(renderer, elements, clear_color)
                .map(|render_frame_result| {
                    #[cfg(feature = "renderer_sync")]
                    {
                        use smithay::backend::drm::compositor::PrimaryPlaneElement;
                        if let PrimaryPlaneElement::Swapchain(element) =
                            render_frame_result.primary_element
                        {
                            let _ = element.sync.wait();
                        }
                    }
                    SurfaceCompositorRenderResult {
                        rendered: !render_frame_result.is_empty,
                        damage: None,
                        states: render_frame_result.states,
                        sync: None,
                    }
                })
                .map_err(|err| match err {
                    smithay::backend::drm::compositor::RenderFrameError::PrepareFrame(err) => {
                        err.into()
                    }
                    smithay::backend::drm::compositor::RenderFrameError::RenderFrame(
                        smithay::backend::renderer::damage::Error::Rendering(err),
                    ) => err.into(),
                    other => {
                        tracing::error!("Unexpected render frame error: {:?}", other);
                        SwapBuffersError::ContextLost(Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Render frame error: {:?}", other),
                        )))
                    }
                }),
        }
    }

    pub fn set_debug_flags(&mut self, flags: DebugFlags) {
        match self {
            SurfaceComposition::Surface {
                surface,
                debug_flags,
                ..
            } => {
                *debug_flags = flags;
                surface.reset_buffers();
            }
            SurfaceComposition::Compositor(c) => c.set_debug_flags(flags),
        }
    }
}

/// Result of surface compositor rendering
pub struct SurfaceCompositorRenderResult<'a> {
    pub rendered: bool,
    pub states: RenderElementStates,
    pub sync: Option<SyncPoint>,
    pub damage: Option<&'a Vec<Rectangle<i32, Physical>>>,
}

/// Dmabuf feedback for a DRM surface (render vs scanout)
pub struct DrmSurfaceDmabufFeedback {
    pub render_feedback: DmabufFeedback,
    pub scanout_feedback: DmabufFeedback,
}

/// Error type for device addition
#[derive(Debug, thiserror::Error)]
pub enum DeviceAddError {
    #[error("Failed to open device using libseat: {0}")]
    DeviceOpen(smithay::backend::session::libseat::Error),
    #[error("Failed to initialize drm device: {0}")]
    DrmDevice(smithay::backend::drm::DrmError),
    #[error("Failed to initialize gbm device: {0}")]
    GbmDevice(std::io::Error),
    #[error("Failed to access drm node: {0}")]
    DrmNode(smithay::backend::drm::CreateDrmNodeError),
    #[error("Failed to add device to GpuManager: {0}")]
    AddNode(smithay::backend::egl::Error),
}

/// Outcome of a render operation
pub struct RenderOutcome {
    pub rendered: bool,
    /// Damage regions from the render.
    pub damage: Option<Vec<Rectangle<i32, Physical>>>,
}

impl RenderOutcome {
    pub fn skipped() -> Self {
        Self {
            rendered: false,
            damage: None,
        }
    }

    pub fn with_frame(rendered: bool, damage: Option<Vec<Rectangle<i32, Physical>>>) -> Self {
        Self { rendered, damage }
    }
}
