use std::{
    collections::hash_map::HashMap,
    io,
    path::Path,
    sync::{atomic::Ordering, Mutex},
    time::{Duration, Instant},
};

use crate::{
    config::Config,
    cursor::Cursor,
    render_elements::{output_render_elements::OutputRenderElements, scene_element::SceneElement},
    shell::WindowRenderElement,
    skia_renderer::SkiaTextureImage,
    state::SurfaceDmabufFeedback,
};
use crate::{
    drawing::*,
    render::*,
    render_elements::workspace_render_elements::WorkspaceRenderElements,
    shell::WindowElement,
    skia_renderer::{SkiaGLesFbo, SkiaRenderer},
    state::{post_repaint, take_presentation_feedback, Backend, ScreenComposer},
};
#[cfg(feature = "renderer_sync")]
use smithay::backend::drm::compositor::PrimaryPlaneElement;
#[cfg(feature = "egl")]
use smithay::backend::renderer::ImportEgl;
#[cfg(feature = "fps_ticker")]
use smithay::backend::renderer::ImportMem;
use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc,
        },
        drm::{
            compositor::DrmCompositor, CreateDrmNodeError, DrmAccessError, DrmDevice, DrmDeviceFd,
            DrmError, DrmEvent, DrmEventMetadata, DrmNode, DrmSurface, GbmBufferedSurface,
            NodeType,
        },
        egl::{self, context::ContextPriority, EGLDevice, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            damage::{Error as OutputDamageTrackerError, OutputDamageTracker},
            element::{
                texture::TextureBuffer, AsRenderElements, RenderElement, RenderElementStates,
            },
            multigpu::{gbm::GbmGlesBackend, GpuManager, MultiRenderer, MultiTexture},
            sync::SyncPoint,
            utils::{import_surface, RendererSurfaceStateUserData},
            Bind, DebugFlags, ExportMem, ImportDma, ImportMemWl, Offscreen, Renderer,
        },
        session::{
            libseat::{self, LibSeatSession},
            Event as SessionEvent, Session,
        },
        udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent},
        SwapBuffersError,
    },
    delegate_dmabuf, delegate_drm_lease,
    desktop::{space::Space, utils::OutputPresentationFeedback},
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::{Mode as WlMode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop, LoopHandle, RegistrationToken,
        },
        drm::{
            control::{
                connector::{self, SubPixel},
                crtc, Device, ModeTypeFlags,
            },
            Device as _,
        },
        input::Libinput,
        rustix::fs::OFlags,
        wayland_protocols::wp::{
            linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1,
            presentation_time::server::wp_presentation_feedback,
        },
        wayland_server::{backend::GlobalId, protocol::wl_surface, Display, DisplayHandle},
    },
    utils::{
        Clock, DeviceFd, IsAlive, Logical, Monotonic, Physical, Point, Rectangle, Scale, Transform,
    },
    wayland::{
        compositor,
        dmabuf::{
            DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState,
            ImportNotifier,
        },
        drm_lease::{
            DrmLease, DrmLeaseBuilder, DrmLeaseHandler, DrmLeaseRequest, DrmLeaseState,
            LeaseRejected,
        },
    },
};
use smithay_drm_extras::{
    drm_scanner::{DrmScanEvent, DrmScanner},
    edid::EdidInfo,
};
use tracing::{debug, error, info, trace, warn};

// we cannot simply pick the first supported format of the intersection of *all* formats, because:
// - we do not want something like Abgr4444, which looses color information, if something better is available
// - some formats might perform terribly
// - we might need some work-arounds, if one supports modifiers, but the other does not
//
// So lets just pick `ARGB2101010` (10-bit) or `ARGB8888` (8-bit) for now, they are widely supported.
const SUPPORTED_FORMATS: &[Fourcc] = &[
    Fourcc::Abgr2101010,
    Fourcc::Argb2101010,
    Fourcc::Abgr8888,
    Fourcc::Argb8888,
];
const SUPPORTED_FORMATS_8BIT_ONLY: &[Fourcc] = &[Fourcc::Abgr8888, Fourcc::Argb8888];

pub type UdevRenderer<'a> = MultiRenderer<
    'a,
    'a,
    GbmGlesBackend<SkiaRenderer, DrmDeviceFd>,
    GbmGlesBackend<SkiaRenderer, DrmDeviceFd>,
>;

#[derive(Debug, PartialEq)]
struct UdevOutputId {
    device_id: DrmNode,
    crtc: crtc::Handle,
}

pub struct UdevData {
    pub session: LibSeatSession,
    dh: DisplayHandle,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    primary_gpu: DrmNode,
    gpus: GpuManager<GbmGlesBackend<SkiaRenderer, DrmDeviceFd>>,
    backends: HashMap<DrmNode, BackendData>,
    pointer_images: Vec<(xcursor::parser::Image, TextureBuffer<MultiTexture>)>,
    pointer_element: PointerElement<MultiTexture>,
    #[cfg(feature = "fps_ticker")]
    fps_texture: Option<MultiTexture>,
    debug_flags: DebugFlags,
    cursor_manager: Cursor,
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

impl DmabufHandler for ScreenComposer<UdevData> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.backend_data.dmabuf_state.as_mut().unwrap().0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self
            .backend_data
            .gpus
            .single_renderer(&self.backend_data.primary_gpu)
            .and_then(|mut renderer| renderer.import_dmabuf(&dmabuf, None))
            .is_ok()
        {
            let _ = notifier.successful::<ScreenComposer<UdevData>>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(ScreenComposer<UdevData>);

impl Backend for UdevData {
    const HAS_RELATIVE_MOTION: bool = true;
    const HAS_GESTURES: bool = true;

    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn reset_buffers(&mut self, output: &Output) {
        if let Some(id) = output.user_data().get::<UdevOutputId>() {
            if let Some(gpu) = self.backends.get_mut(&id.device_id) {
                if let Some(surface) = gpu.surfaces.get_mut(&id.crtc) {
                    surface.compositor.reset_buffers();
                }
            }
        }
    }

    fn early_import(&mut self, surface: &wl_surface::WlSurface) {
        if let Err(err) = self.gpus.early_import(self.primary_gpu, surface) {
            tracing::warn!("Early buffer import failed: {}", err);
        }
        let mut r = self.gpus.single_renderer(&self.primary_gpu).unwrap();
        compositor::with_states(surface, |states| {
            if let Err(err) = import_surface(&mut r, states) {
                tracing::warn!("Early buffer import surface failed: {}", err);
            }
        });
    }

    fn texture_for_surface(
        &self,
        surface: &smithay::backend::renderer::utils::RendererSurfaceState,
    ) -> Option<SkiaTextureImage> {
        let tex = surface.texture::<UdevRenderer>(99);
        if let Some(multitexture) = tex {
            let texture =
                multitexture.get::<GbmGlesBackend<SkiaRenderer, DrmDeviceFd>>(&self.primary_gpu);
            if let Some(texture) = texture {
                return Some(texture.into());
            }
        }
        None
    }
    fn set_cursor(&mut self, image: &CursorImageStatus) {
        if let CursorImageStatus::Named(image) = image {
            self.cursor_manager.load_icon(image.name());
        }
    }
    fn renderer_context(&mut self) -> Option<lay_rs::skia::gpu::DirectContext> {
        let r = self.gpus.single_renderer(&self.primary_gpu).unwrap();
        let r = r.as_ref();
        r.context.clone()
    }
}

pub fn run_udev() {
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Display::new().unwrap();
    let mut display_handle = display.handle();

    /*
     * Initialize session
     */
    let (session, notifier) = match LibSeatSession::new() {
        Ok(ret) => ret,
        Err(err) => {
            error!("Could not initialize a session: {}", err);
            return;
        }
    };

    /*
     * Initialize the compositor
     */
    let primary_gpu = if let Ok(var) = std::env::var("ANVIL_DRM_DEVICE") {
        DrmNode::from_path(var).expect("Invalid drm device path")
    } else {
        primary_gpu(session.seat())
            .unwrap()
            .and_then(|x| {
                DrmNode::from_path(x)
                    .ok()?
                    .node_with_type(NodeType::Render)?
                    .ok()
            })
            .unwrap_or_else(|| {
                all_gpus(session.seat())
                    .unwrap()
                    .into_iter()
                    .find_map(|x| DrmNode::from_path(x).ok())
                    .expect("No GPU!")
            })
    };
    info!("Using {} as primary gpu.", primary_gpu);

    let gpus =
        GpuManager::new(GbmGlesBackend::with_context_priority(ContextPriority::High)).unwrap();

    let data = UdevData {
        dh: display_handle.clone(),
        dmabuf_state: None,
        session,
        primary_gpu,
        gpus,
        backends: HashMap::new(),
        pointer_images: Vec::new(),
        pointer_element: PointerElement::default(),
        #[cfg(feature = "fps_ticker")]
        fps_texture: None,
        debug_flags: DebugFlags::empty(),
        cursor_manager: Cursor::load(),
    };
    let mut state = ScreenComposer::init(display, event_loop.handle(), data, true);

    /*
     * Initialize the udev backend
     */
    let udev_backend = match UdevBackend::new(&state.seat_name) {
        Ok(ret) => ret,
        Err(err) => {
            error!(error = ?err, "Failed to initialize udev backend");
            return;
        }
    };

    /*
     * Initialize libinput backend
     */
    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        state.backend_data.session.clone().into(),
    );
    libinput_context.udev_assign_seat(&state.seat_name).unwrap();
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    /*
     * Bind all our objects that get driven by the event loop
     */
    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, data| {
            let dh = data.backend_data.dh.clone();
            data.process_input_event(&dh, event)
        })
        .unwrap();

    let handle = event_loop.handle();
    event_loop
        .handle()
        .insert_source(notifier, move |event, &mut (), data| match event {
            SessionEvent::PauseSession => {
                libinput_context.suspend();
                info!("pausing session");

                for backend in data.backend_data.backends.values_mut() {
                    backend.drm.pause();
                    backend.active_leases.clear();
                    if let Some(lease_global) = backend.leasing_global.as_mut() {
                        lease_global.suspend();
                    }
                }
            }
            SessionEvent::ActivateSession => {
                info!("resuming session");

                if let Err(err) = libinput_context.resume() {
                    error!("Failed to resume libinput context: {:?}", err);
                }
                for (node, backend) in data
                    .backend_data
                    .backends
                    .iter_mut()
                    .map(|(handle, backend)| (*handle, backend))
                {
                    let _ = backend.drm.activate(false);
                    if let Some(lease_global) = backend.leasing_global.as_mut() {
                        lease_global.resume::<ScreenComposer<UdevData>>();
                    }
                    for surface in backend.surfaces.values_mut() {
                        if let Err(err) = surface.compositor.surface().reset_state() {
                            warn!("Failed to reset drm surface state: {}", err);
                        }
                        // reset the buffers after resume to trigger a full redraw
                        // this is important after a vt switch as the primary plane
                        // has no content and damage tracking may prevent a redraw
                        // otherwise
                        surface.compositor.reset_buffers();
                    }
                    handle.insert_idle(move |data| data.render(node, None));
                }
            }
        })
        .unwrap();

    for (device_id, path) in udev_backend.device_list() {
        if let Err(err) = DrmNode::from_dev_id(device_id)
            .map_err(DeviceAddError::DrmNode)
            .and_then(|node| state.device_added(node, path))
        {
            error!("Skipping device {device_id}: {err}");
        }
    }
    state.shm_state.update_formats(
        state
            .backend_data
            .gpus
            .single_renderer(&primary_gpu)
            .unwrap()
            .shm_formats(),
    );

    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let mut renderer = state
        .backend_data
        .gpus
        .single_renderer(&primary_gpu)
        .unwrap();

    #[cfg(feature = "fps_ticker")]
    {
        let fps_image = image::io::Reader::with_format(
            std::io::Cursor::new(FPS_NUMBERS_PNG),
            image::ImageFormat::Png,
        )
        .decode()
        .unwrap();
        let fps_texture = renderer
            .import_memory(
                &fps_image.to_rgba8(),
                Fourcc::Abgr8888,
                (fps_image.width() as i32, fps_image.height() as i32).into(),
                false,
            )
            .expect("Unable to upload FPS texture");

        for backend in state.backend_data.backends.values_mut() {
            for surface in backend.surfaces.values_mut() {
                surface.fps_element = Some(FpsElement::new(fps_texture.clone()));
            }
        }
        state.backend_data.fps_texture = Some(fps_texture);
    }

    #[cfg(feature = "egl")]
    {
        info!(
            ?primary_gpu,
            "Trying to initialize EGL Hardware Acceleration",
        );
        match renderer.bind_wl_display(&display_handle) {
            Ok(_) => info!("EGL hardware-acceleration enabled"),
            Err(err) => info!(?err, "Failed to initialize EGL hardware-acceleration"),
        }
    }

    // init dmabuf support with format list from our primary gpu
    let dmabuf_formats = renderer.dmabuf_formats();
    let default_feedback = DmabufFeedbackBuilder::new(primary_gpu.dev_id(), dmabuf_formats)
        .build()
        .unwrap();
    let mut dmabuf_state = DmabufState::new();
    let global = dmabuf_state.create_global_with_default_feedback::<ScreenComposer<UdevData>>(
        &display_handle,
        &default_feedback,
    );
    state.backend_data.dmabuf_state = Some((dmabuf_state, global));

    let gpus = &mut state.backend_data.gpus;
    state
        .backend_data
        .backends
        .values_mut()
        .for_each(|backend_data| {
            // Update the per drm surface dmabuf feedback
            backend_data.surfaces.values_mut().for_each(|surface_data| {
                surface_data.dmabuf_feedback = surface_data.dmabuf_feedback.take().or_else(|| {
                    get_surface_dmabuf_feedback(
                        primary_gpu,
                        surface_data.render_node,
                        gpus,
                        &surface_data.compositor,
                    )
                });
            });
        });

    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, data| match event {
            UdevEvent::Added { device_id, path } => {
                if let Err(err) = DrmNode::from_dev_id(device_id)
                    .map_err(DeviceAddError::DrmNode)
                    .and_then(|node| data.device_added(node, &path))
                {
                    error!("Skipping device {device_id}: {err}");
                }
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    data.device_changed(node)
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    data.device_removed(node)
                }
            }
        })
        .unwrap();

    /*
     * Start XWayland if supported
     */
    #[cfg(feature = "xwayland")]
    state.start_xwayland();

    /*
     * And run our loop
     */

    // FIXME: check if we can delay this
    while state.running.load(Ordering::SeqCst) {
        let result = event_loop.dispatch(Some(Duration::from_millis(16)), &mut state);
        if result.is_err() {
            state.running.store(false, Ordering::SeqCst);
        } else {
            display_handle.flush_clients().unwrap();
        }
    }
}

impl DrmLeaseHandler for ScreenComposer<UdevData> {
    fn drm_lease_state(&mut self, node: DrmNode) -> &mut DrmLeaseState {
        self.backend_data
            .backends
            .get_mut(&node)
            .unwrap()
            .leasing_global
            .as_mut()
            .unwrap()
    }

    fn lease_request(
        &mut self,
        node: DrmNode,
        request: DrmLeaseRequest,
    ) -> Result<DrmLeaseBuilder, LeaseRejected> {
        let backend = self
            .backend_data
            .backends
            .get(&node)
            .ok_or(LeaseRejected::default())?;

        let mut builder = DrmLeaseBuilder::new(&backend.drm);
        for conn in request.connectors {
            if let Some((_, crtc)) = backend
                .non_desktop_connectors
                .iter()
                .find(|(handle, _)| *handle == conn)
            {
                builder.add_connector(conn);
                builder.add_crtc(*crtc);
                let planes = backend
                    .drm
                    .planes(crtc)
                    .map_err(LeaseRejected::with_cause)?;
                let (primary_plane, primary_plane_claim) = planes
                    .primary
                    .iter()
                    .find_map(|plane| {
                        backend
                            .drm
                            .claim_plane(plane.handle, *crtc)
                            .map(|claim| (plane, claim))
                    })
                    .ok_or_else(LeaseRejected::default)?;
                builder.add_plane(primary_plane.handle, primary_plane_claim);
                if let Some((cursor, claim)) = planes.cursor.iter().find_map(|plane| {
                    backend
                        .drm
                        .claim_plane(plane.handle, *crtc)
                        .map(|claim| (plane, claim))
                }) {
                    builder.add_plane(cursor.handle, claim);
                }
            } else {
                tracing::warn!(
                    ?conn,
                    "Lease requested for desktop connector, denying request"
                );
                return Err(LeaseRejected::default());
            }
        }

        Ok(builder)
    }

    fn new_active_lease(&mut self, node: DrmNode, lease: DrmLease) {
        let backend = self.backend_data.backends.get_mut(&node).unwrap();
        backend.active_leases.push(lease);
    }

    fn lease_destroyed(&mut self, node: DrmNode, lease: u32) {
        let backend = self.backend_data.backends.get_mut(&node).unwrap();
        backend.active_leases.retain(|l| l.id() != lease);
    }
}

delegate_drm_lease!(ScreenComposer<UdevData>);

pub type RenderSurface =
    GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, Option<OutputPresentationFeedback>>;

pub type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

enum SurfaceComposition {
    Surface {
        surface: RenderSurface,
        damage_tracker: OutputDamageTracker,
        debug_flags: DebugFlags,
    },
    Compositor(GbmDrmCompositor),
}

struct SurfaceCompositorRenderResult<'a> {
    rendered: bool,
    states: RenderElementStates,
    sync: Option<SyncPoint>,
    damage: Option<&'a Vec<Rectangle<i32, Physical>>>,
}

impl SurfaceComposition {
    #[profiling::function]
    fn frame_submitted(
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

    fn format(&self) -> smithay::reexports::gbm::Format {
        match self {
            SurfaceComposition::Compositor(c) => c.format(),
            SurfaceComposition::Surface { surface, .. } => surface.format(),
        }
    }

    fn surface(&self) -> &DrmSurface {
        match self {
            SurfaceComposition::Compositor(c) => c.surface(),
            SurfaceComposition::Surface { surface, .. } => surface.surface(),
        }
    }

    fn reset_buffers(&mut self) {
        match self {
            SurfaceComposition::Compositor(c) => c.reset_buffers(),
            SurfaceComposition::Surface { surface, .. } => surface.reset_buffers(),
        }
    }

    #[profiling::function]
    fn queue_frame(
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
    fn render_frame<R, E, Target>(
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
                        res.sync.wait();
                        let rendered = res.damage.is_some();
                        SurfaceCompositorRenderResult {
                            rendered,
                            damage: res.damage,
                            states: res.states,
                            sync: rendered.then_some(res.sync),
                        }
                    })
                    .map_err(|err| match err {
                        OutputDamageTrackerError::Rendering(err) => err.into(),
                        _ => unreachable!(),
                    });
                renderer.set_debug_flags(current_debug_flags);
                res
            }
            SurfaceComposition::Compositor(compositor) => compositor
                .render_frame(renderer, elements, clear_color)
                .map(|render_frame_result| {
                    #[cfg(feature = "renderer_sync")]
                    if let PrimaryPlaneElement::Swapchain(element) =
                        render_frame_result.primary_element
                    {
                        element.sync.wait();
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
                        OutputDamageTrackerError::Rendering(err),
                    ) => err.into(),
                    _ => unreachable!(),
                }),
        }
    }

    fn set_debug_flags(&mut self, flags: DebugFlags) {
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

struct DrmSurfaceDmabufFeedback {
    render_feedback: DmabufFeedback,
    scanout_feedback: DmabufFeedback,
}

struct SurfaceData {
    dh: DisplayHandle,
    device_id: DrmNode,
    render_node: DrmNode,
    global: Option<GlobalId>,
    compositor: SurfaceComposition,
    #[cfg(feature = "fps_ticker")]
    fps: fps_ticker::Fps,
    #[cfg(feature = "fps_ticker")]
    fps_element: Option<FpsElement<MultiTexture>>,
    dmabuf_feedback: Option<DrmSurfaceDmabufFeedback>,
}

impl Drop for SurfaceData {
    fn drop(&mut self) {
        if let Some(global) = self.global.take() {
            self.dh.remove_global::<ScreenComposer<UdevData>>(global);
        }
    }
}

struct BackendData {
    surfaces: HashMap<crtc::Handle, SurfaceData>,
    non_desktop_connectors: Vec<(connector::Handle, crtc::Handle)>,
    leasing_global: Option<DrmLeaseState>,
    active_leases: Vec<DrmLease>,
    gbm: GbmDevice<DrmDeviceFd>,
    drm: DrmDevice,
    drm_scanner: DrmScanner,
    render_node: DrmNode,
    registration_token: RegistrationToken,
}

#[derive(Debug, thiserror::Error)]
enum DeviceAddError {
    #[error("Failed to open device using libseat: {0}")]
    DeviceOpen(libseat::Error),
    #[error("Failed to initialize drm device: {0}")]
    DrmDevice(DrmError),
    #[error("Failed to initialize gbm device: {0}")]
    GbmDevice(std::io::Error),
    #[error("Failed to access drm node: {0}")]
    DrmNode(CreateDrmNodeError),
    #[error("Failed to add device to GpuManager: {0}")]
    AddNode(egl::Error),
}

fn get_surface_dmabuf_feedback(
    primary_gpu: DrmNode,
    render_node: DrmNode,
    gpus: &mut GpuManager<GbmGlesBackend<SkiaRenderer, DrmDeviceFd>>,
    composition: &SurfaceComposition,
) -> Option<DrmSurfaceDmabufFeedback> {
    let primary_formats = gpus.single_renderer(&primary_gpu).ok()?.dmabuf_formats();
    let render_formats = gpus.single_renderer(&render_node).ok()?.dmabuf_formats();

    let all_render_formats = primary_formats
        .iter()
        .chain(render_formats.iter())
        .copied()
        .collect::<FormatSet>();

    let surface = composition.surface();
    let planes = surface.planes().clone();

    // We limit the scan-out tranche to formats we can also render from
    // so that there is always a fallback render path available in case
    // the supplied buffer can not be scanned out directly
    let planes_formats = surface
        .plane_info()
        .formats
        .iter()
        .copied()
        .chain(planes.overlay.into_iter().flat_map(|p| p.formats))
        .collect::<FormatSet>()
        .intersection(&all_render_formats)
        .copied()
        .collect::<FormatSet>();

    let builder = DmabufFeedbackBuilder::new(primary_gpu.dev_id(), primary_formats);
    let render_feedback = builder
        .clone()
        .add_preference_tranche(render_node.dev_id(), None, render_formats.clone())
        .build()
        .unwrap();

    let scanout_feedback = builder
        .add_preference_tranche(
            surface.device_fd().dev_id().unwrap(),
            Some(zwp_linux_dmabuf_feedback_v1::TrancheFlags::Scanout),
            planes_formats,
        )
        .add_preference_tranche(render_node.dev_id(), None, render_formats)
        .build()
        .unwrap();

    Some(DrmSurfaceDmabufFeedback {
        render_feedback,
        scanout_feedback,
    })
}

impl ScreenComposer<UdevData> {
    fn device_added(&mut self, node: DrmNode, path: &Path) -> Result<(), DeviceAddError> {
        // Try to open the device
        let fd = self
            .backend_data
            .session
            .open(
                path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .map_err(DeviceAddError::DeviceOpen)?;

        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, notifier) =
            DrmDevice::new(fd.clone(), true).map_err(DeviceAddError::DrmDevice)?;
        let gbm = GbmDevice::new(fd).map_err(DeviceAddError::GbmDevice)?;

        let registration_token = self
            .handle
            .insert_source(
                notifier,
                move |event, metadata, data: &mut ScreenComposer<_>| match event {
                    DrmEvent::VBlank(crtc) => {
                        profiling::scope!("vblank", &format!("{crtc:?}"));
                        data.frame_finish(node, crtc, metadata);
                    }
                    DrmEvent::Error(error) => {
                        error!("{:?}", error);
                    }
                },
            )
            .unwrap();

        let render_node =
            EGLDevice::device_for_display(&unsafe { EGLDisplay::new(gbm.clone()).unwrap() })
                .ok()
                .and_then(|x| x.try_get_render_node().ok().flatten())
                .unwrap_or(node);

        self.backend_data
            .gpus
            .as_mut()
            .add_node(render_node, gbm.clone())
            .map_err(DeviceAddError::AddNode)?;

        self.backend_data.backends.insert(
            node,
            BackendData {
                registration_token,
                gbm,
                drm,
                drm_scanner: DrmScanner::new(),
                non_desktop_connectors: Vec::new(),
                render_node,
                surfaces: HashMap::new(),
                leasing_global: DrmLeaseState::new::<ScreenComposer<UdevData>>(
                    &self.display_handle,
                    &node,
                )
                .map_err(|err| {
                    // TODO replace with inspect_err, once stable
                    warn!(?err, "Failed to initialize drm lease global for: {}", node);
                    err
                })
                .ok(),
                active_leases: Vec::new(),
            },
        );

        self.device_changed(node);

        Ok(())
    }

    fn connector_connected(
        &mut self,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let mut renderer = self
            .backend_data
            .gpus
            .single_renderer(&device.render_node)
            .unwrap();
        let render_formats = renderer
            .as_mut()
            .egl_context()
            .dmabuf_render_formats()
            .clone();

        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );
        info!(?crtc, "Trying to setup connector {}", output_name,);

        let non_desktop = device
            .drm
            .get_properties(connector.handle())
            .ok()
            .and_then(|props| {
                let (info, value) = props
                    .into_iter()
                    .filter_map(|(handle, value)| {
                        let info = device.drm.get_property(handle).ok()?;

                        Some((info, value))
                    })
                    .find(|(info, _)| info.name().to_str() == Ok("non-desktop"))?;

                info.value_type().convert_value(value).as_boolean()
            })
            .unwrap_or(false);

        let (make, model) = EdidInfo::for_connector(&device.drm, connector.handle())
            .map(|info| (info.manufacturer, info.model))
            .unwrap_or_else(|| ("Unknown".into(), "Unknown".into()));

        if non_desktop {
            info!(
                "Connector {} is non-desktop, setting up for leasing",
                output_name
            );
            device
                .non_desktop_connectors
                .push((connector.handle(), crtc));
            if let Some(lease_state) = device.leasing_global.as_mut() {
                lease_state.add_connector::<ScreenComposer<UdevData>>(
                    connector.handle(),
                    output_name,
                    format!("{} {}", make, model),
                );
            }
        } else {
            let mode_id = connector
                .modes()
                .iter()
                .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
                .unwrap_or(0);

            let drm_mode = connector.modes()[mode_id];
            let mut wl_mode = WlMode::from(drm_mode);
            // FIXME monitor get preferred mode or from config
            wl_mode.refresh = 60 * 1000;
            let surface = match device
                .drm
                .create_surface(crtc, drm_mode, &[connector.handle()])
            {
                Ok(surface) => surface,
                Err(err) => {
                    warn!("Failed to create drm surface: {}", err);
                    return;
                }
            };
            let subpixel = match connector.subpixel() {
                SubPixel::Unknown => Subpixel::Unknown,
                SubPixel::None => Subpixel::None,
                SubPixel::NotImplemented => Subpixel::Unknown,
                SubPixel::HorizontalRgb => Subpixel::HorizontalRgb,
                SubPixel::HorizontalBgr => Subpixel::HorizontalBgr,
                SubPixel::VerticalRgb => Subpixel::VerticalRgb,
                SubPixel::VerticalBgr => Subpixel::VerticalBgr,
                _ => Subpixel::Unknown,
            };
            let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
            let output = Output::new(
                output_name,
                PhysicalProperties {
                    size: (phys_w as i32, phys_h as i32).into(),
                    subpixel,
                    make,
                    model,
                },
            );

            // FIXME handle multimonitor setup
            let root = self.scene_element.root_layer().unwrap();
            let w = wl_mode.size.w as f32;
            let h = wl_mode.size.h as f32;
            self.workspaces
                .set_screen_dimension(wl_mode.size.w, wl_mode.size.h);
            let scene_size = lay_rs::types::Size::points(w, h);
            root.layer.set_size(scene_size, None);
            self.scene_element.set_size(w, h);
            self.layers_engine.scene_set_size(w, h);

            let global = output.create_global::<ScreenComposer<UdevData>>(&self.display_handle);

            let x = self.workspaces.outputs().fold(0, |acc, o| {
                acc + self.workspaces.output_geometry(o).unwrap().size.w
            });
            let position = (x, 0).into();
            output.set_preferred(wl_mode);
            let screen_scale = Config::with(|c| c.screen_scale);
            output.change_current_state(
                Some(wl_mode),
                None,
                Some(smithay::output::Scale::Fractional(screen_scale)),
                Some(position),
            );

            self.workspaces.map_output(&output, position);

            output.user_data().insert_if_missing(|| UdevOutputId {
                crtc,
                device_id: node,
            });

            #[cfg(feature = "fps_ticker")]
            let fps_element = self.backend_data.fps_texture.clone().map(FpsElement::new);

            let allocator = GbmAllocator::new(
                device.gbm.clone(),
                GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
            );

            let color_formats = if std::env::var("ANVIL_DISABLE_10BIT").is_ok() {
                SUPPORTED_FORMATS_8BIT_ONLY
            } else {
                SUPPORTED_FORMATS
            };

            let compositor = if Config::with(|c| c.compositor_mode == "surface") {
                let gbm_surface = match GbmBufferedSurface::new(
                    surface,
                    allocator,
                    color_formats,
                    render_formats,
                ) {
                    Ok(renderer) => renderer,
                    Err(err) => {
                        warn!("Failed to create rendering surface: {}", err);
                        return;
                    }
                };
                SurfaceComposition::Surface {
                    surface: gbm_surface,
                    damage_tracker: OutputDamageTracker::from_output(&output),
                    debug_flags: self.backend_data.debug_flags,
                }
            } else {
                let driver = match device.drm.get_driver() {
                    Ok(driver) => driver,
                    Err(err) => {
                        warn!("Failed to query drm driver: {}", err);
                        return;
                    }
                };

                let mut planes = surface.planes().clone();

                // Using an overlay plane on a nvidia card breaks
                if driver
                    .name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("nvidia")
                    || driver
                        .description()
                        .to_string_lossy()
                        .to_lowercase()
                        .contains("nvidia")
                {
                    planes.overlay = vec![];
                }
                println!("Max cursor size: {:?}", device.drm.cursor_size());
                let mut compositor = match DrmCompositor::new(
                    &output,
                    surface,
                    Some(planes),
                    allocator,
                    device.gbm.clone(),
                    color_formats,
                    render_formats,
                    device.drm.cursor_size(),
                    Some(device.gbm.clone()),
                ) {
                    Ok(compositor) => compositor,
                    Err(err) => {
                        warn!("Failed to create drm compositor: {}", err);
                        return;
                    }
                };
                compositor.set_debug_flags(self.backend_data.debug_flags);
                SurfaceComposition::Compositor(compositor)
            };

            let dmabuf_feedback = get_surface_dmabuf_feedback(
                self.backend_data.primary_gpu,
                device.render_node,
                &mut self.backend_data.gpus,
                &compositor,
            );

            let surface = SurfaceData {
                dh: self.display_handle.clone(),
                device_id: node,
                render_node: device.render_node,
                global: Some(global),
                compositor,
                #[cfg(feature = "fps_ticker")]
                fps: fps_ticker::Fps::default(),
                #[cfg(feature = "fps_ticker")]
                fps_element,
                dmabuf_feedback,
            };

            device.surfaces.insert(crtc, surface);

            self.schedule_initial_render(node, crtc, self.handle.clone());
        }
    }

    fn connector_disconnected(
        &mut self,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        if let Some(pos) = device
            .non_desktop_connectors
            .iter()
            .position(|(handle, _)| *handle == connector.handle())
        {
            let _ = device.non_desktop_connectors.remove(pos);
            if let Some(leasing_state) = device.leasing_global.as_mut() {
                leasing_state.withdraw_connector(connector.handle());
            }
        } else {
            device.surfaces.remove(&crtc);

            let output = self
                .workspaces
                .outputs()
                .find(|o| {
                    o.user_data()
                        .get::<UdevOutputId>()
                        .map(|id| id.device_id == node && id.crtc == crtc)
                        .unwrap_or(false)
                })
                .cloned();

            if let Some(output) = output {
                self.workspaces.unmap_output(&output);
            }
        }
    }

    fn device_changed(&mut self, node: DrmNode) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let scan_result = match device.drm_scanner.scan_connectors(&device.drm) {
            Ok(scan_result) => scan_result,
            Err(err) => {
                tracing::warn!(?err, "Failed to scan connectors");
                return;
            }
        };

        for event in scan_result {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connector_connected(node, connector, crtc);
                }
                DrmScanEvent::Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connector_disconnected(node, connector, crtc);
                }
                _ => {}
            }
        }

        // fixup window coordinates
        crate::shell::fixup_positions(&mut self.workspaces, self.pointer.current_location());
    }

    fn device_removed(&mut self, node: DrmNode) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let crtcs: Vec<_> = device
            .drm_scanner
            .crtcs()
            .map(|(info, crtc)| (info.clone(), crtc))
            .collect();

        for (connector, crtc) in crtcs {
            self.connector_disconnected(node, connector, crtc);
        }

        debug!("Surfaces dropped");

        // drop the backends on this side
        if let Some(mut backend_data) = self.backend_data.backends.remove(&node) {
            if let Some(mut leasing_global) = backend_data.leasing_global.take() {
                leasing_global.disable_global::<ScreenComposer<UdevData>>();
            }

            self.backend_data
                .gpus
                .as_mut()
                .remove_node(&backend_data.render_node);

            self.handle.remove(backend_data.registration_token);

            debug!("Dropping device");
        }

        crate::shell::fixup_positions(&mut self.workspaces, self.pointer.current_location());
    }

    fn frame_finish(
        &mut self,
        dev_id: DrmNode,
        crtc: crtc::Handle,
        metadata: &mut Option<DrmEventMetadata>,
    ) {
        profiling::scope!("frame_finish", &format!("{crtc:?}"));

        let device_backend = match self.backend_data.backends.get_mut(&dev_id) {
            Some(backend) => backend,
            None => {
                error!("Trying to finish frame on non-existent backend {}", dev_id);
                return;
            }
        };

        let surface = match device_backend.surfaces.get_mut(&crtc) {
            Some(surface) => surface,
            None => {
                error!("Trying to finish frame on non-existent crtc {:?}", crtc);
                return;
            }
        };

        let output = if let Some(output) = self.workspaces.outputs().find(|o| {
            o.user_data().get::<UdevOutputId>()
                == Some(&UdevOutputId {
                    device_id: surface.device_id,
                    crtc,
                })
        }) {
            output.clone()
        } else {
            // somehow we got called with an invalid output
            return;
        };

        let schedule_render = match surface
            .compositor
            .frame_submitted()
            .map_err(Into::<SwapBuffersError>::into)
        {
            Ok(user_data) => {
                if let Some(mut feedback) = user_data.flatten() {
                    let tp = metadata.as_ref().and_then(|metadata| match metadata.time {
                        smithay::backend::drm::DrmEventTime::Monotonic(tp) => Some(tp),
                        smithay::backend::drm::DrmEventTime::Realtime(_) => None,
                    });
                    let seq = metadata
                        .as_ref()
                        .map(|metadata| metadata.sequence)
                        .unwrap_or(0);

                    let (clock, flags) = if let Some(tp) = tp {
                        (
                            tp.into(),
                            wp_presentation_feedback::Kind::Vsync
                                | wp_presentation_feedback::Kind::HwClock
                                | wp_presentation_feedback::Kind::HwCompletion,
                        )
                    } else {
                        (self.clock.now(), wp_presentation_feedback::Kind::Vsync)
                    };

                    feedback.presented(
                        clock,
                        output
                            .current_mode()
                            .map(|mode| Duration::from_secs_f64(1_000f64 / mode.refresh as f64))
                            .unwrap_or_default(),
                        seq as u64,
                        flags,
                    );
                }

                true
            }
            Err(err) => {
                warn!("Error during rendering: {:?}", err);
                match err {
                    SwapBuffersError::AlreadySwapped => true,
                    // If the device has been deactivated do not reschedule, this will be done
                    // by session resume
                    SwapBuffersError::TemporaryFailure(err)
                        if matches!(
                            err.downcast_ref::<DrmError>(),
                            Some(&DrmError::DeviceInactive)
                        ) =>
                    {
                        false
                    }
                    SwapBuffersError::TemporaryFailure(err) => matches!(
                        err.downcast_ref::<DrmError>(),
                        Some(DrmError::Access(DrmAccessError {
                            source,
                            ..
                        })) if source.kind() == io::ErrorKind::PermissionDenied
                    ),
                    SwapBuffersError::ContextLost(err) => panic!("Rendering loop lost: {}", err),
                }
            }
        };

        if schedule_render {
            let output_refresh = match output.current_mode() {
                Some(mode) => mode.refresh,
                None => return,
            };
            // What are we trying to solve by introducing a delay here:
            //
            // Basically it is all about latency of client provided buffers.
            // A client driven by frame callbacks will wait for a frame callback
            // to repaint and submit a new buffer. As we send frame callbacks
            // as part of the repaint in the compositor the latency would always
            // be approx. 2 frames. By introducing a delay before we repaint in
            // the compositor we can reduce the latency to approx. 1 frame + the
            // remaining duration from the repaint to the next VBlank.
            //
            // With the delay it is also possible to further reduce latency if
            // the client is driven by presentation feedback. As the presentation
            // feedback is directly sent after a VBlank the client can submit a
            // new buffer during the repaint delay that can hit the very next
            // VBlank, thus reducing the potential latency to below one frame.
            //
            // Choosing a good delay is a topic on its own so we just implement
            // a simple strategy here. We just split the duration between two
            // VBlanks into two steps, one for the client repaint and one for the
            // compositor repaint. Theoretically the repaint in the compositor should
            // be faster so we give the client a bit more time to repaint. On a typical
            // modern system the repaint in the compositor should not take more than 2ms
            // so this should be safe for refresh rates up to at least 120 Hz. For 120 Hz
            // this results in approx. 3.33ms time for repainting in the compositor.
            // A too big delay could result in missing the next VBlank in the compositor.
            //
            // A more complete solution could work on a sliding window analyzing past repaints
            // and do some prediction for the next repaint.
            let repaint_delay =
                Duration::from_millis(((1_000_000f32 / output_refresh as f32) * 0.6f32) as u64);

            let timer = if self.backend_data.primary_gpu != surface.render_node {
                // However, if we need to do a copy, that might not be enough.
                // (And without actual comparision to previous frames we cannot really know.)
                // So lets ignore that in those cases to avoid thrashing performance.
                trace!("scheduling repaint timer immediately on {:?}", crtc);
                Timer::immediate()
            } else {
                trace!(
                    "scheduling repaint timer with delay {:?} on {:?}",
                    repaint_delay,
                    crtc
                );
                // Timer::from_duration(repaint_delay)
                Timer::immediate()
            };

            self.handle
                .insert_source(timer, move |_, _, data| {
                    data.render(dev_id, Some(crtc));
                    TimeoutAction::Drop
                })
                .expect("failed to schedule frame timer");
        }
    }

    // If crtc is `Some()`, render it, else render all crtcs
    fn render(&mut self, node: DrmNode, crtc: Option<crtc::Handle>) {
        let device_backend = match self.backend_data.backends.get_mut(&node) {
            Some(backend) => backend,
            None => {
                error!("Trying to render on non-existent backend {}", node);
                return;
            }
        };

        if let Some(crtc) = crtc {
            self.render_surface(node, crtc);
        } else {
            let crtcs: Vec<_> = device_backend.surfaces.keys().copied().collect();
            for crtc in crtcs {
                self.render_surface(node, crtc);
            }
        };
    }

    fn render_surface(&mut self, node: DrmNode, crtc: crtc::Handle) {
        profiling::scope!("render_surface", &format!("{crtc:?}"));

        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let surface = if let Some(surface) = device.surfaces.get_mut(&crtc) {
            surface
        } else {
            return;
        };

        let start = Instant::now();

        let render_node = surface.render_node;
        let primary_gpu = self.backend_data.primary_gpu;
        let mut renderer = if primary_gpu == render_node {
            self.backend_data.gpus.single_renderer(&render_node)
        } else {
            let format = surface.compositor.format();

            self.backend_data
                .gpus
                .renderer(&primary_gpu, &render_node, format)
        }
        .unwrap();

        let output = if let Some(output) = self.workspaces.outputs().find(|o| {
            o.user_data().get::<UdevOutputId>()
                == Some(&UdevOutputId {
                    device_id: surface.device_id,
                    crtc,
                })
        }) {
            output.clone()
        } else {
            // somehow we got called with an invalid output
            return;
        };

        // let output_scale = output.current_scale().fractional_scale();
        // let integer_scale = output_scale.round() as u32;
        let config_scale = Config::with(|c| c.screen_scale);

        // TODO get scale from the rendersurface when supporting HiDPI
        let cursor_frame = self
            .backend_data
            .cursor_manager
            .get_image(config_scale as f32, self.clock.now().into());

        let pointer_width = cursor_frame.width as i32;

        let pointer_images = &mut self.backend_data.pointer_images;
        let pointer_image = pointer_images
            .iter()
            .find_map(|(image, texture)| {
                if image == &cursor_frame {
                    Some(texture.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let texture = TextureBuffer::from_memory(
                    &mut renderer,
                    &cursor_frame.pixels_rgba,
                    Fourcc::Abgr8888,
                    (cursor_frame.width as i32, cursor_frame.height as i32),
                    false,
                    2,
                    Transform::Normal,
                    None,
                )
                .expect("Failed to import cursor bitmap");
                pointer_images.push((cursor_frame, texture.clone()));
                texture
            });
        // set cursor
        self.backend_data
            .pointer_element
            .set_texture(pointer_image.clone());
        let pointer_scale = pointer_width as f64 / self.backend_data.cursor_manager.size as f64;
        let result = render_surface(
            surface,
            &mut renderer,
            self.workspaces.space(),
            &output,
            self.pointer.current_location(),
            // &pointer_image,
            &mut self.backend_data.pointer_element,
            pointer_scale,
            self.dnd_icon.as_ref(),
            &mut self.cursor_status.lock().unwrap(),
            &self.clock,
            self.scene_element.clone(),
        );
        {
            self.workspaces.refresh_space();
            self.popups.cleanup();
            self.update_dnd();
            self.scene_element.update();
        }
        let reschedule = match &result {
            Ok(has_rendered) => !has_rendered,
            Err(err) => {
                warn!("Error during rendering: {:?}", err);
                match err {
                    SwapBuffersError::AlreadySwapped => false,
                    SwapBuffersError::TemporaryFailure(err) => match err.downcast_ref::<DrmError>()
                    {
                        Some(DrmError::DeviceInactive) => true,
                        Some(DrmError::Access(DrmAccessError { source, .. })) => {
                            source.kind() == io::ErrorKind::PermissionDenied
                        }
                        _ => false,
                    },
                    SwapBuffersError::ContextLost(err) => panic!("Rendering loop lost: {}", err),
                }
            }
        };

        if reschedule {
            let output_refresh = match output.current_mode() {
                Some(mode) => mode.refresh,
                None => return,
            };
            // If reschedule is true we either hit a temporary failure or more likely rendering
            // did not cause any damage on the output. In this case we just re-schedule a repaint
            // after approx. one frame to re-test for damage.
            let reschedule_duration =
                Duration::from_millis((1_000_000f32 / output_refresh as f32) as u64);
            trace!(
                "reschedule repaint timer with delay {:?} on {:?}",
                reschedule_duration,
                crtc,
            );
            let timer = Timer::from_duration(reschedule_duration);
            self.handle
                .insert_source(timer, move |_, _, data| {
                    data.render(node, Some(crtc));
                    TimeoutAction::Drop
                })
                .expect("failed to schedule frame timer");
        } else {
            let elapsed = start.elapsed();
            tracing::trace!(?elapsed, "rendered surface");
        }

        profiling::finish_frame!();
    }

    fn schedule_initial_render(
        &mut self,
        node: DrmNode,
        crtc: crtc::Handle,
        evt_handle: LoopHandle<'static, ScreenComposer<UdevData>>,
    ) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let surface = if let Some(surface) = device.surfaces.get_mut(&crtc) {
            surface
        } else {
            return;
        };

        let node = surface.render_node;
        let result = {
            let mut renderer = self.backend_data.gpus.single_renderer(&node).unwrap();
            initial_render(surface, &mut renderer)
        };

        if let Err(err) = result {
            match err {
                SwapBuffersError::AlreadySwapped => {}
                SwapBuffersError::TemporaryFailure(err) => {
                    // TODO dont reschedule after 3(?) retries
                    warn!("Failed to submit page_flip: {}", err);
                    let handle = evt_handle.clone();
                    evt_handle
                        .insert_idle(move |data| data.schedule_initial_render(node, crtc, handle));
                }
                SwapBuffersError::ContextLost(err) => panic!("Rendering loop lost: {}", err),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[profiling::function]
fn render_surface<'a, 'b>(
    surface: &'a mut SurfaceData,
    renderer: &mut UdevRenderer<'a>,
    space: &Space<WindowElement>,
    output: &Output,
    pointer_location: Point<f64, Logical>,
    pointer_element: &mut PointerElement<MultiTexture>,
    _pointer_scale: f64,
    dnd_icon: Option<&wl_surface::WlSurface>,
    cursor_status: &mut CursorImageStatus,
    clock: &Clock<Monotonic>,
    scene_element: SceneElement,
) -> Result<bool, SwapBuffersError> {
    let output_geometry = space.output_geometry(output).unwrap();
    let scale = Scale::from(output.current_scale().fractional_scale());

    let mut workspace_render_elements: Vec<WorkspaceRenderElements<_>> = Vec::new();

    let output_scale = output.current_scale().fractional_scale();

    let cursor_config_size = Config::with(|c| c.cursor_size);
    let cursor_config_physical_size = cursor_config_size as f64 * output_scale;

    if output_geometry.to_f64().contains(pointer_location) {
        let (cursor_phy_size, cursor_hotspot) = match cursor_status {
            CursorImageStatus::Surface(ref surface) => {
                compositor::with_states(surface, |states| {
                    let data = states.data_map.get::<RendererSurfaceStateUserData>();
                    let (size, cursor_scale) = data
                        .map(|data| {
                            let data = data.lock().unwrap();
                            if let Some(view) = data.view().as_ref() {
                                let surface_scale = data.buffer_scale() as f64;
                                // println!("surface_scale: {}", surface_scale);
                                let src_view = view.src.to_physical(surface_scale);
                                (src_view.size, surface_scale)
                            } else {
                                (
                                    (cursor_config_size as f64, cursor_config_size as f64).into(),
                                    1.0,
                                )
                            }
                        })
                        .unwrap_or_else(|| {
                            (
                                (
                                    cursor_config_size as f64 * output_scale,
                                    cursor_config_size as f64 * output_scale,
                                )
                                    .into(),
                                1.0,
                            )
                        });
                    (
                        size,
                        states
                            .data_map
                            .get::<Mutex<CursorImageAttributes>>()
                            .unwrap()
                            .lock()
                            .unwrap()
                            .hotspot
                            .to_f64()
                            .to_physical(cursor_scale),
                    )
                })
            }
            CursorImageStatus::Named(_) => {
                let cursor_image = pointer_element
                    .cursor_manager
                    .get_image(output_scale as f32, clock.now().into());
                (
                    (cursor_image.width as f64, cursor_image.height as f64).into(),
                    (cursor_image.xhot as f64, cursor_image.yhot as f64).into(),
                )
            }
            _ => (
                (
                    cursor_config_size as f64 * output_scale,
                    cursor_config_size as f64 * output_scale,
                )
                    .into(),
                (0.0, 0.0).into(),
            ),
        };
        let cursor_pos = pointer_location - output_geometry.loc.to_f64();
        let cursor_pos_scaled = (cursor_pos.to_physical(scale) - cursor_hotspot).to_i32_round();

        let cursor_rescale = cursor_config_physical_size / cursor_phy_size.w;
        // // set cursor
        // pointer_element.set_texture(pointer_image.clone());
        // println!("rendering cursor: {:?}", rescale_cursor);
        // draw the cursor as relevant
        // println!("cursor phy size: {:?}, config_phy {:?} should_scale: {}", cursor_phy_size, cursor_config_physical_size, cursor_scale);

        {
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Surface(ref surface) = *cursor_status {
                reset = !surface.alive();
            }
            if reset {
                *cursor_status = CursorImageStatus::default_named();
            }

            pointer_element.set_status(cursor_status.clone());
        }
        workspace_render_elements.extend(pointer_element.render_elements(
            renderer,
            cursor_pos_scaled,
            cursor_rescale.into(),
            1.0,
        ));

        // draw the dnd icon if applicable
        // {
        //     if let Some(wl_surface) = dnd_icon.as_ref() {
        //         if wl_surface.alive() {
        //             custom_elements.extend(AsRenderElements::<UdevRenderer<'a>>::render_elements(
        //                 &SurfaceTree::from_surface(wl_surface),
        //                 renderer,
        //                 cursor_pos_scaled,
        //                 scale,
        //                 1.0,
        //             ));
        //         }
        //     }
        // }
    }

    #[cfg(feature = "fps_ticker")]
    if let Some(element) = surface.fps_element.as_mut() {
        element.update_fps(surface.fps.avg().round() as u32);
        surface.fps.tick();
        custom_elements.push(WorkspaceRenderElements::Fps(element.clone()));
    }
    workspace_render_elements.push(WorkspaceRenderElements::Scene(scene_element));

    let output_render_elements: Vec<OutputRenderElements<'a, _, WindowRenderElement<_>>> =
        workspace_render_elements
            .into_iter()
            .map(OutputRenderElements::from)
            .collect::<Vec<_>>();
    let (output_elements, clear_color) =
        output_elements(output, space, output_render_elements, dnd_icon, renderer);

    let SurfaceCompositorRenderResult {
        rendered,
        states,
        sync,
        damage,
    } = surface.compositor.render_frame::<_, _, SkiaGLesFbo>(
        renderer,
        &output_elements,
        clear_color,
    )?;

    post_repaint(
        output,
        &states,
        space,
        surface
            .dmabuf_feedback
            .as_ref()
            .map(|feedback| SurfaceDmabufFeedback {
                render_feedback: &feedback.render_feedback,
                scanout_feedback: &feedback.scanout_feedback,
            }),
        clock.now(),
    );

    if rendered {
        let output_presentation_feedback = take_presentation_feedback(output, space, &states);
        let damage = damage.cloned();
        surface
            .compositor
            .queue_frame(sync, damage, Some(output_presentation_feedback))
            .map_err(Into::<SwapBuffersError>::into)?;
    }

    Ok(rendered)
}

fn initial_render(
    surface: &mut SurfaceData,
    renderer: &mut UdevRenderer<'_>,
) -> Result<(), SwapBuffersError> {
    surface
        .compositor
        .render_frame::<_, WorkspaceRenderElements<_>, SkiaGLesFbo>(renderer, &[], CLEAR_COLOR)?;
    surface.compositor.queue_frame(None, None, None)?;
    surface.compositor.reset_buffers();

    Ok(())
}
