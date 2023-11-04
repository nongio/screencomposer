#[cfg(feature = "xwayland")]
use std::collections::{hash_map::HashMap, HashSet};

#[cfg(feature = "renderer_sync")]
use smithay::backend::drm::compositor::PrimaryPlaneElement;
// #[cfg(feature = "egl")]
#[cfg(feature = "debug")]
use smithay::backend::renderer::ImportMem;
use smithay::{
    backend::renderer::ImportEgl,
    reexports::calloop::{generic::Generic, Interest, LoopHandle, PostAction},
};
use smithay::{
    backend::{
        allocator::{
            dmabuf::{AnyError, Dmabuf, DmabufAllocator},
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            vulkan::{ImageUsageFlags, VulkanAllocator},
            Allocator, Fourcc,
        },
        drm::{
            compositor::DrmCompositor, CreateDrmNodeError, DrmDevice, DrmDeviceFd, DrmError,
            DrmNode, NodeType,
        },
        egl::{self, context::ContextPriority},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::texture::TextureBuffer,
            gles::GlesRenderer,
            multigpu::{gbm::GbmGlesBackend, GpuManager, MultiRenderer, MultiTexture},
            DebugFlags, ImportDma, ImportMemWl,
        },
        session::{
            libseat::{self, LibSeatSession},
            Event as SessionEvent, Session,
        },
        udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent},
        vulkan::{version::Version, Instance, PhysicalDevice},
    },
    desktop::utils::OutputPresentationFeedback,
    output::Output,
    reexports::{
        ash::vk::ExtPhysicalDeviceDrmFn,
        calloop::{Mode, RegistrationToken},
        drm::control::{connector, crtc},
        input::Libinput,
        wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1,
        wayland_server::{protocol::wl_surface, Display},
    },
    wayland::{
        dmabuf::{DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufState},
        drm_lease::{DrmLease, DrmLeaseState},
    },
};
use smithay_drm_extras::drm_scanner::DrmScanner;
use tracing::{error, info, warn};

use crate::{
    cursor::PointerElement,
    debug::fps::{FpsElement, FPS_NUMBERS_PNG},
    renderer::layers_renderer::LayersRenderer,
    state::{Backend, ScreenComposer},
    CalloopData,
};

use self::compositor::{SurfaceComposition, SurfaceData};

mod compositor;
mod dmabuf;
mod drm;
mod input;
mod state;

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

type UdevRenderer<'a, 'b> =
    MultiRenderer<'a, 'a, 'b, GbmGlesBackend<LayersRenderer>, GbmGlesBackend<LayersRenderer>>;

#[derive(Debug, PartialEq)]
struct UdevOutputId {
    device_id: DrmNode,
    crtc: crtc::Handle,
}

pub struct UdevData {
    pub session: LibSeatSession,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    primary_gpu: DrmNode,
    allocator: Option<Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>>,
    gpus: GpuManager<GbmGlesBackend<LayersRenderer>>,
    backends: HashMap<DrmNode, BackendData>,
    pointer_images: Vec<(xcursor::parser::Image, TextureBuffer<MultiTexture>)>,
    pointer_element: PointerElement<MultiTexture>,
    #[cfg(feature = "debug")]
    fps_texture: Option<MultiTexture>,
    pointer_image: crate::cursor::Cursor,
    debug_flags: DebugFlags,
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
        if let Err(err) = self
            .gpus
            .early_import(Some(self.primary_gpu), self.primary_gpu, surface)
        {
            warn!("Early buffer import failed: {}", err);
        }
    }
}

pub fn init_udev(
    event_loop_handle: LoopHandle<'static, CalloopData<UdevData>>,
) -> Result<ScreenComposer<UdevData>, &'static str> {
    let display: Display<ScreenComposer<UdevData>> = Display::new().unwrap();
    let display_handle = display.handle();

    /*
     * Initialize session
     */
    let (session, notifier) = match LibSeatSession::new() {
        Ok(ret) => ret,
        Err(err) => {
            error!("Could not initialize a session: {}", err);
            return Err("Could not initialize a session");
        }
    };

    /*
     * Initialize the compositor
     */
    let primary_gpu = if let Ok(var) = std::env::var("SCREENCOMPOSER_DRM_DEVICE") {
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
        dmabuf_state: None,
        session,
        primary_gpu,
        gpus,
        allocator: None,
        backends: HashMap::new(),
        pointer_image: crate::cursor::Cursor::load(),
        pointer_images: Vec::new(),
        pointer_element: PointerElement::default(),
        #[cfg(feature = "debug")]
        fps_texture: None,
        debug_flags: DebugFlags::empty(),
    };

    let dh = display_handle.clone();
    let mut state = ScreenComposer::new(event_loop_handle.clone(), dh, data, true);

    /*
     * Initialize the udev backend
     */
    let udev_backend = match UdevBackend::new(state.backend_data.seat_name()) {
        Ok(ret) => ret,
        Err(err) => {
            error!(error = ?err, "Failed to initialize udev backend");
            return Err("Failed to initialize udev backend");
        }
    };

    /*
     * Initialize libinput backend
     */
    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        state.backend_data.session.clone().into(),
    );
    libinput_context
        .udev_assign_seat(&state.backend_data.seat_name())
        .unwrap();
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    /*
     * Bind all our objects that get driven by the event loop
     */

    let dh = display_handle.clone();
    event_loop_handle
        .clone()
        .insert_source(libinput_backend, move |event, _, data| {
            data.state.process_input_event(&dh, event)
        })
        .unwrap();

    let handle = event_loop_handle.clone();
    event_loop_handle
        .clone()
        .insert_source(notifier, move |event, &mut (), data| match event {
            SessionEvent::PauseSession => {
                libinput_context.suspend();
                info!("pausing session");

                for backend in data.state.backend_data.backends.values_mut() {
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
                    .state
                    .backend_data
                    .backends
                    .iter_mut()
                    .map(|(handle, backend)| (*handle, backend))
                {
                    backend.drm.activate();
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
                    handle.insert_idle(move |data| data.state.render(node, None));
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

    let skip_vulkan = std::env::var("ANVIL_NO_VULKAN")
        .map(|x| {
            x == "1"
                || x.to_lowercase() == "true"
                || x.to_lowercase() == "yes"
                || x.to_lowercase() == "y"
        })
        .unwrap_or(false);

    if !skip_vulkan {
        if let Ok(instance) = Instance::new(Version::VERSION_1_2, None) {
            if let Some(physical_device) =
                PhysicalDevice::enumerate(&instance)
                    .ok()
                    .and_then(|devices| {
                        devices
                            .filter(|phd| phd.has_device_extension(ExtPhysicalDeviceDrmFn::name()))
                            .find(|phd| {
                                phd.primary_node().unwrap() == Some(primary_gpu)
                                    || phd.render_node().unwrap() == Some(primary_gpu)
                            })
                    })
            {
                match VulkanAllocator::new(
                    &physical_device,
                    ImageUsageFlags::COLOR_ATTACHMENT | ImageUsageFlags::SAMPLED,
                ) {
                    Ok(allocator) => {
                        state.backend_data.allocator = Some(Box::new(DmabufAllocator(allocator))
                            as Box<dyn Allocator<Buffer = Dmabuf, Error = AnyError>>);
                    }
                    Err(err) => {
                        warn!("Failed to create vulkan allocator: {}", err);
                    }
                }
            }
        }
    }

    if state.backend_data.allocator.is_none() {
        info!("No vulkan allocator found, using GBM.");
        let gbm = state
            .backend_data
            .backends
            .get(&primary_gpu)
            // If the primary_gpu failed to initialize, we likely have a kmsro device
            .or_else(|| state.backend_data.backends.values().next())
            // Don't fail, if there is no allocator. There is a chance, that this a single gpu system and we don't need one.
            .map(|backend| backend.gbm.clone());
        state.backend_data.allocator = gbm.map(|gbm| {
            Box::new(DmabufAllocator(GbmAllocator::new(
                gbm,
                GbmBufferFlags::RENDERING,
            ))) as Box<_>
        });
    }

    // #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let mut renderer = state
        .backend_data
        .gpus
        .single_renderer(&primary_gpu)
        .unwrap();

    #[cfg(feature = "debug")]
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

    // #[cfg(feature = "egl")]
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
    let dmabuf_formats = renderer.dmabuf_formats().collect::<Vec<_>>();
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

    event_loop_handle
        .clone()
        .insert_source(udev_backend, move |event, _, data| match event {
            UdevEvent::Added { device_id, path } => {
                if let Err(err) = DrmNode::from_dev_id(device_id)
                    .map_err(DeviceAddError::DrmNode)
                    .and_then(|node| data.state.device_added(node, &path))
                {
                    error!("Skipping device {device_id}: {err}");
                }
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    data.state.device_changed(node)
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    data.state.device_removed(node)
                }
            }
        })
        .unwrap();

    /*
     * Start XWayland if supported
     */
    // #[cfg(feature = "xwayland")]
    // if let Err(e) = state.xwayland.start(
    //     state.handle.clone(),
    //     None,
    //     std::iter::empty::<(OsString, OsString)>(),
    //     true,
    //     |_| {},
    // ) {
    //     error!("Failed to start XWayland: {}", e);
    // }
    event_loop_handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, data| {
                profiling::scope!("dispatch_clients");
                // Safety: we don't drop the display
                unsafe {
                    display.get_mut().dispatch_clients(&mut data.state).unwrap();
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("Failed to init wayland server source");
    Ok(state)
}

pub type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

struct DrmSurfaceDmabufFeedback {
    render_feedback: DmabufFeedback,
    scanout_feedback: DmabufFeedback,
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
    gpus: &mut GpuManager<GbmGlesBackend<LayersRenderer>>,
    composition: &SurfaceComposition,
) -> Option<DrmSurfaceDmabufFeedback> {
    let primary_formats = gpus
        .single_renderer(&primary_gpu)
        .ok()?
        .dmabuf_formats()
        .collect::<HashSet<_>>();

    let render_formats = gpus
        .single_renderer(&render_node)
        .ok()?
        .dmabuf_formats()
        .collect::<HashSet<_>>();

    let all_render_formats = primary_formats
        .iter()
        .chain(render_formats.iter())
        .copied()
        .collect::<HashSet<_>>();

    let surface = composition.surface();
    let planes = surface.planes().clone();

    // We limit the scan-out tranche to formats we can also render from
    // so that there is always a fallback render path available in case
    // the supplied buffer can not be scanned out directly
    let planes_formats = planes
        .primary
        .formats
        .into_iter()
        .chain(planes.overlay.into_iter().flat_map(|p| p.formats))
        .collect::<HashSet<_>>()
        .intersection(&all_render_formats)
        .copied()
        .collect::<Vec<_>>();

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
