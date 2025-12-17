use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use lay_rs::{engine::Engine, prelude::taffy};
use tracing::{info, warn};

use smithay::{
    backend::renderer::{
        element::{
            default_primary_scanout_output_compare, utils::select_dmabuf_feedback,
            RenderElementStates,
        },
        utils::{RendererSurfaceState, RendererSurfaceStateUserData},
    },
    delegate_compositor, delegate_keyboard_shortcuts_inhibit, delegate_layer_shell,
    delegate_output, delegate_pointer_gestures, delegate_presentation, delegate_relative_pointer,
    delegate_shm, delegate_text_input_manager, delegate_viewporter,
    delegate_virtual_keyboard_manager, delegate_xdg_foreign, delegate_xdg_shell,
    desktop::{
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
            update_surface_primary_scanout_output, with_surfaces_surface_tree,
            OutputPresentationFeedback,
        },
        PopupManager,
    },
    input::{
        keyboard::{xkb, Keysym, ModifiersState, XkbConfig},
        pointer::{CursorIcon, CursorImageAttributes, CursorImageStatus, PointerHandle},
        Seat, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{
            channel::{channel, Event as ChannelEvent, Sender as ChannelSender},
            generic::Generic,
            Interest, LoopHandle, Mode, PostAction,
        },
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason, ObjectId},
            protocol::{wl_data_device_manager::DndAction, wl_surface::WlSurface},
            Display, DisplayHandle, Resource,
        },
    },
    utils::{self, Clock, Monotonic, SERIAL_COUNTER},
    wayland::{
        compositor::{
            self, CompositorClientState, CompositorState, SurfaceAttributes, SurfaceData,
            TraversalAction,
        },
        dmabuf::DmabufFeedback,
        fractional_scale::{with_fractional_scale, FractionalScaleManagerState},
        input_method::InputMethodManagerState,
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
            KeyboardShortcutsInhibitor,
        },
        output::{OutputHandler, OutputManagerState},
        pointer_constraints::PointerConstraintsState,
        pointer_gestures::PointerGesturesState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        security_context::{SecurityContext, SecurityContextState},
        selection::{
            data_device::DataDeviceState, primary_selection::PrimarySelectionState,
            wlr_data_control::DataControlState,
        },
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{decoration::XdgDecorationState, SurfaceCachedState, XdgShellState},
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
        tablet_manager::TabletManagerState,
        text_input::TextInputManagerState,
        viewporter::ViewporterState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_activation::XdgActivationState,
        xdg_foreign::{XdgForeignHandler, XdgForeignState},
    },
};

#[cfg(feature = "xwayland")]
use crate::cursor::Cursor;
use crate::{
    config::{Config, ModifierMaskLookup},
    focus::KeyboardFocusTarget,
    render_elements::scene_element::SceneElement,
    shell::{LayerShellSurface, WindowElement},
    skia_renderer::SkiaTextureImage,
    workspaces::{WindowViewBaseModel, WindowViewSurface, Workspaces},
};
#[cfg(feature = "xwayland")]
use smithay::{
    delegate_xwayland_keyboard_grab,
    utils::{Point, Size},
    wayland::selection::{SelectionSource, SelectionTarget},
    wayland::xwayland_keyboard_grab::{XWaylandKeyboardGrabHandler, XWaylandKeyboardGrabState},
    xwayland::{X11Wm, XWayland, XWaylandEvent},
};

pub struct CalloopData<BackendData: Backend + 'static> {
    pub state: ScreenComposer<BackendData>,
    pub display_handle: DisplayHandle,
}

#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub security_context: Option<SecurityContext>,
}
impl ClientData for ClientState {
    /// Notification that a client was initialized
    fn initialized(&self, _client_id: ClientId) {}
    /// Notification that a client is disconnected
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

pub struct ScreenComposer<BackendData: Backend + 'static> {
    pub backend_data: BackendData,
    pub socket_name: Option<String>,
    pub display_handle: DisplayHandle,
    pub running: Arc<AtomicBool>,
    pub handle: LoopHandle<'static, ScreenComposer<BackendData>>,
    pub loop_wakeup_sender: ChannelSender<()>,
    pub loop_wakeup_pending: Arc<AtomicBool>,

    // desktop
    pub popups: PopupManager,
    /// Cache mapping popup surface IDs to their root window surface IDs
    /// for fast lookup during commit/destroy without re-traversing the popup tree
    pub popup_root_cache: HashMap<ObjectId, ObjectId>,
    /// Compositor-owned layer shell surfaces, keyed by surface ObjectId
    pub layer_surfaces: HashMap<ObjectId, LayerShellSurface>,
    pub workspaces: Workspaces,

    // smithay state
    pub compositor_state: CompositorState,
    pub data_device_state: DataDeviceState,
    pub layer_shell_state: WlrLayerShellState,
    pub output_manager_state: OutputManagerState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,
    pub seat_state: SeatState<ScreenComposer<BackendData>>,
    pub keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState,
    pub shm_state: ShmState,
    pub viewporter_state: ViewporterState,
    pub xdg_activation_state: XdgActivationState,
    pub xdg_decoration_state: XdgDecorationState,
    pub xdg_shell_state: XdgShellState,
    pub presentation_state: PresentationState,
    pub fractional_scale_manager_state: FractionalScaleManagerState,
    pub xdg_foreign_state: XdgForeignState,

    #[cfg(feature = "xwayland")]
    pub xwayland_shell_state: xwayland_shell::XWaylandShellState,

    pub dnd_icon: Option<WlSurface>,

    // input-related fields
    pub suppressed_keys: Vec<Keysym>,
    pub keycode_remap: HashMap<u32, u32>,
    pub current_modifiers: ModifiersState,
    pub app_switcher_hold_modifiers: Option<ModifiersState>,
    pub modifier_masks: ModifierMaskLookup,
    pub cursor_status: Arc<Mutex<CursorImageStatus>>,
    pub seat_name: String,
    pub seat: Seat<ScreenComposer<BackendData>>,
    pub clock: Clock<Monotonic>,
    pub pointer: PointerHandle<ScreenComposer<BackendData>>,

    #[cfg(feature = "xwayland")]
    pub xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    pub xdisplay: Option<u32>,

    #[cfg(feature = "debug")]
    pub renderdoc: Option<renderdoc::RenderDoc<renderdoc::V141>>,

    pub scene_element: SceneElement,

    // layers
    pub layers_engine: Arc<Engine>,

    pub show_desktop: bool,
    pub is_expose_swiping: bool,
    pub is_workspace_swiping: bool,
    pub workspace_swipe_accumulated: (f64, f64),
    pub workspace_swipe_active: bool,
    pub workspace_swipe_velocity_samples: Vec<f64>,
    pub is_pinching: bool,
    pub is_resizing: bool,

    // screenshare
    pub screenshare_sessions: HashMap<String, crate::screenshare::ScreencastSession>,
    /// Manager for the screenshare D-Bus service (started lazily when needed).
    pub screenshare_manager: Option<crate::screenshare::ScreenshareManager>,
}

pub mod data_device_handler;
pub mod dnd_grab_handler;
pub mod fractional_scale_handler;
pub mod input_method_handler;
pub mod seat_handler;
pub mod security_context_handler;
pub mod selection_handler;
pub mod xdg_activation_handler;
pub mod xdg_decoration_handler;
pub mod xwayland_handler;

impl<BackendData: Backend> OutputHandler for ScreenComposer<BackendData> {}

impl<BackendData: Backend> ShmHandler for ScreenComposer<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl<BackendData: Backend> KeyboardShortcutsInhibitHandler for ScreenComposer<BackendData> {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.keyboard_shortcuts_inhibit_state
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        // Just grant the wish for everyone
        inhibitor.activate();
    }
}

impl<BackendData: Backend> XdgForeignHandler for ScreenComposer<BackendData> {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign_state
    }
}

delegate_compositor!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_output!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_shm!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_text_input_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_keyboard_shortcuts_inhibit!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_virtual_keyboard_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_pointer_gestures!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_relative_pointer!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_viewporter!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_xdg_shell!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_layer_shell!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_presentation!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_xdg_foreign!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend + 'static> ScreenComposer<BackendData> {
    pub fn init(
        display: Display<ScreenComposer<BackendData>>,
        handle: LoopHandle<'static, ScreenComposer<BackendData>>,
        backend_data: BackendData,
        listen_on_socket: bool,
    ) -> ScreenComposer<BackendData> {
        let dh = display.handle();

        let clock = Clock::new();

        // init wayland clients
        let socket_name = if listen_on_socket {
            let source = ListeningSocketSource::new_auto().unwrap();
            let socket_name = source.socket_name().to_string_lossy().into_owned();
            handle
                .insert_source(source, |client_stream, _, data| {
                    if let Ok(_client) = data
                        .display_handle
                        .insert_client(client_stream, Arc::new(ClientState::default()))
                    {
                        // warn!("Error adding wayland client: {}", err);
                    };
                })
                .expect("Failed to init wayland socket source");
            info!(name = socket_name, "Listening on wayland socket");
            Some(socket_name)
        } else {
            None
        };
        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, data| {
                    profiling::scope!("dispatch_clients");
                    // Safety: we don't drop the display
                    unsafe {
                        display.get_mut().dispatch_clients(data).unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .expect("Failed to init wayland server source");

        let (loop_wakeup_sender, loop_wakeup_channel) = channel::<()>();
        let loop_wakeup_pending = Arc::new(AtomicBool::new(false));
        let pending_flag = loop_wakeup_pending.clone();
        handle
            .insert_source(loop_wakeup_channel, move |event, _, _| {
                if matches!(event, ChannelEvent::Msg(_) | ChannelEvent::Closed) {
                    pending_flag.store(false, Ordering::Release);
                }
            })
            .expect("Failed to insert loop wake channel");

        // init globals
        let compositor_state = CompositorState::new::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        let data_control_state =
            DataControlState::new::<Self, _>(&dh, Some(&primary_selection_state), |_| true);
        let mut seat_state = SeatState::new();
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        let xdg_activation_state = XdgActivationState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let presentation_state = PresentationState::new::<Self>(&dh, clock.id() as u32);
        let fractional_scale_manager_state = FractionalScaleManagerState::new::<Self>(&dh);
        TextInputManagerState::new::<Self>(&dh);
        InputMethodManagerState::new::<Self, _>(&dh, |_client| true);
        VirtualKeyboardManagerState::new::<Self, _>(&dh, |_client| true);
        // Expose global only if backend supports relative motion events
        if BackendData::HAS_RELATIVE_MOTION {
            RelativePointerManagerState::new::<Self>(&dh);
        }
        PointerConstraintsState::new::<Self>(&dh);
        if BackendData::HAS_GESTURES {
            PointerGesturesState::new::<Self>(&dh);
        }
        TabletManagerState::new::<Self>(&dh);
        SecurityContextState::new::<Self, _>(&dh, |client| {
            client
                .get_data::<ClientState>()
                .is_none_or(|client_state| client_state.security_context.is_none())
        });
        let xdg_foreign_state = XdgForeignState::new::<Self>(&dh);

        // init input
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());

        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::default_named()));
        let pointer = seat.add_pointer();
        let k = Config::with(|c| (c.keyboard_repeat_delay, c.keyboard_repeat_rate));
        seat.add_keyboard(XkbConfig::default(), k.0, k.1)
            .expect("Failed to initialize the keyboard");

        let keyboard_shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<Self>(&dh);

        #[cfg(feature = "xwayland")]
        let xwayland_shell_state = xwayland_shell::XWaylandShellState::new::<Self>(&dh.clone());

        #[cfg(feature = "xwayland")]
        XWaylandKeyboardGrabState::new::<Self>(&dh.clone());

        let layers_engine = Engine::create(500.0, 500.0);
        let root_layer = layers_engine.new_layer();
        root_layer.set_key("screen_composer_root");
        root_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layers_engine.add_layer(&root_layer);
        let scene_element = SceneElement::with_engine(layers_engine.clone());
        let workspaces = Workspaces::new(layers_engine.clone());

        #[cfg(feature = "debugger")]
        layers_engine.start_debugger();

        let mut composer = ScreenComposer {
            backend_data,
            display_handle: dh,
            socket_name,
            running: Arc::new(AtomicBool::new(true)),
            handle,
            loop_wakeup_sender,
            loop_wakeup_pending,

            popups: PopupManager::default(),
            popup_root_cache: HashMap::new(),
            layer_surfaces: HashMap::new(),
            compositor_state,
            data_device_state,
            layer_shell_state,
            output_manager_state,
            primary_selection_state,
            data_control_state,
            seat_state,
            keyboard_shortcuts_inhibit_state,
            shm_state,
            viewporter_state,
            xdg_activation_state,
            xdg_decoration_state,
            xdg_shell_state,
            presentation_state,
            fractional_scale_manager_state,
            xdg_foreign_state,
            dnd_icon: None,
            suppressed_keys: Vec::new(),
            keycode_remap: HashMap::new(),
            current_modifiers: ModifiersState::default(),
            app_switcher_hold_modifiers: None,
            modifier_masks: ModifierMaskLookup::default(),
            cursor_status,
            seat_name,
            seat,
            pointer,
            clock,
            #[cfg(feature = "xwayland")]
            xwayland_shell_state,
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xdisplay: None,
            #[cfg(feature = "debug")]
            renderdoc: renderdoc::RenderDoc::new().ok(),

            workspaces,
            layers_engine,
            scene_element,

            show_desktop: false,
            // support variables for gestures
            is_expose_swiping: false,
            is_workspace_swiping: false,
            workspace_swipe_accumulated: (0.0, 0.0),
            workspace_swipe_active: false,
            workspace_swipe_velocity_samples: Vec::new(),
            is_pinching: false,
            is_resizing: false,

            // screenshare
            screenshare_sessions: HashMap::new(),
            screenshare_manager: None,
        };

        composer.rebuild_keycode_remap();
        composer.rebuild_modifier_masks();

        composer
    }

    pub fn schedule_event_loop_dispatch(&self) {
        if !self.loop_wakeup_pending.swap(true, Ordering::AcqRel)
            && self.loop_wakeup_sender.send(()).is_err()
        {
            self.loop_wakeup_pending.store(false, Ordering::Release);
        }
    }

    fn rebuild_keycode_remap(&mut self) {
        self.keycode_remap.clear();

        let remaps = Config::with(|config| config.parsed_key_remaps());
        if remaps.is_empty() {
            return;
        }

        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };

        let mapping = keyboard.with_xkb_state(self, |ctx| {
            let xkb = ctx.xkb().lock().unwrap();
            let keymap = unsafe { xkb.keymap() };
            build_keycode_remap_map(keymap, &remaps)
        });

        self.keycode_remap = mapping;
    }

    fn rebuild_modifier_masks(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            self.modifier_masks = ModifierMaskLookup::default();
            return;
        };

        let masks = keyboard.with_xkb_state(self, |ctx| {
            let xkb = ctx.xkb().lock().unwrap();
            let keymap = unsafe { xkb.keymap() };
            ModifierMaskLookup::from_keymap(keymap)
        });
        self.modifier_masks = masks;
    }

    #[cfg(feature = "xwayland")]
    pub fn start_xwayland(&mut self) {
        use std::process::Stdio;

        let (xwayland, client) = XWayland::spawn(
            &self.display_handle,
            None,
            std::iter::empty::<(String, String)>(),
            true,
            Stdio::null(),
            Stdio::null(),
            |_| (),
        )
        .expect("failed to start XWayland");

        let ret = self
            .handle
            .insert_source(xwayland, move |event, _, data| match event {
                XWaylandEvent::Ready {
                    x11_socket,
                    display_number,
                } => {
                    let mut wm = X11Wm::start_wm(data.handle.clone(), x11_socket, client.clone())
                        .expect("Failed to attach X11 Window Manager");

                    let cursor = Cursor::load();
                    let image = cursor.get_image(1, Duration::ZERO);
                    wm.set_cursor(
                        &image.pixels_rgba,
                        Size::from((image.width as u16, image.height as u16)),
                        Point::from((image.xhot as u16, image.yhot as u16)),
                    )
                    .expect("Failed to set xwayland default cursor");
                    data.xwm = Some(wm);
                    data.xdisplay = Some(display_number);
                }
                XWaylandEvent::Error => {
                    warn!("XWayland crashed on startup");
                }
            });
        if let Err(e) = ret {
            tracing::error!(
                "Failed to insert the XWaylandSource into the event loop: {}",
                e
            );
        }
    }
    pub fn set_cursor(&mut self, image: &CursorImageStatus) {
        *self.cursor_status.lock().unwrap() = image.clone();
        self.backend_data.set_cursor(image);
    }

    pub fn load_cursor_for_action(
        &mut self,
        action: smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction,
    ) {
        let cursor = if action == DndAction::Copy {
            CursorImageStatus::Named(CursorIcon::Copy)
        } else if action == DndAction::Move {
            CursorImageStatus::Named(CursorIcon::Move)
        } else if action == DndAction::Ask {
            CursorImageStatus::Named(CursorIcon::Help)
        } else {
            CursorImageStatus::Hidden
        };
        self.set_cursor(&cursor);
    }

    pub fn get_cursor_position(&self) -> utils::Point<f64, utils::Physical> {
        let cursor_guard: std::sync::MutexGuard<CursorImageStatus> =
            self.cursor_status.lock().unwrap();
        let cursor_hotspot = if let CursorImageStatus::Surface(ref surface) = *cursor_guard {
            compositor::with_states(surface, |states| {
                states
                    .data_map
                    .get::<Mutex<CursorImageAttributes>>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .hotspot
            })
        } else {
            (0, 0).into()
        };

        let cursor_pos = self.pointer.current_location() - cursor_hotspot.to_f64();
        let output = self
            .workspaces
            .output_under(cursor_pos)
            .next()
            .cloned()
            .unwrap();
        let scale = output.current_scale().fractional_scale();

        cursor_pos.to_physical(scale).to_f64()
    }

    pub fn get_render_elements(
        &self,
        surface: &WlSurface,
        scale_factor: f64,
    ) -> VecDeque<WindowViewSurface> {
        let initial_location: smithay::utils::Point<f64, smithay::utils::Physical> =
            (0.0, 0.0).into();
        let mut render_elements = VecDeque::new();

        smithay::wayland::compositor::with_surface_tree_downward(
            surface,
            initial_location,
            |_, states, location| {
                let mut location = *location;
                let data = states.data_map.get::<RendererSurfaceStateUserData>();
                let mut cached_state = states.cached_state.get::<SurfaceCachedState>();
                let cached_state = cached_state.current();
                let surface_geometry = cached_state.geometry.unwrap_or_default();

                if let Some(data) = data {
                    let data = data.lock().unwrap();

                    if let Some(view) = data.view() {
                        location += view.offset.to_f64().to_physical(scale_factor);
                        location -= surface_geometry.loc.to_f64().to_physical(scale_factor);
                        TraversalAction::DoChildren(location)
                    } else {
                        TraversalAction::SkipChildren
                    }
                } else {
                    TraversalAction::SkipChildren
                }
            },
            |surface, states, location| {
                if let Some(window_view) =
                    self.window_view_for_surface(surface, states, location, scale_factor)
                {
                    render_elements.push_front(window_view);
                }
            },
            |_, _, _| true,
        );
        render_elements
    }

    pub fn update_dnd(&mut self) {
        if let Some(dnd_surface) = self.dnd_icon.as_ref() {
            profiling::scope!("update_dnd_icon");
            let cursor_position = self.get_cursor_position();

            let scale = Config::with(|c| c.screen_scale);
            let render_elements = self.get_render_elements(dnd_surface, scale);
            self.workspaces
                .dnd_view
                .view_content
                .update_state(&render_elements.into());

            self.workspaces
                .dnd_view
                .layer
                .set_position((cursor_position.x as f32, cursor_position.y as f32), None);
        }
    }

    #[profiling::function]
    pub fn window_view_for_surface(
        &self,
        surface: &WlSurface,
        states: &SurfaceData,
        location: &smithay::utils::Point<f64, smithay::utils::Physical>,
        scale: f64,
    ) -> Option<WindowViewSurface> {
        let id = surface.id();
        let mut cached_state = states.cached_state.get::<SurfaceCachedState>();
        let cached_state = cached_state.current();
        let surface_geometry = cached_state
            .geometry
            .unwrap_or_default()
            .to_f64()
            .to_physical(scale);
        let mut surface_attributes = states.cached_state.get::<SurfaceAttributes>();
        let surface_attributes = surface_attributes.current();
        if let Some(render_surface) = states.data_map.get::<RendererSurfaceStateUserData>() {
            let render_surface: std::sync::MutexGuard<RendererSurfaceState> =
                render_surface.lock().unwrap();

            if let Some(view) = render_surface.view() {
                let mut texture_id = None;
                if let Some(t) = self.backend_data.texture_for_surface(&render_surface) {
                    // Store for debug comparison rendering (unique per surface id)
                    texture_id = Some(t.tid);
                    crate::textures_storage::set(&id, t);
                }
                let wvs = WindowViewSurface {
                    id: id.clone(),
                    log_offset_x: location.x as f32,
                    log_offset_y: location.y as f32,

                    phy_src_x: view.src.loc.x as f32 * surface_attributes.buffer_scale as f32,
                    phy_src_y: view.src.loc.y as f32 * surface_attributes.buffer_scale as f32,
                    phy_src_w: view.src.size.w as f32 * surface_attributes.buffer_scale as f32,
                    phy_src_h: view.src.size.h as f32 * surface_attributes.buffer_scale as f32,

                    phy_dst_x: view.offset.x as f32 * scale as f32 - surface_geometry.loc.x as f32,
                    phy_dst_y: view.offset.y as f32 * scale as f32 - surface_geometry.loc.y as f32,
                    phy_dst_w: view.dst.w as f32 * scale as f32,
                    phy_dst_h: view.dst.h as f32 * scale as f32,
                    texture_id,
                    commit: render_surface.current_commit(),
                    transform: surface_attributes.buffer_transform.into(),
                };
                return Some(wvs);
            }
        };
        None
    }

    pub fn update_window_view(&mut self, window: &WindowElement) {
        let scale_factor = Config::with(|c| c.screen_scale);
        if let Some(window_surface) = window.wl_surface() {
            let id = window_surface.id();
            let location = self
                .workspaces
                .element_location(window)
                .unwrap_or((0, 0).into())
                .to_f64()
                .to_physical(scale_factor);
            let window_geometry = self
                .workspaces
                .element_geometry(window)
                .unwrap_or_default()
                .to_f64()
                .to_physical(scale_factor);
            let title = window.xdg_title();
            let fullscreen = window.xdg_is_fullscreen();

            let mut render_elements = VecDeque::new();

            // Collect popup surfaces and send them to the popup overlay layer
            PopupManager::popups_for_surface(&window_surface).for_each(|(popup, popup_offset)| {
                let offset: smithay::utils::Point<f64, smithay::utils::Physical> =
                    popup_offset.to_physical_precise_round(scale_factor);
                let popup_surface = popup.wl_surface();
                let popup_id = popup_surface.id();

                // Calculate absolute popup position (window position + popup offset)
                let popup_position = lay_rs::types::Point {
                    x: location.x as f32 + offset.x as f32,
                    y: location.y as f32 + offset.y as f32,
                };

                // Collect surfaces for this popup
                let mut popup_surfaces = Vec::new();
                let popup_origin: smithay::utils::Point<f64, smithay::utils::Physical> =
                    (0.0, 0.0).into();
                with_surfaces_surface_tree(popup_surface, |surface, states| {
                    if let Some(window_view) =
                        self.window_view_for_surface(surface, states, &popup_origin, scale_factor)
                    {
                        popup_surfaces.push(window_view);
                    }
                });

                // Send popup to the overlay layer
                self.workspaces.popup_overlay.update_popup(
                    &popup_id,
                    &id,
                    popup_position,
                    popup_surfaces,
                );
            });

            let initial_location: smithay::utils::Point<f64, smithay::utils::Physical> =
                (0.0, 0.0).into();

            smithay::wayland::compositor::with_surface_tree_downward(
                &window_surface,
                initial_location,
                |_, states, location| {
                    profiling::scope!("surface_tree_downward");
                    let mut location = *location;
                    let data = states.data_map.get::<RendererSurfaceStateUserData>();
                    let mut cached_state = states.cached_state.get::<SurfaceCachedState>();
                    let cached_state = cached_state.current();
                    let surface_geometry = cached_state.geometry.unwrap_or_default();

                    if let Some(data) = data {
                        let data = data.lock().unwrap();

                        if let Some(view) = data.view() {
                            location += view.offset.to_f64().to_physical(scale_factor);
                            location -= surface_geometry.loc.to_f64().to_physical(scale_factor);
                            TraversalAction::DoChildren(location)
                        } else {
                            TraversalAction::SkipChildren
                        }
                    } else {
                        TraversalAction::SkipChildren
                    }
                },
                |surface, states, location| {
                    if let Some(window_view) =
                        self.window_view_for_surface(surface, states, location, scale_factor)
                    {
                        render_elements.push_front(window_view);
                    }
                },
                |_, _, _| true,
            );

            if let Some(window_view) = self.workspaces.get_window_view(&id) {
                let model = WindowViewBaseModel {
                    x: location.x as f32,
                    y: location.y as f32,
                    w: window_geometry.size.w as f32,
                    h: window_geometry.size.h as f32,
                    title,
                    fullscreen,
                    // active: window.toplevel().unwrap().with_pending_state(|state| {
                    //     state.states.contains(xdg_toplevel::State::Activated)
                    // }),
                    // TODO: find a way to get the active state
                    active: false,
                };
                window_view.view_base.update_state(&model);
                window_view
                    .view_content
                    .update_state(&render_elements.into());

                self.workspaces.expose_update_if_needed();
            }
        }
    }

    /// Update a layer shell surface's lay_rs layer with current buffer content
    pub fn update_layer_surface(&mut self, surface_id: &ObjectId) {
        let Some(layer_shell_surface) = self.layer_surfaces.get(surface_id) else {
            return;
        };

        let scale_factor = Config::with(|c| c.screen_scale);
        let wl_surface = layer_shell_surface.layer_surface().wl_surface();

        // Get the output geometry to compute surface placement
        let output_geometry = self
            .workspaces
            .output_geometry(layer_shell_surface.output())
            .unwrap_or_default();

        // Compute the layer surface geometry based on anchors/margins
        let geometry = layer_shell_surface.compute_geometry(output_geometry);

        // Collect render elements from the surface tree
        let mut render_elements: Vec<WindowViewSurface> = Vec::new();
        let initial_location: smithay::utils::Point<f64, smithay::utils::Physical> =
            (0.0, 0.0).into();

        smithay::wayland::compositor::with_surface_tree_downward(
            wl_surface,
            initial_location,
            |_, states, location| {
                let mut location = *location;
                let data = states
                    .data_map
                    .get::<smithay::backend::renderer::utils::RendererSurfaceStateUserData>(
                );
                let mut cached_state = states.cached_state.get::<SurfaceCachedState>();
                let cached_state = cached_state.current();
                let surface_geometry = cached_state.geometry.unwrap_or_default();

                if let Some(data) = data {
                    let data = data.lock().unwrap();
                    if let Some(view) = data.view() {
                        location += view.offset.to_f64().to_physical(scale_factor);
                        location -= surface_geometry.loc.to_f64().to_physical(scale_factor);
                        TraversalAction::DoChildren(location)
                    } else {
                        TraversalAction::SkipChildren
                    }
                } else {
                    TraversalAction::SkipChildren
                }
            },
            |surface, states, location| {
                if let Some(wvs) =
                    self.window_view_for_surface(surface, states, location, scale_factor)
                {
                    render_elements.push(wvs);
                }
            },
            |_, _, _| true,
        );

        // Update the lay_rs layer position and size
        let layer = &layer_shell_surface.layer;
        layer.set_position(
            lay_rs::types::Point {
                x: (geometry.loc.x as f64 * scale_factor) as f32,
                y: (geometry.loc.y as f64 * scale_factor) as f32,
            },
            None,
        );
        layer.set_size(
            lay_rs::types::Size::points(
                (geometry.size.w as f64 * scale_factor) as f32,
                (geometry.size.h as f64 * scale_factor) as f32,
            ),
            None,
        );

        // If we have render elements, set up the drawing
        if !render_elements.is_empty() {
            // Clone what we need for the draw closure
            let elements = render_elements.clone();
            let width = (geometry.size.w as f64 * scale_factor) as f32;
            let height = (geometry.size.h as f64 * scale_factor) as f32;

            layer.set_draw_content(move |canvas: &lay_rs::skia::Canvas, _w, _h| {
                for wvs in &elements {
                    if wvs.phy_dst_w <= 0.0 || wvs.phy_dst_h <= 0.0 {
                        continue;
                    }
                    let tex = crate::textures_storage::get(&wvs.id);
                    if let Some(tex) = tex {
                        let src_h = (wvs.phy_src_h - wvs.phy_src_y).max(1.0);
                        let src_w = (wvs.phy_src_w - wvs.phy_src_x).max(1.0);
                        let scale_y = wvs.phy_dst_h / src_h;
                        let scale_x = wvs.phy_dst_w / src_w;
                        let mut matrix = lay_rs::skia::Matrix::new_identity();
                        matrix.pre_translate((-wvs.phy_src_x, -wvs.phy_src_y));
                        matrix.pre_scale((scale_x, scale_y), None);

                        let sampling = lay_rs::skia::SamplingOptions::from(
                            lay_rs::skia::CubicResampler::catmull_rom(),
                        );
                        let mut paint = lay_rs::skia::Paint::new(
                            lay_rs::skia::Color4f::new(1.0, 1.0, 1.0, 1.0),
                            None,
                        );
                        paint.set_shader(tex.image.to_shader(
                            (lay_rs::skia::TileMode::Clamp, lay_rs::skia::TileMode::Clamp),
                            sampling,
                            &matrix,
                        ));

                        let dst_rect = lay_rs::skia::Rect::from_xywh(
                            wvs.phy_dst_x,
                            wvs.phy_dst_y,
                            wvs.phy_dst_w,
                            wvs.phy_dst_h,
                        );
                        canvas.draw_rect(dst_rect, &paint);
                    }
                }
                lay_rs::skia::Rect::from_xywh(0.0, 0.0, width, height)
            });
        }
    }

    pub fn quit_appswitcher_app(&mut self) {
        self.workspaces.quit_appswitcher_app();
        // FIXME focus the previous window
    }
    pub fn toggle_maximize_focused_window(&mut self) {
        let Some(window) = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
            .and_then(|focus| match focus {
                KeyboardFocusTarget::Window(window) => Some(window),
                _ => None,
            })
        else {
            return;
        };

        match window.underlying_surface() {
            smithay::desktop::WindowSurface::Wayland(_) => {
                if let Some(toplevel) = window.toplevel() {
                    let toplevel = toplevel.clone();
                    let is_maximized = toplevel
                        .current_state()
                        .states
                        .contains(xdg_toplevel::State::Maximized);
                    if is_maximized {
                        <Self as smithay::wayland::shell::xdg::XdgShellHandler>::unmaximize_request(
                            self, toplevel,
                        );
                    } else {
                        <Self as smithay::wayland::shell::xdg::XdgShellHandler>::maximize_request(
                            self, toplevel,
                        );
                    }
                }
            }
            #[cfg(feature = "xwayland")]
            smithay::desktop::WindowSurface::X11(surface) => {
                if surface.is_maximized() {
                    self.unmaximize_request_x11(&surface);
                } else {
                    self.maximize_request_x11(&surface);
                }
            }
        }
    }
    pub fn close_focused_window(&mut self) {
        if let Some(keyboard) = self.seat.get_keyboard() {
            if let Some(KeyboardFocusTarget::Window(window)) = keyboard.current_focus() {
                match window.underlying_surface() {
                    smithay::desktop::WindowSurface::Wayland(toplevel) => toplevel.send_close(),
                    #[cfg(feature = "xwayland")]
                    smithay::desktop::WindowSurface::X11(surface) => {
                        let _ = surface.close();
                    }
                }
            }
        }
    }
    pub fn raise_next_app_window(&mut self) {
        if let Some(wid) = self.workspaces.raise_next_app_window() {
            self.set_keyboard_focus_on_surface(&wid);
        }
    }
    pub fn focus_app(&mut self, app_id: &str) {
        if let Some(wid) = self.workspaces.focus_app(app_id) {
            self.set_keyboard_focus_on_surface(&wid);
        }
    }
    pub fn set_current_workspace_index(&mut self, index: usize) {
        self.workspaces.set_current_workspace_index(index, None);
        // Focus the top window of the new workspace, or clear focus if empty
        if let Some(top_wid) = self.workspaces.get_top_window_of_workspace(index) {
            self.set_keyboard_focus_on_surface(&top_wid);
        } else {
            self.clear_keyboard_focus();
        }
    }

    pub fn set_keyboard_focus_on_surface(&mut self, wid: &ObjectId) {
        if let Some(window) = self.workspaces.get_window_for_surface(wid) {
            let keyboard = self.seat.get_keyboard().unwrap();
            let serial = SERIAL_COUNTER.next_serial();
            keyboard.set_focus(self, Some(window.clone().into()), serial);
        }
    }

    pub fn clear_keyboard_focus(&mut self) {
        if let Some(keyboard) = self.seat.get_keyboard() {
            let serial = SERIAL_COUNTER.next_serial();
            keyboard.set_focus(self, None, serial);
        }
    }

    /// Dismiss all active popups and release any pointer/keyboard grabs
    pub fn dismiss_all_popups(&mut self) {
        let serial = SERIAL_COUNTER.next_serial();

        // Unset pointer grab if active
        if let Some(pointer) = self.seat.get_pointer() {
            if pointer.is_grabbed() {
                pointer.unset_grab(self, serial, 0);
            }
        }

        // Unset keyboard grab if active
        if let Some(keyboard) = self.seat.get_keyboard() {
            if keyboard.is_grabbed() {
                keyboard.unset_grab(self);
            }
        }
    }
}

fn build_keycode_remap_map(keymap: &xkb::Keymap, remaps: &[(Keysym, Keysym)]) -> HashMap<u32, u32> {
    let mut result = HashMap::new();

    for &(from_sym, to_sym) in remaps {
        let from_code = find_keycode_for_keysym(keymap, from_sym);
        let to_code = find_keycode_for_keysym(keymap, to_sym);

        match (from_code, to_code) {
            (Some(src), Some(dst)) => {
                if src != dst {
                    result.insert(src, dst);
                }
            }
            (None, _) => warn!(
                source = xkb::keysym_get_name(from_sym),
                "no keycode found for source keysym"
            ),
            (_, None) => warn!(
                target = xkb::keysym_get_name(to_sym),
                "no keycode found for target keysym"
            ),
        }
    }

    result
}

fn find_keycode_for_keysym(keymap: &xkb::Keymap, target: Keysym) -> Option<u32> {
    let mut result = None;

    keymap.key_for_each(|km, keycode| {
        if result.is_some() {
            return;
        }

        let layout_count = km.num_layouts_for_key(keycode);
        for layout in 0..layout_count {
            let syms = km.key_get_syms_by_level(keycode, layout, 0);
            if syms.iter().any(|sym| *sym == target) {
                let raw = keycode.raw();
                result = Some(raw.saturating_sub(8));
                break;
            }
        }
    });

    result
}

#[derive(Debug, Copy, Clone)]
pub struct SurfaceDmabufFeedback<'a> {
    pub render_feedback: &'a DmabufFeedback,
    pub scanout_feedback: &'a DmabufFeedback,
}

#[profiling::function]
pub fn post_repaint<'a>(
    output: &Output,
    render_element_states: &RenderElementStates,
    window_elements: &[&WindowElement],
    dmabuf_feedback: Option<SurfaceDmabufFeedback<'_>>,
    time: impl Into<Duration>,
) {
    let time = time.into();
    let throttle = Some(Duration::from_secs(1));

    window_elements.iter().for_each(|window| {
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
        window.send_frame(output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
        // Send frame to all windows since we're processing all workspaces
        window.send_frame(output, time, throttle, surface_primary_scanout_output);
        if let Some(dmabuf_feedback) = dmabuf_feedback {
            window.send_dmabuf_feedback(output, surface_primary_scanout_output, |surface, _| {
                select_dmabuf_feedback(
                    surface,
                    render_element_states,
                    dmabuf_feedback.render_feedback,
                    dmabuf_feedback.scanout_feedback,
                )
            });
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

#[profiling::function]
pub fn take_presentation_feedback<'a>(
    output: &Output,
    window_elements: &[&WindowElement],
    render_element_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut output_presentation_feedback = OutputPresentationFeedback::new(output);

    window_elements.iter().for_each(|window| {
        // Process all windows since we're handling all workspaces
        window.take_presentation_feedback(
            &mut output_presentation_feedback,
            surface_primary_scanout_output,
            |surface, _| {
                surface_presentation_feedback_flags_from_states(surface, render_element_states)
            },
        );
    });

    // space.elements().for_each(|window| {
    //     if space.outputs_for_element(window).contains(output) {
    //         window.take_presentation_feedback(
    //             &mut output_presentation_feedback,
    //             surface_primary_scanout_output,
    //             |surface, _| {
    //                 surface_presentation_feedback_flags_from_states(surface, render_element_states)
    //             },
    //         );
    //     }
    // });
    // TODO layers presentation feedback
    // let map = smithay::desktop::layer_map_for_output(output);
    // for layer_surface in map.layers() {
    //     layer_surface.take_presentation_feedback(
    //         &mut output_presentation_feedback,
    //         surface_primary_scanout_output,
    //         |surface, _| {
    //             surface_presentation_feedback_flags_from_states(surface, render_element_states)
    //         },
    //     );
    // }

    output_presentation_feedback
}

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
    fn texture_for_surface(&self, surface: &RendererSurfaceState) -> Option<SkiaTextureImage>;
    fn set_cursor(&mut self, image: &CursorImageStatus); //, renderer: &mut SkiaRenderer);
    fn renderer_context(&mut self) -> Option<lay_rs::skia::gpu::DirectContext>;
    fn request_redraw(&mut self) {}
    /// Get GBM device for DMA-BUF screenshare (None for backends without DMA-BUF support)
    fn gbm_device(
        &self,
    ) -> Option<smithay::backend::allocator::gbm::GbmDevice<smithay::backend::drm::DrmDeviceFd>>
    {
        None
    }
    /// Get render format and modifier for screenshare.
    /// Returns (fourcc, modifier) tuple, or None if not available.
    fn render_format(&mut self) -> Option<(u32, u64)> {
        None
    }
    /// Get all supported modifiers for a given format from the backend.
    /// Used for DMA-BUF format negotiation.
    fn get_format_modifiers(&mut self, _fourcc: smithay::backend::allocator::Fourcc) -> Vec<u64> {
        vec![]
    }
    /// Whether this backend prefers DMA-BUF for screenshare (zero-copy)
    fn prefers_dmabuf_screenshare(&self) -> bool {
        false
    }
}
