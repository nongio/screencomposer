use std::{
    collections::HashMap,
    ffi::OsString,
    os::unix::io::AsRawFd,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use layers::{engine::AnimationRef, renderer::skia_fbo::SkiaFboRenderer};
use smithay::{
    backend::{
        renderer::{gles::GlesRenderer, utils::CommitCounter},
        winit::WindowSize,
    },
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopHandle, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason, ObjectId},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Clock, Logical, Monotonic, Point},
    wayland::{
        compositor::{self, CompositorClientState, CompositorState, TraversalAction},
        data_device::DataDeviceState,
        output::OutputManagerState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::{sc_layer_shell::ScLayerShellState, CalloopData};
use layers::prelude::*;
use std::cell::Cell;
use tracing::{debug, error, info, trace, warn};

mod scene;
mod surface;
mod update;
#[derive(Clone)]
pub struct SurfaceLayer {
    pub layer: Layer,
    pub commit_counter: CommitCounter,
    pub parent: Option<ObjectId>,
}

pub struct ScreenComposer<BackendData: Backend + 'static> {
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
    pub sc_shell_state: ScLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<ScreenComposer<BackendData>>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub cursor_status: Arc<Mutex<CursorImageStatus>>,
    pub pointer: PointerHandle<ScreenComposer<BackendData>>,
    pub seat: Seat<Self>,

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
        display: &mut Display<Self>,
        backend_data: &BackendData,
        listen_on_socket: bool,
    ) -> Self {
        let clock = Clock::new().expect("failed to initialize clock");

        // init wayland clients
        let socket_name = if listen_on_socket {
            let source = ListeningSocketSource::new_auto().unwrap();
            let socket_name = source.socket_name().to_string_lossy().into_owned();
            event_loop_handle
                .insert_source(source, |client_stream, _, data| {
                    if let Err(err) = data
                        .display
                        .handle()
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
        event_loop_handle
            .insert_source(
                Generic::new(
                    display.backend().poll_fd().as_raw_fd(),
                    Interest::READ,
                    Mode::Level,
                ),
                |_, _, data| {
                    profiling::scope!("dispatch_clients");
                    data.display.dispatch_clients(&mut data.state).unwrap();
                    Ok(PostAction::Continue)
                },
            )
            .expect("Failed to init wayland server source");

        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let sc_shell_state: ScLayerShellState = ScLayerShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
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
        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::Default));

        Self {
            running: Arc::new(AtomicBool::new(true)),

            start_time,
            display_handle: dh,
            clock,
            space,
            event_loop_handle,
            socket_name,

            compositor_state,
            xdg_shell_state,
            sc_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            cursor_status,
            data_device_state,
            popups,
            seat,
            pointer,
            skia_renderer,
            engine,
            needs_redraw: true,
            layers_map: HashMap::new(),
            animations_map: HashMap::new(),
            renderer,
        }
    }

    fn init_wayland_listener(
        display: &mut Display<ScreenComposer<BackendData>>,
        event_loop: &mut EventLoop<CalloopData<BackendData>>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        let handle = event_loop.handle();

        event_loop
            .handle()
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display
                    .handle()
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        handle
            .insert_source(
                Generic::new(
                    display.backend().poll_fd().as_raw_fd(),
                    Interest::READ,
                    Mode::Level,
                ),
                |_, _, state| {
                    state.display.dispatch_clients(&mut state.state).unwrap();
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

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

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
}
