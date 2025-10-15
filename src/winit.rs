use std::{
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};

#[cfg(feature = "perf-counters")]
use once_cell::sync::Lazy;
#[cfg(feature = "perf-counters")]
use std::sync::atomic::AtomicU64;
#[cfg(feature = "perf-counters")]
use std::time::Instant;

#[cfg(feature = "egl")]
use smithay::backend::renderer::ImportEgl;

use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        egl::{context::GlAttributes, EGLDevice},
        renderer::{
            damage::{Error as OutputDamageTrackerError, OutputDamageTracker},
            element::AsRenderElements,
            utils::{import_surface, RendererSurfaceState, RendererSurfaceStateUserData},
            ImportDma, ImportMemWl,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
        SwapBuffersError,
    },
    delegate_dmabuf,
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::EventLoop,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{protocol::wl_surface, Display},
        winit::{
            dpi::LogicalSize, dpi::Size, platform::pump_events::PumpStatus,
            window::WindowAttributes,
        },
    },
    utils::{IsAlive, Transform},
    wayland::{
        compositor::{self, with_states},
        dmabuf::{
            DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState,
            ImportNotifier,
        },
    },
};
use tracing::{error, info, warn};

use crate::{
    config::Config,
    drawing::*,
    render::*,
    render_elements::workspace_render_elements::WorkspaceRenderElements,
    shell::WindowElement,
    skia_renderer::{SkiaRenderer, SkiaTexture, SkiaTextureImage},
    state::{post_repaint, take_presentation_feedback, Backend, ScreenComposer},
};

#[cfg(feature = "debug")]
use smithay::reexports::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub const OUTPUT_NAME: &str = "winit";

#[cfg(feature = "perf-counters")]
static FRAME_TOTAL: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static FRAME_RENDERED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static FRAME_SUBMITTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static FRAME_LOG_STATE: Lazy<Mutex<FrameLogState>> = Lazy::new(|| Mutex::new(FrameLogState::new()));

#[cfg(feature = "perf-counters")]
struct FrameLogState {
    last_log: Instant,
    prev_total: u64,
    prev_rendered: u64,
    prev_submitted: u64,
}

#[cfg(feature = "perf-counters")]
impl FrameLogState {
    fn new() -> Self {
        Self {
            last_log: Instant::now(),
            prev_total: 0,
            prev_rendered: 0,
            prev_submitted: 0,
        }
    }
}

#[cfg(feature = "perf-counters")]
fn record_frame_result(has_rendered: bool, frame_submitted: bool) {
    FRAME_TOTAL.fetch_add(1, Ordering::Relaxed);
    if has_rendered {
        FRAME_RENDERED.fetch_add(1, Ordering::Relaxed);
    }
    if frame_submitted {
        FRAME_SUBMITTED.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-counters"))]
#[inline]
fn record_frame_result(_has_rendered: bool, _frame_submitted: bool) {}

#[cfg(not(feature = "perf-counters"))]
#[inline]
fn log_frame_stats() {}

#[cfg(feature = "perf-counters")]
fn log_frame_stats() {
    let mut state = FRAME_LOG_STATE.lock().unwrap();
    if state.last_log.elapsed() < Duration::from_secs(1) {
        return;
    }

    let total = FRAME_TOTAL.load(Ordering::Relaxed);
    let rendered = FRAME_RENDERED.load(Ordering::Relaxed);
    let submitted = FRAME_SUBMITTED.load(Ordering::Relaxed);

    let delta_total = total - state.prev_total;
    let delta_rendered = rendered - state.prev_rendered;
    let delta_submitted = submitted - state.prev_submitted;
    let delta_without_damage = delta_total.saturating_sub(delta_rendered);

    info!(
        target: "screen_composer::perf.frame",
        total_frames = total,
        frames_per_sec = delta_total,
        frames_with_damage = delta_rendered,
        frames_without_damage = delta_without_damage,
        frames_submitted = delta_submitted,
    );

    state.prev_total = total;
    state.prev_rendered = rendered;
    state.prev_submitted = submitted;
    state.last_log = Instant::now();
}

pub struct WinitData {
    backend: WinitGraphicsBackend<SkiaRenderer>,
    damage_tracker: OutputDamageTracker,
    dmabuf_state: (DmabufState, DmabufGlobal, Option<DmabufFeedback>),
    full_redraw: u8,
    #[cfg(feature = "fps_ticker")]
    pub fps: fps_ticker::Fps,
}

impl DmabufHandler for ScreenComposer<WinitData> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.backend_data.dmabuf_state.0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self
            .backend_data
            .backend
            .renderer()
            .import_dmabuf(&dmabuf, None)
            .is_ok()
        {
            let _ = notifier.successful::<ScreenComposer<WinitData>>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(ScreenComposer<WinitData>);

impl Backend for WinitData {
    fn seat_name(&self) -> String {
        String::from("winit")
    }
    fn reset_buffers(&mut self, _output: &Output) {
        self.full_redraw = 4;
    }
    fn early_import(&mut self, surface: &wl_surface::WlSurface) {
        with_states(surface, |states| {
            let _ = import_surface(self.backend.renderer(), states);
        });
    }
    fn texture_for_surface(
        &self,
        render_surface: &RendererSurfaceState,
    ) -> Option<SkiaTextureImage> {
        let tex = render_surface.texture::<SkiaRenderer>(99);
        tex.map(|t| t.clone().into())
    }
    fn set_cursor(&mut self, _image: &CursorImageStatus) {}
    fn renderer_context(&mut self) -> Option<lay_rs::skia::gpu::DirectContext> {
        let r = self.backend.renderer();
        r.context.clone()
    }
    fn request_redraw(&mut self) {
        self.full_redraw = self.full_redraw.max(2);
    }
}

pub fn run_winit() {
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Display::new().unwrap();
    let mut display_handle = display.handle();

    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let (mut backend, mut winit) = match winit::init_from_attributes_with_gl_attr::<SkiaRenderer>(
        WindowAttributes::default()
            .with_title("Screen Composer".to_string())
            .with_inner_size(Size::new(LogicalSize::new(1280.0, 1000.0)))
            .with_visible(true),
        GlAttributes {
            version: (3, 0),
            profile: None,
            debug: cfg!(debug_assertions),
            vsync: true,
        },
    ) {
        Ok(ret) => ret,
        Err(err) => {
            error!("Failed to initialize Winit backend: {}", err);
            return;
        }
    };
    let size = backend.window_size();

    let mode = Mode {
        size,
        refresh: 60_000,
    };
    let output = Output::new(
        OUTPUT_NAME.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "ScreenComposer".into(),
            model: "Winit".into(),
        },
    );
    let _global = output.create_global::<ScreenComposer<WinitData>>(&display.handle());
    let config_screen_scale = Config::with(|c| c.screen_scale);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        Some(smithay::output::Scale::Fractional(config_screen_scale)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    #[cfg(feature = "fps_ticker")]
    let fps_image = image::io::Reader::with_format(
        std::io::Cursor::new(FPS_NUMBERS_PNG),
        image::ImageFormat::Png,
    )
    .decode()
    .unwrap();
    #[cfg(feature = "fps_ticker")]
    let fps_texture = backend
        .renderer()
        .import_memory(
            &fps_image.to_rgba8(),
            Fourcc::Abgr8888,
            (fps_image.width() as i32, fps_image.height() as i32).into(),
            false,
        )
        .expect("Unable to upload FPS texture");
    #[cfg(feature = "fps_ticker")]
    let mut fps_element = FpsElement::new(fps_texture);

    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());

    let dmabuf_default_feedback = match render_node {
        Ok(Some(node)) => {
            let dmabuf_formats = backend.renderer().dmabuf_formats();
            let dmabuf_default_feedback = DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
                .build()
                .unwrap();
            Some(dmabuf_default_feedback)
        }
        Ok(None) => {
            warn!("failed to query render node, dmabuf will use v3");
            None
        }
        Err(err) => {
            warn!(?err, "failed to egl device for display, dmabuf will use v3");
            None
        }
    };

    // if we failed to build dmabuf feedback we fall back to dmabuf v3
    // Note: egl on Mesa requires either v4 or wl_drm (initialized with bind_wl_display)
    let dmabuf_state = if let Some(default_feedback) = dmabuf_default_feedback {
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state
            .create_global_with_default_feedback::<ScreenComposer<WinitData>>(
                &display.handle(),
                &default_feedback,
            );
        (dmabuf_state, dmabuf_global, Some(default_feedback))
    } else {
        let dmabuf_formats = backend.renderer().dmabuf_formats();
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state
            .create_global::<ScreenComposer<WinitData>>(&display.handle(), dmabuf_formats);
        (dmabuf_state, dmabuf_global, None)
    };

    #[cfg(feature = "egl")]
    if backend
        .renderer()
        .bind_wl_display(&display.handle())
        .is_ok()
    {
        info!("EGL hardware-acceleration enabled");
    };

    let data = {
        let damage_tracker = OutputDamageTracker::from_output(&output);

        WinitData {
            backend,
            damage_tracker,
            dmabuf_state,
            full_redraw: 0,
            #[cfg(feature = "fps_ticker")]
            fps: fps_ticker::Fps::default(),
        }
    };
    let mut state = ScreenComposer::init(display, event_loop.handle(), data, true);

    let root = state.scene_element.root_layer().unwrap();
    let scene_size = size;
    state
        .layers_engine
        .scene_set_size(scene_size.w as f32, scene_size.h as f32);
    root.set_size(
        lay_rs::types::Size::points(scene_size.w as f32, scene_size.h as f32),
        None,
    );

    state
        .shm_state
        .update_formats(state.backend_data.backend.renderer().shm_formats());

    state.workspaces.map_output(&output, (0, 0));

    #[cfg(feature = "xwayland")]
    state.start_xwayland();

    info!("Initialization completed, starting the main loop.");

    let mut pointer_element = PointerElement::<SkiaTexture>::default();

    // rendering / events loop
    while state.running.load(Ordering::SeqCst) {
        #[cfg(feature = "profile-with-puffin")]
        profiling::puffin::GlobalProfiler::lock().new_frame();

        #[cfg(feature = "fps_ticker")]
        state.backend_data.fps.tick();

        state.update_dnd();

        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, .. } => {
                // We only have one output
                let output = state.workspaces.outputs().next().unwrap().clone();
                state.workspaces.map_output(&output, (0, 0));
                let mode = Mode {
                    size,
                    refresh: 60_000,
                };
                let config_screen_scale = Config::with(|c| c.screen_scale);
                output.change_current_state(
                    Some(mode),
                    None,
                    Some(smithay::output::Scale::Fractional(config_screen_scale)),
                    None,
                );
                output.set_preferred(mode);
                let pointer_location = state.pointer.current_location();
                crate::shell::fixup_positions(&mut state.workspaces, pointer_location);
                state.scene_element.set_size(size.w as f32, size.h as f32);
                state.workspaces.set_screen_dimension(size.w, size.h);
                root.set_size(
                    lay_rs::types::Size::points(size.w as f32, size.h as f32),
                    None,
                );
            }
            WinitEvent::Input(event) => state.process_input_event_windowed(event, OUTPUT_NAME),
            _ => (),
        });

        if let PumpStatus::Exit(_) = status {
            state.running.store(false, Ordering::SeqCst);
            break;
        }

        let scene_has_damage = state.scene_element.update();
        let mut needs_redraw_soon;
        let pointer_active;
        // drawing logic
        {
            #[cfg(feature = "profile-with-puffin")]
            profiling::puffin::profile_scope!("drawing logic");
            let backend = &mut state.backend_data.backend;

            let mut cursor_guard = state.cursor_status.lock().unwrap();

            // draw the cursor as relevant
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Surface(ref surface) = *cursor_guard {
                reset = !surface.alive();
            }
            if reset {
                *cursor_guard = CursorImageStatus::default_named();
            }
            let cursor_visible = !matches!(*cursor_guard, CursorImageStatus::Surface(_));

            if let CursorImageStatus::Named(cursor) = *cursor_guard {
                backend.window().set_cursor(cursor);
            }

            pointer_element.set_status(cursor_guard.clone());

            #[cfg(feature = "fps_ticker")]
            let fps = state.backend_data.fps.avg().round() as u32;
            #[cfg(feature = "fps_ticker")]
            fps_element.update_fps(fps);

            let full_redraw = &mut state.backend_data.full_redraw;
            *full_redraw = full_redraw.saturating_sub(1);
            needs_redraw_soon = *full_redraw > 0;

            let damage_tracker = &mut state.backend_data.damage_tracker;

            let output_scale = output.current_scale().fractional_scale();
            let cursor_config_size = Config::with(|c| c.cursor_size);
            // let cursor_config_physical_size = cursor_config_size as f64 * output_scale;
            let (_cursor_phy_size, cursor_hotspot) = match *cursor_guard {
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
                                        (cursor_config_size as f64, cursor_config_size as f64)
                                            .into(),
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
                _ => (
                    (
                        cursor_config_size as f64 * output_scale,
                        cursor_config_size as f64 * output_scale,
                    )
                        .into(),
                    (0.0, 0.0).into(),
                ),
            };

            let cursor_rescale = 1.0; //cursor_config_physical_size / cursor_phy_size.w;

            let cursor_pos = state.pointer.current_location();
            let cursor_pos_scaled =
                (cursor_pos.to_physical(output_scale) - cursor_hotspot).to_i32_round();

            // println!("cursor phy size: {:?}, config_phy {:?} should_scale: {}", cursor_phy_size, cursor_config_physical_size, cursor_rescale);
            let pointer_uses_surface = !cursor_visible;
            let should_draw = scene_has_damage
                || needs_redraw_soon
                || pointer_uses_surface
                || state.dnd_icon.is_some();
            pointer_active = pointer_uses_surface || state.dnd_icon.is_some();

            #[cfg(feature = "debug")]
            let mut renderdoc = state.renderdoc.as_mut();

            if should_draw {
                let render_res = backend.bind().and_then(|_| {
                    #[cfg(feature = "debug")]
                    if let Some(renderdoc) = renderdoc.as_mut() {
                        renderdoc.start_frame_capture(
                            backend.renderer().egl_context().get_context_handle(),
                            backend
                                .window()
                                .window_handle()
                                .map(|handle| {
                                    if let RawWindowHandle::Wayland(handle) = handle.as_raw() {
                                        handle.surface.as_ptr()
                                    } else {
                                        std::ptr::null_mut()
                                    }
                                })
                                .unwrap_or_else(|_| std::ptr::null_mut()),
                        );
                    }
                    let age = if *full_redraw > 0 {
                        0
                    } else {
                        backend.buffer_age().unwrap_or(0)
                    };

                    let renderer = backend.renderer();

                    let mut elements = Vec::<WorkspaceRenderElements<_>>::new();

                    // Collect render elements
                    elements.extend(pointer_element.render_elements(
                        renderer,
                        cursor_pos_scaled,
                        (cursor_rescale).into(),
                        1.0,
                    ));
                    #[cfg(feature = "fps_ticker")]
                    elements.push(WorkspaceRenderElements::Fps(fps_element.clone()));

                    let scene_element = state.scene_element.clone();
                    elements.push(WorkspaceRenderElements::Scene(scene_element));

                    #[cfg(feature = "profile-with-puffin")]
                    profiling::puffin::profile_scope!("render_output");

                    // Get all window elements from all workspaces
                    let all_window_elements: Vec<&WindowElement> =
                        state.workspaces.spaces_elements().collect();

                    render_output(
                        &output,
                        &all_window_elements,
                        elements,
                        state.dnd_icon.as_ref(),
                        renderer,
                        damage_tracker,
                        age,
                    )
                    .map_err(|err| match err {
                        OutputDamageTrackerError::Rendering(err) => err.into(),
                        _ => unreachable!(),
                    })
                });

                match render_res {
                    Ok(render_output_result) => {
                        let has_rendered = render_output_result.damage.is_some();
                        let mut frame_submitted = false;
                        if let Some(damage) = render_output_result.damage {
                            match backend.submit(Some(damage)) {
                                Ok(_) => {
                                    frame_submitted = true;
                                }
                                Err(err) => warn!("Failed to submit buffer: {}", err),
                            }
                        }

                        #[cfg(feature = "debug")]
                        if let Some(renderdoc) = renderdoc.as_mut() {
                            renderdoc.end_frame_capture(
                                backend.renderer().egl_context().get_context_handle(),
                                backend
                                    .window()
                                    .window_handle()
                                    .map(|handle| {
                                        if let RawWindowHandle::Wayland(handle) = handle.as_raw() {
                                            handle.surface.as_ptr()
                                        } else {
                                            std::ptr::null_mut()
                                        }
                                    })
                                    .unwrap_or_else(|_| std::ptr::null_mut()),
                            );
                        }

                        backend.window().set_cursor_visible(cursor_visible);

                        let time = state.clock.now();
                        let all_window_elements: Vec<&WindowElement> =
                            state.workspaces.spaces_elements().collect();
                        post_repaint(
                            &output,
                            &render_output_result.states,
                            &all_window_elements,
                            None,
                            time,
                        );

                        record_frame_result(has_rendered, frame_submitted);
                        if has_rendered || frame_submitted {
                            needs_redraw_soon = true;
                        }

                        if has_rendered {
                            let all_window_elements: Vec<&WindowElement> =
                                state.workspaces.spaces_elements().collect();
                            let mut output_presentation_feedback = take_presentation_feedback(
                                &output,
                                &all_window_elements,
                                &render_output_result.states,
                            );
                            output_presentation_feedback.presented(
                                time,
                                output
                                    .current_mode()
                                    .map(|mode| {
                                        Duration::from_secs_f64(1_000f64 / mode.refresh as f64)
                                    })
                                    .unwrap_or_default(),
                                0,
                                wp_presentation_feedback::Kind::Vsync,
                            );
                        }
                    }
                    Err(SwapBuffersError::ContextLost(err)) => {
                        #[cfg(feature = "debug")]
                        if let Some(renderdoc) = renderdoc.as_mut() {
                            renderdoc.discard_frame_capture(
                                backend.renderer().egl_context().get_context_handle(),
                                backend
                                    .window()
                                    .window_handle()
                                    .map(|handle| {
                                        if let RawWindowHandle::Wayland(handle) = handle.as_raw() {
                                            handle.surface.as_ptr()
                                        } else {
                                            std::ptr::null_mut()
                                        }
                                    })
                                    .unwrap_or_else(|_| std::ptr::null_mut()),
                            );
                        }

                        error!("Critical Rendering Error: {}", err);
                        state.running.store(false, Ordering::SeqCst);
                    }
                    Err(err) => warn!("Rendering error: {}", err),
                }
            } else {
                backend.window().set_cursor_visible(cursor_visible);
                record_frame_result(false, false);
            }
        }
        log_frame_stats();
        // Rendering Done, prepare loop
        let wait_timeout = if needs_redraw_soon {
            Some(Duration::from_millis(1))
        } else if scene_has_damage || pointer_active {
            Some(Duration::from_millis(1))
        } else {
            Some(Duration::from_millis(16))
        };
        let result = event_loop.dispatch(wait_timeout, &mut state);
        if result.is_err() {
            state.running.store(false, Ordering::SeqCst);
        } else {
            state.workspaces.refresh_space();
            state.popups.cleanup();
            display_handle.flush_clients().unwrap();
        }
    }
}
