use std::{
    cmp::max,
    collections::{HashMap, VecDeque},
    fmt::Debug,
    os::unix::io::OwnedFd,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use layers::{
    engine::{LayersEngine, NodeRef},
    prelude::{taffy, Transition},
};
use tracing::{info, warn};

use smithay::{
    backend::{
        input::TabletToolDescriptor,
        renderer::{
            element::{
                default_primary_scanout_output_compare, utils::select_dmabuf_feedback,
                RenderElementStates,
            },
            utils::{RendererSurfaceState, RendererSurfaceStateUserData},
        },
    },
    delegate_compositor, delegate_data_control, delegate_data_device, delegate_fractional_scale,
    delegate_input_method_manager, delegate_keyboard_shortcuts_inhibit, delegate_layer_shell,
    delegate_output, delegate_pointer_constraints, delegate_pointer_gestures,
    delegate_presentation, delegate_primary_selection, delegate_relative_pointer, delegate_seat,
    delegate_security_context, delegate_shm, delegate_tablet_manager, delegate_text_input_manager,
    delegate_viewporter, delegate_virtual_keyboard_manager, delegate_xdg_activation,
    delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{
        space::SpaceElement,
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
            update_surface_primary_scanout_output, with_surfaces_surface_tree,
            OutputPresentationFeedback,
        },
        PopupKind, PopupManager, Space,
    },
    input::{
        keyboard::{Keysym, XkbConfig},
        pointer::{CursorIcon, CursorImageAttributes, CursorImageStatus, PointerHandle},
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
            protocol::{
                wl_data_device_manager::DndAction, wl_data_source::WlDataSource,
                wl_surface::WlSurface,
            },
            Display, DisplayHandle, Resource,
        },
    },
    utils::{self, Clock, Monotonic, Rectangle, Serial, SERIAL_COUNTER},
    wayland::{
        compositor::{
            self, get_parent, with_states, CompositorClientState, CompositorState,
            SurfaceAttributes, SurfaceData, TraversalAction,
        },
        dmabuf::DmabufFeedback,
        fractional_scale::{
            with_fractional_scale, FractionalScaleHandler, FractionalScaleManagerState,
        },
        input_method::{InputMethodHandler, InputMethodManagerState, PopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
            KeyboardShortcutsInhibitor,
        },
        output::{OutputHandler, OutputManagerState},
        pointer_constraints::{
            with_pointer_constraint, PointerConstraintsHandler, PointerConstraintsState,
        },
        pointer_gestures::PointerGesturesState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        seat::WaylandFocus,
        security_context::{
            SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
            SecurityContextState,
        },
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            primary_selection::{
                set_primary_focus, PrimarySelectionHandler, PrimarySelectionState,
            },
            wlr_data_control::{DataControlHandler, DataControlState},
            SelectionHandler,
        },
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{
                decoration::{XdgDecorationHandler, XdgDecorationState},
                SurfaceCachedState, ToplevelSurface, XdgShellState, XdgToplevelSurfaceData,
            },
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
        tablet_manager::{TabletManagerState, TabletSeatHandler},
        text_input::TextInputManagerState,
        viewporter::ViewporterState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_activation::{
            XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
        },
        xdg_foreign::{XdgForeignHandler, XdgForeignState},
    },
};

#[cfg(feature = "xwayland")]
use crate::cursor::Cursor;
use crate::{
    config::Config,
    focus::{KeyboardFocusTarget, PointerFocusTarget},
    render_elements::scene_element::SceneElement,
    shell::WindowElement,
    skia_renderer::SkiaTexture,
    workspace::{DndView, WindowView, WindowViewBaseModel, WindowViewSurface, Workspace},
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
    pub xdg_foreign_state: XdgForeignState,

    #[cfg(feature = "xwayland")]
    pub xwayland_shell_state: xwayland_shell::XWaylandShellState,

    pub dnd_icon: Option<WlSurface>,

    // input-related fields
    pub suppressed_keys: Vec<Keysym>,
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
    // state
    pub workspace: Arc<Workspace>,
    // views
    pub window_views: HashMap<ObjectId, WindowView>,
    pub dnd_view: DndView,
    // layers
    pub layers_engine: LayersEngine,

    pub show_desktop: bool,
    pub is_swiping: bool,
    pub is_pinching: bool,
    pub is_resizing: bool,
}

delegate_compositor!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> DataDeviceHandler for ScreenComposer<BackendData> {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
    fn action_choice(
        &mut self,
        available: smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction,
        preferred: smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction,
    ) -> smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction {
        // println!("available {:?} preferred {:?}", available, preferred);
        // if the preferred action is valid (a single action) and in the available actions, use it
        // otherwise, follow a fallback stategy

        if [DndAction::Move, DndAction::Copy, DndAction::Ask].contains(&preferred)
            && available.contains(preferred)
        {
            self.load_cursor_for_action(preferred);
            preferred
        } else if available.contains(DndAction::Ask) {
            self.load_cursor_for_action(DndAction::Ask);
            DndAction::Ask
        } else if available.contains(DndAction::Copy) {
            self.load_cursor_for_action(DndAction::Copy);
            DndAction::Copy
        } else if available.contains(DndAction::Move) {
            self.load_cursor_for_action(DndAction::Move);
            DndAction::Move
        } else {
            self.load_cursor_for_action(DndAction::None);
            DndAction::empty()
        }
    }
}

impl<BackendData: Backend> ClientDndGrabHandler for ScreenComposer<BackendData> {
    fn started(
        &mut self,
        _source: Option<WlDataSource>,
        icon: Option<WlSurface>,
        _seat: Seat<Self>,
    ) {
        self.dnd_icon = icon;
        let p = self.get_cursor_position();
        let p = (p.x as f32, p.y as f32).into();
        self.dnd_view.set_initial_position(p);
        self.dnd_view.layer.set_scale((1.0, 1.0), None);

        self.dnd_view
            .layer
            .set_opacity(0.8, Some(Transition::default()));
    }
    fn dropped(&mut self, _seat: Seat<Self>) {
        self.dnd_icon = None;
        self.dnd_view
            .layer
            .set_opacity(0.0, Some(Transition::default()));
        self.dnd_view
            .layer
            .set_scale((1.2, 1.2), Some(Transition::default()));
        // self.dnd_view.layer.set_position(self.dnd_view.initial_position, Some(Transition::default()));
    }
}
impl<BackendData: Backend> ServerDndGrabHandler for ScreenComposer<BackendData> {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {
        unreachable!("Anvil doesn't do server-side grabs");
    }
}
delegate_data_device!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> OutputHandler for ScreenComposer<BackendData> {}

delegate_output!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> SelectionHandler for ScreenComposer<BackendData> {
    type SelectionUserData = ();

    #[cfg(feature = "xwayland")]
    fn new_selection(
        &mut self,
        ty: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
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
    fn new_selection(
        &mut self,
        ty: smithay::wayland::selection::SelectionTarget,
        source: Option<smithay::wayland::selection::SelectionSource>,
        _seat: Seat<Self>,
    ) {
        println!("new_selection {:?} {:?}", ty, source);
    }
    fn send_selection(
        &mut self,
        ty: smithay::wayland::selection::SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &Self::SelectionUserData,
    ) {
        println!("send_selection {:?} {:?} {:?}", ty, mime_type, fd);
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
    type KeyboardFocus = KeyboardFocusTarget<BackendData>;
    type PointerFocus = PointerFocusTarget<BackendData>;
    type TouchFocus = PointerFocusTarget<BackendData>;

    fn seat_state(&mut self) -> &mut SeatState<ScreenComposer<BackendData>> {
        &mut self.seat_state
    }

    fn focus_changed(
        &mut self,
        seat: &Seat<Self>,
        target: Option<&KeyboardFocusTarget<BackendData>>,
    ) {
        let dh = &self.display_handle;

        let wl_surface = target.and_then(WaylandFocus::wl_surface);

        let focus = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        *self.cursor_status.lock().unwrap() = image;
    }
    fn led_state_changed(
        &mut self,
        _seat: &Seat<Self>,
        _led_state: smithay::input::keyboard::LedState,
    ) {
        println!("keyboard led_state_changed {:?}", _led_state);
    }
}
delegate_seat!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> TabletSeatHandler for ScreenComposer<BackendData> {
    fn tablet_tool_image(&mut self, _tool: &TabletToolDescriptor, image: CursorImageStatus) {
        // TODO: tablet tools should have their own cursors
        let mut cursor_status = self.cursor_status.lock().unwrap();
        *cursor_status = image;
    }
}
delegate_tablet_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

delegate_text_input_manager!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> InputMethodHandler for ScreenComposer<BackendData> {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            warn!("Failed to track popup: {}", err);
        }
    }

    fn popup_repositioned(&mut self, _: PopupSurface) {}

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, smithay::utils::Logical> {
        self.space
            .elements()
            .find_map(|window| {
                (window.wl_surface().as_deref() == Some(parent)).then(|| window.geometry())
            })
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
        let Some(current_focus) = pointer.current_focus() else {
            return;
        };
        if current_focus.wl_surface().as_deref() == Some(surface) {
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
                .find(|window| window.wl_surface().map(|s| *s == surface).unwrap_or(false))
                .cloned();
            if let Some(window) = w {
                self.raise_element(&window, true, Some(SERIAL_COUNTER.next_serial()), true);
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
                DecorationMode::ServerSide => Mode::ClientSide,
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
                        self.window_for_surface(&root).and_then(|window| {
                            self.space.outputs_for_element(&window).first().cloned()
                        })
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
    fn context_created(
        &mut self,
        source: SecurityContextListenerSource,
        security_context: SecurityContext,
    ) {
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
    fn keyboard_focus_for_xsurface(
        &self,
        surface: &WlSurface,
    ) -> Option<KeyboardFocusTarget<BackendData>> {
        let elem = self
            .space
            .elements()
            .find(|elem| elem.wl_surface().as_deref() == Some(surface))?;
        Some(KeyboardFocusTarget::Window(elem.clone()))
    }
}
#[cfg(feature = "xwayland")]
delegate_xwayland_keyboard_grab!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

#[cfg(feature = "xwayland")]
delegate_xwayland_shell!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> XdgForeignHandler for ScreenComposer<BackendData> {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign_state
    }
}
smithay::delegate_xdg_foreign!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

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
                        display.get_mut().dispatch_clients(data).unwrap();
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
        TabletManagerState::new::<Self>(&dh);
        SecurityContextState::new::<Self, _>(&dh, |client| {
            client
                .get_data::<ClientState>()
                .map_or(true, |client_state| client_state.security_context.is_none())
        });
        let xdg_foreign_state = XdgForeignState::new::<Self>(&dh);

        // init input
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());

        let cursor_status = Arc::new(Mutex::new(CursorImageStatus::default_named()));
        let pointer = seat.add_pointer();
        seat.add_keyboard(XkbConfig::default(), 200, 25)
            .expect("Failed to initialize the keyboard");

        let keyboard_shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<Self>(&dh);

        #[cfg(feature = "xwayland")]
        let xwayland_shell_state = xwayland_shell::XWaylandShellState::new::<Self>(&dh.clone());

        #[cfg(feature = "xwayland")]
        XWaylandKeyboardGrabState::new::<Self>(&dh.clone());

        let layers_engine = LayersEngine::new(500.0, 500.0);
        let root_layer = layers_engine.new_layer();
        root_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layers_engine.scene_add_layer(root_layer.clone());
        let scene_element = SceneElement::with_engine(layers_engine.clone());
        let space = Space::default();
        let workspace = Workspace::new(layers_engine.clone(), cursor_status.clone());

        let dnd_view = DndView::new(layers_engine.clone(), root_layer.id().unwrap());
        ScreenComposer {
            backend_data,
            display_handle: dh,
            socket_name,
            running: Arc::new(AtomicBool::new(true)),
            handle,
            space,
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
            xdg_foreign_state,
            dnd_icon: None,
            suppressed_keys: Vec::new(),
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

            // WIP workspace
            workspace,
            layers_engine,
            scene_element,
            window_views: HashMap::new(),
            dnd_view,

            show_desktop: false,
            // support variables for gestures
            is_swiping: false,
            is_pinching: false,
            is_resizing: false,
        }
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
    pub fn update_workspace_applications(&mut self) {
        let windows = self.space.elements().filter_map(|we| {
            let id = we.wl_surface().unwrap().id();
            if let Some(wv) = self.get_window_view(&id) {
                let state = wv.view_base.get_state();
                Some((we.clone(), wv.layer.clone(), state))
            } else {
                None
            }
        });
        if !self.is_resizing {
            self.workspace.update_with_window_elements(windows);
        }
    }
    #[profiling::function]
    fn window_view_for_surface(
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
                let texture = self.backend_data.texture_for_surface(&render_surface);
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
                    texture,
                    commit: render_surface.current_commit(),
                    transform: surface_attributes.buffer_transform.into(),
                };
                return Some(wvs);
            }
        };
        None
    }
    #[profiling::function]
    pub fn update_windows(&mut self) {
        let windows = self.space.elements();
        for window in windows {
            let output = self.space.outputs_for_element(window);
            let scale_factor = output
                .first()
                .map(|output| output.current_scale())
                .unwrap_or(smithay::output::Scale::Fractional(1.0))
                .fractional_scale();
            if let Some(window_surface) = window.wl_surface() {
                let id = window_surface.id();
                let location = self
                    .space
                    .element_location(window)
                    .unwrap_or((0, 0).into())
                    .to_f64()
                    .to_physical(scale_factor);
                let window_geometry = self
                    .space
                    .element_geometry(window)
                    .unwrap_or_default()
                    .to_f64()
                    .to_physical(scale_factor);
                let mut title = "".to_string();
                let mut fullscreen = false;
                smithay::wayland::compositor::with_states(&window_surface, |states| {
                    if let Some(attributes) = states.data_map.get::<XdgToplevelSurfaceData>() {
                        let attributes = attributes.lock().unwrap();
                        title = attributes.title.as_ref().cloned().unwrap_or_default();
                        fullscreen = attributes.current.fullscreen_output.is_some();
                    }
                });

                let mut render_elements = VecDeque::new();
                PopupManager::popups_for_surface(&window_surface).for_each(
                    |(popup, popup_offset)| {
                        let offset: smithay::utils::Point<f64, smithay::utils::Physical> =
                            (popup_offset - popup.geometry().loc)
                                .to_physical_precise_round(scale_factor);
                        let popup_surface = popup.wl_surface();
                        with_surfaces_surface_tree(popup_surface, |surface, states| {
                            if let Some(window_view) =
                                self.window_view_for_surface(surface, states, &offset, scale_factor)
                            {
                                render_elements.push_front(window_view);
                            }
                        });
                    },
                );
                let initial_location: smithay::utils::Point<f64, smithay::utils::Physical> =
                    (0.0, 0.0).into();

                smithay::wayland::compositor::with_surface_tree_downward(
                    &window_surface,
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

                if let Some(window_view) = self.get_window_view(&id) {
                    let model = WindowViewBaseModel {
                        x: location.x as f32,
                        y: location.y as f32,
                        w: window_geometry.size.w as f32,
                        h: window_geometry.size.h as f32,
                        title,
                        fullscreen,
                    };

                    self.workspace.update_window(&id, &model);
                    window_view.view_base.update_state(&model);
                    window_view
                        .view_content
                        .update_state(&render_elements.into());
                }
            }
        }

        if let Some(dnd_surface) = self.dnd_icon.as_ref() {
            let cursor_position = self.get_cursor_position();

            let scale = Config::with(|c| c.screen_scale);
            let render_elements = self.get_render_elements(dnd_surface, scale);
            self.dnd_view
                .view_content
                .update_state(&render_elements.into());

            self.dnd_view
                .layer
                .set_position((cursor_position.x as f32, cursor_position.y as f32), None);
        }
    }

    // scene_element
    // window_views
    pub fn expose_show_all(&mut self, delta: f32, end_gesture: bool) {
        self.workspace.expose_show_all(delta, end_gesture);
    }

    pub fn expose_show_desktop(&mut self, delta: f32, end_gesture: bool) {
        self.workspace.expose_show_desktop(delta, end_gesture);
    }

    pub fn get_or_add_window_view(
        &mut self,
        object_id: &ObjectId,
        parent_layer_id: NodeRef,
        window: WindowElement,
    ) -> &WindowView {
        self.window_views
            .entry(object_id.clone())
            .or_insert_with(|| WindowView::new(self.layers_engine.clone(), parent_layer_id, window))
    }
    pub fn remove_window_view(&mut self, object_id: &ObjectId) {
        if let Some(_view) = self.window_views.remove(object_id) {}
    }
    pub fn get_window_view(&self, id: &ObjectId) -> Option<&WindowView> {
        self.window_views.get(id)
    }
    pub fn mut_window_view(&mut self, id: &ObjectId) -> Option<&mut WindowView> {
        self.window_views.get_mut(id)
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
        let output = self.space.output_under(cursor_pos).next().cloned().unwrap();
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
    pub fn next_window(&mut self, serial: Option<Serial>) {
        let windows = self.workspace.get_current_app_windows();

        if !windows.is_empty() {
            for (i, window_id) in windows.iter().enumerate() {
                if i == 0 {
                    if let Some(window) = self.workspace.get_window_for_surface(window_id) {
                        if let Some(we) = window.window_element.as_ref() {
                            self.raise_element(we, true, serial, true);
                        }
                    }
                }
            }
        }
    }
    pub fn prev_window(&mut self, serial: Option<Serial>) {
        let windows = self.workspace.get_current_app_windows();

        if !windows.is_empty() {
            let current_window = (windows.len() as i32) - 1;
            let current_window = max(current_window, 0) as usize;
            for (i, window_id) in windows.iter().enumerate() {
                if i == current_window {
                    if let Some(window) = self.workspace.get_window_for_surface(window_id) {
                        if let Some(we) = window.window_element.as_ref() {
                            self.raise_element(we, true, serial, true);
                        }
                    }
                }
            }
        }
    }
    pub fn raise_element(
        &mut self,
        window: &WindowElement,
        activate: bool,
        serial: Option<Serial>,
        update: bool,
    ) {
        self.space.raise_element(window, activate);
        let id = window.wl_surface().unwrap().id();
        {
            if let Some(view) = self.get_window_view(&id) {
                view.raise();
            }
        }
        if let Some(serial) = serial {
            let keyboard = self.seat.get_keyboard().unwrap();
            keyboard.set_focus(self, Some(window.clone().into()), serial);
        }
        if update {
            self.update_workspace_applications();
        }
    }
    pub fn raise_app_element(
        &mut self,
        we: &WindowElement,
        activate: bool,
        serial: Option<Serial>,
    ) {
        let id = we.wl_surface().unwrap().id();
        let window = {
            let window_wl = self.workspace.wl_windows_map.read().unwrap();
            window_wl.get(&id).unwrap().clone()
        };

        let windows = self.workspace.get_app_windows(&window.app_id);
        for window_id in windows.iter() {
            if let Some(window) = self.workspace.get_window_for_surface(window_id) {
                if let Some(we) = window.window_element.as_ref() {
                    self.raise_element(we, false, None, false);
                }
            }
        }
        self.raise_element(we, activate, serial, true);
    }
    pub fn raise_app_elements(&mut self, app_id: &str, activate: bool, serial: Option<Serial>) {
        let windows = self.workspace.get_app_windows(app_id);
        for (i, window_id) in windows.iter().rev().enumerate() {
            if let Some(window) = self.workspace.get_window_for_surface(window_id) {
                if let Some(we) = window.window_element.as_ref() {
                    if i == 0 {
                        self.raise_element(we, activate, serial, false);
                    } else {
                        self.raise_element(we, false, None, false);
                    }
                }
            }
        }
        self.update_workspace_applications();
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
        window.send_frame(output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        })
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

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
    fn texture_for_surface(&self, surface: &RendererSurfaceState) -> Option<SkiaTexture>;
    fn set_cursor(&mut self, image: &CursorImageStatus); //, renderer: &mut SkiaRenderer);
}
