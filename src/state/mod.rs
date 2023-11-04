use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use layers::{engine::AnimationRef, renderer::skia_fbo::SkiaFboRenderer};
use smithay::{
    backend::renderer::{
        element::{
            default_primary_scanout_output_compare, utils::select_dmabuf_feedback,
            RenderElementStates,
        },
        gles::GlesRenderer,
        utils::CommitCounter,
    },
    desktop::{
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
            update_surface_primary_scanout_output, OutputPresentationFeedback,
        },
        PopupManager, Space, Window,
    },
    input::{
        keyboard::Keysym,
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopHandle, Mode},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason, ObjectId},
            protocol::wl_surface::WlSurface,
            DisplayHandle,
        },
    },
    utils::{Clock, Monotonic},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        dmabuf::DmabufFeedback,
        fractional_scale::with_fractional_scale,
        output::OutputManagerState,
        selection::{
            data_device::DataDeviceState, primary_selection::PrimarySelectionState,
            wlr_data_control::DataControlState,
        },
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

// use crate::{sc_layer_shell::ScLayerShellState, CalloopData};
use layers::prelude::*;
use std::cell::Cell;
use tracing::{info, warn};

use crate::CalloopData;

mod scene;
mod surface;
mod update;
#[derive(Clone)]
pub struct SurfaceLayer {
    pub layer: Layer,
    pub commit_counter: CommitCounter,
    pub parent: Option<ObjectId>,
}

pub struct ScreenComposer<BackendData: Backend + 'static + Sized> {
    pub backend_data: BackendData,
    pub running: Arc<AtomicBool>,

    // pub backend_data: BackendData,
    pub start_time: std::time::Instant,
    pub socket_name: Option<String>,
    pub display_handle: DisplayHandle,

    pub clock: Clock<Monotonic>,

    pub event_loop_handle: LoopHandle<'static, CalloopData<BackendData>>,

    // Wayland State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    // pub sc_shell_state: ScLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<ScreenComposer<BackendData>>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,

    // input-related fields
    pub suppressed_keys: Vec<Keysym>,
    pub cursor_status: Arc<Mutex<CursorImageStatus>>,
    pub pointer: PointerHandle<ScreenComposer<BackendData>>,
    pub seat: Seat<Self>,

    pub dnd_icon: Option<WlSurface>,

    // Layers State
    pub space: Space<Window>,
    pub skia_renderer: Option<Cell<SkiaFboRenderer>>,
    pub engine: LayersEngine,
    pub layers_map: HashMap<ObjectId, SurfaceLayer>,
    pub animations_map: HashMap<ObjectId, AnimationRef>,
    pub needs_redraw: bool,
    pub renderer: Option<GlesRenderer>,
}

impl<BackendData: Backend> ScreenComposer<BackendData> {
    pub fn new(
        event_loop_handle: LoopHandle<'static, CalloopData<BackendData>>,
        display_handle: DisplayHandle,
        backend_data: BackendData,
        listen_on_socket: bool,
    ) -> Self {
        let clock = Clock::new();

        // init wayland clients
        let socket_name = if listen_on_socket {
            let source = ListeningSocketSource::new_auto().unwrap();
            let socket_name = source.socket_name().to_string_lossy().into_owned();
            event_loop_handle
                .clone()
                .insert_source(source, |client_stream, _, data| {
                    if let Err(err) = data
                        .state
                        .display_handle
                        .insert_client(client_stream, Arc::new(ClientState::default()))
                    {
                        warn!("Error adding wayland client: {}", err);
                    };
                })
                .expect("Failed to init wayland socket source");
            info!(name = socket_name, "Listening on wayland socket");
            Some(socket_name)
        } else {
            None
        };

        let dh = display_handle;

        let start_time = std::time::Instant::now();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        // let sc_shell_state: ScLayerShellState = ScLayerShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        let data_control_state =
            DataControlState::new::<Self, _>(&dh, Some(&primary_selection_state), |_| true);

        let popups = PopupManager::default();

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        let pointer = seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        // Get the loop signal, used to stop the event loop
        // let loop_signal = event_loop.get_signal();
        let skia_renderer = None;
        let engine = LayersEngine::new();
        let renderer = None;
        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::default_named()));

        Self {
            running: Arc::new(AtomicBool::new(true)),
            backend_data,
            start_time,
            display_handle: dh,
            clock,
            space,
            event_loop_handle: event_loop_handle.clone(),
            socket_name,

            compositor_state,
            xdg_shell_state,
            // sc_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            cursor_status,
            data_device_state,
            data_control_state,
            primary_selection_state,
            popups,
            seat,
            pointer,
            skia_renderer,
            engine,
            needs_redraw: true,
            layers_map: HashMap::new(),
            animations_map: HashMap::new(),
            renderer,
            suppressed_keys: Vec::new(),
            dnd_icon: None,
        }
    }

    fn init_wayland_listener(
        // _display: &mut Display<ScreenComposer<BackendData>>,
        event_loop: &mut EventLoop<CalloopData<BackendData>>,
        listen_on_socket: bool,
    ) -> Option<String> {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        // let socket_name = listening_socket.socket_name().to_os_string();

        let handle = event_loop.handle();
        // init wayland clients
        let socket_name = if listen_on_socket {
            let source = ListeningSocketSource::new_auto().unwrap();
            let socket_name = source.socket_name().to_string_lossy().into_owned();
            handle
                .insert_source(source, |client_stream, _, data| {
                    if let Err(err) = data
                        .state
                        .display_handle
                        .insert_client(client_stream, Arc::new(ClientState::default()))
                    {
                        warn!("Error adding wayland client: {}", err);
                    };
                })
                .expect("Failed to init wayland socket source");
            info!(name = socket_name, "Listening on wayland socket");
            Some(socket_name)
        } else {
            None
        };
        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        event_loop
            .handle()
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        socket_name
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

pub trait Backend: 'static + Sized {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
}

#[derive(Debug, Copy, Clone)]
pub struct SurfaceDmabufFeedback<'a> {
    pub render_feedback: &'a DmabufFeedback,
    pub scanout_feedback: &'a DmabufFeedback,
}

#[profiling::function]
pub fn take_presentation_feedback(
    output: &Output,
    space: &Space<Window>,
    render_element_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut output_presentation_feedback = OutputPresentationFeedback::new(output);

    space.elements().for_each(|window| {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut output_presentation_feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }
    });
    let map = smithay::desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.take_presentation_feedback(
            &mut output_presentation_feedback,
            surface_primary_scanout_output,
            |surface, _| {
                surface_presentation_feedback_flags_from_states(surface, render_element_states)
            },
        );
    }

    output_presentation_feedback
}

#[profiling::function]
pub fn post_repaint(
    output: &Output,
    render_element_states: &RenderElementStates,
    space: &Space<Window>,
    dmabuf_feedback: Option<SurfaceDmabufFeedback<'_>>,
    time: impl Into<Duration>,
) {
    let time = time.into();
    let throttle = Some(Duration::from_secs(1));

    space.elements().for_each(|window| {
        window.with_surfaces(|surface, states| {
            let primary_scanout_output = update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_element_states,
                default_primary_scanout_output_compare,
            );

            if let Some(output) = primary_scanout_output {
                with_fractional_scale(states, |fraction_scale| {
                    fraction_scale.set_preferred_scale(output.current_scale().fractional_scale());
                });
            }
        });

        if space.outputs_for_element(window).contains(output) {
            window.send_frame(output, time, throttle, surface_primary_scanout_output);
            if let Some(dmabuf_feedback) = dmabuf_feedback {
                window.send_dmabuf_feedback(
                    output,
                    surface_primary_scanout_output,
                    |surface, _| {
                        select_dmabuf_feedback(
                            surface,
                            render_element_states,
                            dmabuf_feedback.render_feedback,
                            dmabuf_feedback.scanout_feedback,
                        )
                    },
                );
            }
        }
    });
    let map = smithay::desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.with_surfaces(|surface, states| {
            let primary_scanout_output = update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_element_states,
                default_primary_scanout_output_compare,
            );

            if let Some(output) = primary_scanout_output {
                with_fractional_scale(states, |fraction_scale| {
                    fraction_scale.set_preferred_scale(output.current_scale().fractional_scale());
                });
            }
        });

        layer_surface.send_frame(output, time, throttle, surface_primary_scanout_output);
        if let Some(dmabuf_feedback) = dmabuf_feedback {
            layer_surface.send_dmabuf_feedback(
                output,
                surface_primary_scanout_output,
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        dmabuf_feedback.render_feedback,
                        dmabuf_feedback.scanout_feedback,
                    )
                },
            );
        }
    }
}

// #[profiling::function]
// pub fn output_elements<R>(
//     output: &Output,
//     space: &Space<Window>,
//     custom_elements: impl IntoIterator<Item = CustomRenderElements<R>>,
//     renderer: &mut R,
//     show_window_preview: bool,
// ) -> (
//     Vec<OutputRenderElements<R, WindowRenderElement<R>>>,
//     [f32; 4],
// )
// where
//     R: Renderer + ImportAll + ImportMem,
//     R::TextureId: Clone + 'static,
// {
//     if let Some(window) = output
//         .user_data()
//         .get::<FullscreenSurface>()
//         .and_then(|f| f.get())
//     {
//         let scale = output.current_scale().fractional_scale().into();
//         let window_render_elements: Vec<WindowRenderElement<R>> =
//             AsRenderElements::<R>::render_elements(&window, renderer, (0, 0).into(), scale, 1.0);

//         let elements = custom_elements
//             .into_iter()
//             .map(OutputRenderElements::from)
//             .chain(
//                 window_render_elements
//                     .into_iter()
//                     .map(|e| OutputRenderElements::Window(Wrap::from(e))),
//             )
//             .collect::<Vec<_>>();
//         (elements, CLEAR_COLOR_FULLSCREEN)
//     } else {
//         let mut output_render_elements = custom_elements
//             .into_iter()
//             .map(OutputRenderElements::from)
//             .collect::<Vec<_>>();

//         if show_window_preview && space.elements_for_output(output).count() > 0 {
//             output_render_elements.extend(space_preview_elements(renderer, space, output));
//         }

//         let space_elements = smithay::desktop::space::space_render_elements::<_, WindowElement, _>(
//             renderer,
//             [space],
//             output,
//             1.0,
//         )
//         .expect("output without mode?");
//         output_render_elements.extend(space_elements.into_iter().map(OutputRenderElements::Space));

//         (output_render_elements, CLEAR_COLOR)
//     }
// }
