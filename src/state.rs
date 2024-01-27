use std::{
    os::unix::io::OwnedFd,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration, collections::{HashMap, VecDeque},
};

use layers::{engine::{LayersEngine, animation::{Transition, timing::{Easing, TimingFunction}}}, prelude::{Layer, taffy, Interpolate}, types::{Color, BorderRadius}};
use tracing::{info, warn};

use smithay::{
    backend::renderer::{element::{
        default_primary_scanout_output_compare, utils::select_dmabuf_feedback, RenderElementStates,
    }, utils::{RendererSurfaceStateUserData, RendererSurfaceState}},
    delegate_compositor, delegate_data_control, delegate_data_device, delegate_fractional_scale,
    delegate_input_method_manager, delegate_keyboard_shortcuts_inhibit, delegate_layer_shell,
    delegate_output, delegate_pointer_constraints, delegate_pointer_gestures, delegate_presentation,
    delegate_primary_selection, delegate_relative_pointer, delegate_seat, delegate_security_context,
    delegate_shm, delegate_tablet_manager, delegate_text_input_manager, delegate_viewporter,
    delegate_virtual_keyboard_manager, delegate_xdg_activation, delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{
        space::SpaceElement,
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output, update_surface_primary_scanout_output, with_surfaces_surface_tree, OutputPresentationFeedback
        },
        PopupKind, PopupManager, Space,
    },
    input::{
        keyboard::{Keysym, XkbConfig},
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatHandler, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{generic::Generic, Interest, LoopHandle, Mode, PostAction},
        wayland_protocols::xdg::decoration::{
            self as xdg_decoration, 
            zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
        },
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason, ObjectId},
            protocol::{wl_data_source::WlDataSource, wl_surface::WlSurface},
            Display, DisplayHandle, Resource,
        },
    },
    utils::{Clock, Monotonic, Rectangle},
    wayland::{
        compositor::{get_parent, with_states, CompositorClientState, CompositorState, SurfaceAttributes, SurfaceData, TraversalAction},
        dmabuf::DmabufFeedback,
        fractional_scale::{with_fractional_scale, FractionalScaleHandler, FractionalScaleManagerState},
        input_method::{InputMethodHandler, InputMethodManagerState, PopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
        },
        output::OutputManagerState,
        pointer_constraints::{with_pointer_constraint, PointerConstraintsHandler, PointerConstraintsState},
        pointer_gestures::PointerGesturesState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        seat::WaylandFocus,
        security_context::{
            SecurityContext, SecurityContextHandler, SecurityContextListenerSource, SecurityContextState,
        },
        selection::data_device::{
            set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
            ServerDndGrabHandler,
        },
        selection::{
            primary_selection::{set_primary_focus, PrimarySelectionHandler, PrimarySelectionState},
            wlr_data_control::{DataControlHandler, DataControlState},
            SelectionHandler,
        },
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{
                decoration::{XdgDecorationHandler, XdgDecorationState},
                ToplevelSurface, XdgShellState, XdgToplevelSurfaceData, SurfaceCachedState,
            },
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
        tablet_manager::TabletSeatTrait,
        text_input::TextInputManagerState,
        viewporter::ViewporterState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_activation::{
            XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
        },
    },
};

#[cfg(feature = "xwayland")]
use crate::cursor::Cursor;
use crate::{app_switcher::AppSwitcherView, focus::FocusTarget, render_elements::scene_element::SceneElement, shell::WindowElement, utils::{bin_pack, image_from_path}, window_view::{WindowView, view::WindowViewSurface}, workspace_view::BackgroundView};
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
    pub handle: LoopHandle<'static, CalloopData<BackendData>>,

    // desktop
    pub space: Space<WindowElement>,
    pub popups: PopupManager,

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

    pub dnd_icon: Option<WlSurface>,

    // input-related fields
    pub suppressed_keys: Vec<Keysym>,
    pub cursor_status: Arc<Mutex<CursorImageStatus>>,
    pub seat_name: String,
    pub seat: Seat<ScreenComposer<BackendData>>,
    pub clock: Clock<Monotonic>,
    pub pointer: PointerHandle<ScreenComposer<BackendData>>,

    #[cfg(feature = "xwayland")]
    pub xwayland: XWayland,
    #[cfg(feature = "xwayland")]
    pub xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    pub xdisplay: Option<u32>,

    #[cfg(feature = "debug")]
    pub renderdoc: Option<renderdoc::RenderDoc<renderdoc::V141>>,

    pub show_window_preview: bool,

    pub layers_engine: LayersEngine,
    pub scene_element: SceneElement,
    pub app_switcher: AppSwitcherView,
    pub window_views: HashMap<ObjectId, WindowView>,
    pub show_all: bool,
    pub show_desktop: bool,
    pub workspace_layer: Layer,
    pub windows_layer: Layer,
    pub overlay_layer: Layer,
    pub background_view: BackgroundView,
    //
    pub is_swiping: bool,
    pub is_pinching: bool,
    pub swipe_gesture: layers::types::Point,
    pub pinch_gesture: layers::types::Point,
}

delegate_compositor!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> DataDeviceHandler for ScreenComposer<BackendData> {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl<BackendData: Backend> ClientDndGrabHandler for ScreenComposer<BackendData> {
    fn started(&mut self, _source: Option<WlDataSource>, icon: Option<WlSurface>, _seat: Seat<Self>) {
        self.dnd_icon = icon;
    }
    fn dropped(&mut self, _seat: Seat<Self>) {
        self.dnd_icon = None;
    }
}
impl<BackendData: Backend> ServerDndGrabHandler for ScreenComposer<BackendData> {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {
        unreachable!("Anvil doesn't do server-side grabs");
    }
}
delegate_data_device!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_output!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> SelectionHandler for ScreenComposer<BackendData> {
    type SelectionUserData = ();

    #[cfg(feature = "xwayland")]
    fn new_selection(&mut self, ty: SelectionTarget, source: Option<SelectionSource>, _seat: Seat<Self>) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(err) = xwm.new_selection(ty, source.map(|source| source.mime_types())) {
                warn!(?err, ?ty, "Failed to set Xwayland selection");
            }
        }
    }

    #[cfg(feature = "xwayland")]
    fn send_selection(
        &mut self,
        ty: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &(),
    ) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(err) = xwm.send_selection(ty, mime_type, fd, self.handle.clone()) {
                warn!(?err, "Failed to send primary (X11 -> Wayland)");
            }
        }
    }
}

impl<BackendData: Backend> PrimarySelectionHandler for ScreenComposer<BackendData> {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}
delegate_primary_selection!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> DataControlHandler for ScreenComposer<BackendData> {
    fn data_control_state(&self) -> &DataControlState {
        &self.data_control_state
    }
}

delegate_data_control!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> ShmHandler for ScreenComposer<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}
delegate_shm!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> SeatHandler for ScreenComposer<BackendData> {
    type KeyboardFocus = FocusTarget;
    type PointerFocus = FocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<ScreenComposer<BackendData>> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, target: Option<&FocusTarget>) {
        let dh = &self.display_handle;

        let wl_surface = target.and_then(WaylandFocus::wl_surface);

        let focus = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }
    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        *self.cursor_status.lock().unwrap() = image;
    }
}
delegate_seat!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_tablet_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_text_input_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> InputMethodHandler for ScreenComposer<BackendData> {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            warn!("Failed to track popup: {}", err);
        }
    }

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, smithay::utils::Logical> {
        self.space
            .elements()
            .find_map(|window| (window.wl_surface().as_ref() == Some(parent)).then(|| window.geometry()))
            .unwrap_or_default()
    }
}

delegate_input_method_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> KeyboardShortcutsInhibitHandler for ScreenComposer<BackendData> {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.keyboard_shortcuts_inhibit_state
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        // Just grant the wish for everyone
        inhibitor.activate();
    }
}

delegate_keyboard_shortcuts_inhibit!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_virtual_keyboard_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_pointer_gestures!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_relative_pointer!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> PointerConstraintsHandler for ScreenComposer<BackendData> {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        // XXX region
        if pointer.current_focus().and_then(|x| x.wl_surface()).as_ref() == Some(surface) {
            with_pointer_constraint(surface, pointer, |constraint| {
                constraint.unwrap().activate();
            });
        }
    }
}
delegate_pointer_constraints!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_viewporter!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> XdgActivationHandler for ScreenComposer<BackendData> {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        if let Some((serial, seat)) = data.serial {
            let keyboard = self.seat.get_keyboard().unwrap();
            Seat::from_resource(&seat) == Some(self.seat.clone())
                && keyboard
                    .last_enter()
                    .map(|last_enter| serial.is_no_older_than(&last_enter))
                    .unwrap_or(false)
        } else {
            false
        }
    }

    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if token_data.timestamp.elapsed().as_secs() < 10 {
            // Just grant the wish
            let w = self
                .space
                .elements()
                .find(|window| window.wl_surface().map(|s| s == surface).unwrap_or(false))
                .cloned();
            if let Some(window) = w {
                self.space.raise_element(&window, true);
            }
        }
    }
}
delegate_xdg_activation!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> XdgDecorationHandler for ScreenComposer<BackendData> {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        // Set the default to client side
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;

        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(match mode {
                DecorationMode::ServerSide => Mode::ServerSide,
                _ => Mode::ClientSide,
            });
        });

        let initial_configure_sent = with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });
        if initial_configure_sent {
            toplevel.send_pending_configure();
        }
    }
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
        let initial_configure_sent = with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });
        if initial_configure_sent {
            toplevel.send_pending_configure();
        }
    }
}
delegate_xdg_decoration!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_xdg_shell!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_layer_shell!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_presentation!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> FractionalScaleHandler for ScreenComposer<BackendData> {
    fn new_fractional_scale(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        // Here we can set the initial fractional scale
        //
        // First we look if the surface already has a primary scan-out output, if not
        // we test if the surface is a subsurface and try to use the primary scan-out output
        // of the root surface. If the root also has no primary scan-out output we just try
        // to use the first output of the toplevel.
        // If the surface is the root we also try to use the first output of the toplevel.
        //
        // If all the above tests do not lead to a output we just use the first output
        // of the space (which in case of anvil will also be the output a toplevel will
        // initially be placed on)
        #[allow(clippy::redundant_clone)]
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }

        with_states(&surface, |states| {
            let primary_scanout_output = surface_primary_scanout_output(&surface, states)
                .or_else(|| {
                    if root != surface {
                        with_states(&root, |states| {
                            surface_primary_scanout_output(&root, states).or_else(|| {
                                self.window_for_surface(&root).and_then(|window| {
                                    self.space.outputs_for_element(&window).first().cloned()
                                })
                            })
                        })
                    } else {
                        self.window_for_surface(&root)
                            .and_then(|window| self.space.outputs_for_element(&window).first().cloned())
                    }
                })
                .or_else(|| self.space.outputs().next().cloned());
            if let Some(output) = primary_scanout_output {
                with_fractional_scale(states, |fractional_scale| {
                    fractional_scale.set_preferred_scale(output.current_scale().fractional_scale());
                });
            }
        });
    }
}
delegate_fractional_scale!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend + 'static> SecurityContextHandler for ScreenComposer<BackendData> {
    fn context_created(&mut self, source: SecurityContextListenerSource, security_context: SecurityContext) {
        self.handle
            .insert_source(source, move |client_stream, _, data| {
                let client_state = ClientState {
                    security_context: Some(security_context.clone()),
                    ..ClientState::default()
                };
                if let Err(err) = data
                    .display_handle
                    .insert_client(client_stream, Arc::new(client_state))
                {
                    warn!("Error adding wayland client: {}", err);
                };
            })
            .expect("Failed to init wayland socket source");
    }
}
delegate_security_context!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

#[cfg(feature = "xwayland")]
impl<BackendData: Backend + 'static> XWaylandKeyboardGrabHandler for ScreenComposer<BackendData> {
    fn keyboard_focus_for_xsurface(&self, surface: &WlSurface) -> Option<FocusTarget> {
        let elem = self
            .space
            .elements()
            .find(|elem| elem.wl_surface().as_ref() == Some(surface))?;
        Some(FocusTarget::Window(elem.clone()))
    }
}
#[cfg(feature = "xwayland")]
delegate_xwayland_keyboard_grab!(@<BackendData: Backend + 'static> AnvilState<BackendData>);

impl<BackendData: Backend + 'static> ScreenComposer<BackendData> {
    pub fn init(
        display: Display<ScreenComposer<BackendData>>,
        handle: LoopHandle<'static, CalloopData<BackendData>>,
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
                    if let Err(err) = data
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
        handle
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
        SecurityContextState::new::<Self, _>(&dh, |client| {
            client
                .get_data::<ClientState>()
                .map_or(true, |client_state| client_state.security_context.is_none())
        });

        // init input
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());

        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::default_named()));
        let pointer = seat.add_pointer();
        seat.add_keyboard(XkbConfig::default(), 200, 25)
            .expect("Failed to initialize the keyboard");

        let cursor_status2 = cursor_status.clone();
        seat.tablet_seat().on_cursor_surface(move |_tool, new_status| {
            // TODO: tablet tools should have their own cursors
            *cursor_status2.lock().unwrap() = new_status;
        });

        let keyboard_shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<Self>(&dh);

        #[cfg(feature = "xwayland")]
        let xwayland = {
            XWaylandKeyboardGrabState::new::<Self>(&dh);

            let (xwayland, channel) = XWayland::new(&dh);
            let dh = dh.clone();
            let ret = handle.insert_source(channel, move |event, _, data| match event {
                XWaylandEvent::Ready {
                    connection,
                    client,
                    client_fd: _,
                    display,
                } => {
                    let mut wm = X11Wm::start_wm(data.state.handle.clone(), dh.clone(), connection, client)
                        .expect("Failed to attach X11 Window Manager");
                    let cursor = Cursor::load();
                    let image = cursor.get_image(1, Duration::ZERO);
                    wm.set_cursor(
                        &image.pixels_rgba,
                        Size::from((image.width as u16, image.height as u16)),
                        Point::from((image.xhot as u16, image.yhot as u16)),
                    )
                    .expect("Failed to set xwayland default cursor");
                    data.state.xwm = Some(wm);
                    data.state.xdisplay = Some(display);
                }
                XWaylandEvent::Exited => {
                    let _ = data.state.xwm.take();
                }
            });
            if let Err(e) = ret {
                tracing::error!("Failed to insert the XWaylandSource into the event loop: {}", e);
            }
            xwayland
        };
        let layers_engine = LayersEngine::new(500.0, 500.0);
        let root_layer = layers_engine.new_layer();
        root_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        let workspace_layer = layers_engine.new_layer();
        workspace_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        workspace_layer.set_size(layers::types::Size::percent(1.0, 1.0), None);

        let background_layer = layers_engine.new_layer();
        background_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        background_layer.set_size(layers::types::Size::percent(1.0, 1.0), None);
        background_layer.set_background_color(Color::new_rgba(1.0, 0.0, 0.0, 1.0), None);
        background_layer.set_border_corner_radius(BorderRadius::new_single(20.0), None);
        background_layer.set_opacity(0.0, None);
        let windows_layer = layers_engine.new_layer();
        windows_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        let overlay_layer = layers_engine.new_layer();
        overlay_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layers_engine.scene_add_layer(root_layer.clone());
        let workspace_id = layers_engine.scene_add_layer(workspace_layer.clone());

        layers_engine.scene_add_layer_to(background_layer.clone(), Some(workspace_id));
        layers_engine.scene_add_layer_to(windows_layer.clone(), Some(workspace_id));

        layers_engine.scene_add_layer(overlay_layer.clone());

        let scene_element = SceneElement::with_engine(layers_engine.clone());
        let app_switcher = AppSwitcherView::new(layers_engine.clone());
        let mut background_view = BackgroundView::new(layers_engine.clone(), background_layer.clone());
        let background_image = image_from_path("./resources/background.jpg");
        background_view.state.image = Some(background_image);
        background_view.render();

        ScreenComposer {
            backend_data,
            display_handle: dh,
            socket_name,
            running: Arc::new(AtomicBool::new(true)),
            handle,
            space: Space::default(),
            popups: PopupManager::default(),
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
            dnd_icon: None,
            suppressed_keys: Vec::new(),
            cursor_status,
            seat_name,
            seat,
            pointer,
            clock,
            #[cfg(feature = "xwayland")]
            xwayland,
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xdisplay: None,
            #[cfg(feature = "debug")]
            renderdoc: renderdoc::RenderDoc::new().ok(),

            // WIP workspace
            show_window_preview: false,
            layers_engine,
            scene_element,
            app_switcher,
            window_views: HashMap::new(),
            show_all: false,
            show_desktop: false,
            windows_layer,
            overlay_layer,
            workspace_layer,
            background_view,
            swipe_gesture: (0.0, 0.0).into(),
            pinch_gesture: (0.0, 0.0).into(),
            is_swiping: false,
            is_pinching: false,
        }
    }

    pub fn update_appswitcher(&mut self) {
        let windows = self.xdg_shell_state.toplevel_surfaces().iter()
            .map(|tl| {
                self.space.elements().find(|window| {
                    if let Some(surface) = window.wl_surface().as_ref() {
                        surface.id() == tl.wl_surface().id()
                    } else {
                        false
                    }
                }).unwrap().to_owned()
            }).collect::<Vec<_>>();
        self.app_switcher.update_with_window_elements(windows.as_slice());
    }

    fn window_view_for_surface(&self, surface: &WlSurface, states: &SurfaceData, location: &smithay::utils::Point<f64, smithay::utils::Physical>, scale: f64) -> Option<WindowViewSurface> {
        let id = surface.id();
        let cached_state = states.cached_state.current::<SurfaceCachedState>();
        let surface_geometry = cached_state.geometry.unwrap_or_default().to_f64().to_physical(scale);
        let surface_attributes = states.cached_state.current::<SurfaceAttributes>();
        if let Some(render_surface) = states.data_map.get::<RendererSurfaceStateUserData>() {
            let render_surface = render_surface.borrow();
            if let Some(view) = render_surface.view() {
                let image = self.backend_data.image_for_surface(&render_surface);
                let wvs = WindowViewSurface {
                    id: id.clone(),
                    offset_x: view.offset.x as f32 * scale as f32,//geometry.loc.x as f32,
                    offset_y: view.offset.y as f32 * scale as f32,//geometry.loc.y as f32,
                    x: location.x as f32 - surface_geometry.loc.x as f32,
                    y: location.y as f32 - surface_geometry.loc.y as f32,
                    w: view.dst.w as f32 * scale as f32,//surface_geometry.size.w as f32,
                    h: view.dst.h as f32 * scale as f32,//surface_geometry.size.h as f32,
                    image,
                    commit: render_surface.current_commit(),
                    transform: surface_attributes.buffer_transform.into(),
                };
                return Some(wvs);
            }
        };
        None
    }
    pub fn update_windows(&mut self) {
        let windows = self.space.elements();
        for window in windows {
            let output = self.space.outputs_for_element(window);
            let scale_factor = output.first().map(|output| output.current_scale()).unwrap_or(smithay::output::Scale::Fractional(1.0)).fractional_scale();
            let window_surface = window.wl_surface().unwrap();
            let id = window_surface.id();
            let location = self.space.element_location(window).unwrap_or((0,0).into()).to_f64().to_physical(scale_factor);
            let window_geometry = self.space.element_geometry(window).unwrap_or_default().to_f64().to_physical(scale_factor);

            let mut title = "".to_string();
            
            smithay::wayland::compositor::with_states(
                window.wl_surface().as_ref().unwrap(),
                |states| {
                    let attributes = states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap();

                    title = attributes.title.as_ref().cloned().unwrap_or_default();
                });

            let mut render_elements = VecDeque::new();
            PopupManager::popups_for_surface(&window_surface).for_each(|(popup, popup_offset)| {
                let offset: smithay::utils::Point<f64, smithay::utils::Physical> = (popup_offset - popup.geometry().loc)
                    .to_physical_precise_round(scale_factor);
                let popup_surface = popup.wl_surface();
                with_surfaces_surface_tree(
                    popup_surface,
                    |surface, states| {
                        if let Some(window_view) = self.window_view_for_surface(surface, states, &offset, scale_factor) {
                            render_elements.push_front(window_view);
                        }
                    }
                );
            });
            let initial_location:smithay::utils::Point<f64, smithay::utils::Physical> = (0.0, 0.0).into();

            smithay::wayland::compositor::with_surface_tree_downward(
                &window_surface,
                initial_location,
                |_, states, location| {
                    let mut location = *location;
                    let data = states.data_map.get::<RendererSurfaceStateUserData>();
                    let cached_state = states.cached_state.current::<SurfaceCachedState>();
                    let surface_geometry = cached_state.geometry.unwrap_or_default();
            
                    if let Some(data) = data {
                        let data = &*data.borrow();
        
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
                    if let Some(window_view) = self.window_view_for_surface(surface, states, location, scale_factor) {
                        render_elements.push_front(window_view);
                    }
                },
            |_, _, _| {
                true
            }, );

            
            if let Some(view) = self.window_views.get_mut(&id) {
                view.state.window_element = Some(window.clone());
                view.state.x = location.x as f32;
                view.state.y = location.y as f32;
                view.state.w = window_geometry.size.w as f32;
                view.state.h = window_geometry.size.h as f32;
                view.state.render_elements = render_elements.into();
                view.state.title = title;
                view.render();
            }
        }
    }
    pub fn expose_show_desktop(&mut self, delta: layers::types::Point) {
        let toplevels = self.xdg_shell_state.toplevel_surfaces().iter();

        for toplevel in toplevels {
            let id = toplevel.wl_surface().id();
            if let Some(window_layer_id) = self.windows_layer.id() {
                let view = self.window_views.entry(id).or_insert_with(|| WindowView::new(self.layers_engine.clone(), window_layer_id));
                let delta = delta.x.clamp(0.0, 1.0);

                let to_x = -view.state.w;
                let x= view.state.x.interpolate(&to_x, delta);
                let to_y = -view.state.h;
                let y= view.state.y.interpolate(&to_y, delta);
                
                if delta != 0.0 && delta != 1.0 {
                    view.layer.set_position(layers::types::Point {
                        x,
                        y,
                    }, None);
                } else {
                    view.layer.set_position(layers::types::Point {
                        x,
                        y,
                    }, Some(Transition {
                        duration: 0.5,
                        timing: TimingFunction::Easing(Easing::ease_in()),
                        ..Default::default()
                    }));
                }
            }
            
        }
    }

    pub fn expose_show_all(&mut self, delta: f32) {

        let toplevels: std::slice::Iter<'_, ToplevelSurface> = self.xdg_shell_state.toplevel_surfaces().iter();
        let padding_top = 200.0;
        let screen_size_w = self.scene_element.size.0;
        let screen_size_h = self.scene_element.size.1 - padding_top;
        let bin = bin_pack(&self.window_views, screen_size_w, screen_size_h);

        
        let mut delta = delta.max(0.0);
        delta = delta.powf(0.7);

        for (_, window) in self.window_views.iter() {
            
            let id = window.layer.id().unwrap();
            let id:usize = id.0.into();
            if let Some(rect) = bin.find_by_id(id as isize) {
                let to_x = rect.x() as f32;
                let to_y = rect.y() as f32;
                let to_width = rect.width() as f32;
                let to_height = rect.height() as f32;
                let size = window.layer.size();
                let (window_width, window_height) = match (size.width, size.height) {
                    (taffy::Dimension::Points(width), taffy::Dimension::Points(height)) => (width, height),
                    _ => (0.0, 0.0),
                };
                let scale_x = to_width / window_width;
                let scale_y = to_height / window_height;
                let scale = scale_x.min(scale_y).min(1.0);

                let scale = 1.0.interpolate(&scale, delta);
                let delta = delta.clamp(0.0, 1.0);
                let x= window.state.x.interpolate(&to_x, delta);
                let y= window.state.y.interpolate(&to_y, delta);
                if delta != 0.0 && delta != 1.0 {
                    window.layer.set_position(layers::types::Point {
                        x,
                        y,
                    }, None);
                    window.layer.set_scale(layers::types::Point {
                        x: scale,
                        y: scale,
                    }, None);
                } else {
                    window.layer.set_position(layers::types::Point {
                        x,
                        y,
                    }, Some(Transition {
                        duration: 0.5,
                        timing: TimingFunction::Easing(Easing::ease_in()),
                        ..Default::default()
                    }));
                    window.layer.set_scale(layers::types::Point {
                        x: scale,
                        y: scale,
                    }, Some(Transition {
                        duration: 0.5,
                        timing: TimingFunction::Easing(Easing::ease_in()),
                        ..Default::default()
                    }));
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct SurfaceDmabufFeedback<'a> {
    pub render_feedback: &'a DmabufFeedback,
    pub scanout_feedback: &'a DmabufFeedback,
}

#[profiling::function]
pub fn post_repaint(
    output: &Output,
    render_element_states: &RenderElementStates,
    space: &Space<WindowElement>,
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
        window.send_frame(
            output,
            time,
            Some(Duration::ZERO),
            |_, _| Some(output.clone()),
        )
        // if space.outputs_for_element(window).contains(output) {
        //     window.send_frame(output, time, throttle, surface_primary_scanout_output);
        //     if let Some(dmabuf_feedback) = dmabuf_feedback {
        //         window.send_dmabuf_feedback(output, surface_primary_scanout_output, |surface, _| {
        //             select_dmabuf_feedback(
        //                 surface,
        //                 render_element_states,
        //                 dmabuf_feedback.render_feedback,
        //                 dmabuf_feedback.scanout_feedback,
        //             )
        //         });
        //     }
        // }
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
            layer_surface.send_dmabuf_feedback(output, surface_primary_scanout_output, |surface, _| {
                select_dmabuf_feedback(
                    surface,
                    render_element_states,
                    dmabuf_feedback.render_feedback,
                    dmabuf_feedback.scanout_feedback,
                )
            });
        }
    }
}

#[profiling::function]
pub fn take_presentation_feedback(
    output: &Output,
    space: &Space<WindowElement>,
    render_element_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut output_presentation_feedback = OutputPresentationFeedback::new(output);

    space.elements().for_each(|window| {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut output_presentation_feedback,
                surface_primary_scanout_output,
                |surface, _| surface_presentation_feedback_flags_from_states(surface, render_element_states),
            );
        }
    });
    let map = smithay::desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.take_presentation_feedback(
            &mut output_presentation_feedback,
            surface_primary_scanout_output,
            |surface, _| surface_presentation_feedback_flags_from_states(surface, render_element_states),
        );
    }

    output_presentation_feedback
}

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
    fn image_for_surface(&self, surface: &RendererSurfaceState) -> Option<skia_safe::Image>;
}
