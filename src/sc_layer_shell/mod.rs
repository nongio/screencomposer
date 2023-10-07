mod protocol;

use std::sync::{Arc, Mutex};

use layers::{
    engine::TransactionRef,
    prelude::Transition,
    types::{Color, PaintColor, Point},
};
use smithay::{
    backend::renderer::utils::CommitCounter,
    utils::{Logical, Serial, Size},
    wayland::compositor::{self, Cacheable},
};
use wayland_backend::server::{ClientId, GlobalId, ObjectId};
use wayland_server::{
    protocol::wl_surface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::{
    sc_layer_shell::protocol::ScLayerShellHandler,
    state::{Backend, SurfaceLayer},
    ScreenComposer,
};

use self::protocol::{sc_animation_v1::ScAnimationV1, sc_layer_surface_v1, sc_shell_unstable_v1};

type ZscLayerShellV1 = protocol::sc_shell_unstable_v1::ScShellUnstableV1;
type ZscLayerShellSurfaceV1 = protocol::sc_layer_surface_v1::ScLayerSurfaceV1;
type ZscLayerShellAnimationV1 = protocol::sc_animation_v1::ScAnimationV1;
/// A handle to a layer surface
#[derive(Clone)]
pub struct LayerSurface {
    wl_surface: wl_surface::WlSurface,
    sc_layer: ZscLayerShellSurfaceV1,
    layer: layers::prelude::Layer,
}

/// State of a layer surface
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LayerSurfaceState {
    /// The suggested size of the surface
    pub size: Size<i32, Logical>,
}

/// A configure message for layer surfaces
#[derive(Debug, Clone)]
pub struct ScLayerSurfaceConfigure {
    /// The state associated with this configure
    pub state: LayerSurfaceState,

    /// A serial number to track ACK from the client
    ///
    /// This should be an ever increasing number, as the ACK-ing
    /// from a client for a serial will validate all pending lower
    /// serials.
    pub serial: Serial,
}
/// Attributes for layer surface
#[derive(Debug)]
pub struct LayerSurfaceAttributes {
    surface: ZscLayerShellSurfaceV1,
    /// Defines if the surface has received at least one
    /// layer_surface.ack_configure from the client
    pub configured: bool,
    /// The serial of the last acked configure
    pub configure_serial: Option<Serial>,
    /// Holds the state if the surface has sent the initial
    /// configure event to the client. It is expected that
    /// during the first commit a initial
    /// configure event is sent to the client
    pub initial_configure_sent: bool,
    /// Holds the configures the server has sent out
    /// to the client waiting to be acknowledged by
    /// the client. All pending configures that are older
    /// than the acknowledged one will be discarded during
    /// processing layer_surface.ack_configure.
    pending_configures: Vec<ScLayerSurfaceConfigure>,
    /// Holds the pending state as set by the server.
    pub server_pending: Option<LayerSurfaceState>,
    /// Holds the last server_pending state that has been acknowledged
    /// by the client. This state should be cloned to the current
    /// during a commit.
    pub last_acked: Option<LayerSurfaceState>,
    /// Holds the current state of the layer after a successful
    /// commit.
    pub current: LayerSurfaceState,
}

/// Represents the client pending state
#[derive(Debug, Default, Clone, Copy)]
pub struct LayerSurfaceCachedState {
    /// The size requested by the client
    pub size: Size<i32, Logical>,
    position: Point,
    background_color: Color,
}
// trait needed to cache the state
impl Cacheable for LayerSurfaceCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        *self
    }
    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

impl LayerSurfaceAttributes {
    fn new(surface: ZscLayerShellSurfaceV1) -> Self {
        Self {
            surface,
            configured: false,
            configure_serial: None,
            initial_configure_sent: false,
            pending_configures: Vec::new(),
            server_pending: None,
            last_acked: None,
            current: Default::default(),
        }
    }

    fn ack_configure(&mut self, serial: Serial) -> Option<ScLayerSurfaceConfigure> {
        let configure = self
            .pending_configures
            .iter()
            .find(|configure| configure.serial == serial)
            .cloned()?;

        self.last_acked = Some(configure.state.clone());

        self.configured = true;
        self.configure_serial = Some(serial);
        self.pending_configures.retain(|c| c.serial > serial);
        Some(configure)
    }

    fn reset(&mut self) {
        self.configured = false;
        self.configure_serial = None;
        self.initial_configure_sent = false;
        self.pending_configures = Vec::new();
        self.server_pending = None;
        self.last_acked = None;
        self.current = Default::default();
    }

    fn current_server_state(&self) -> &LayerSurfaceState {
        self.pending_configures
            .last()
            .map(|c| &c.state)
            .or(self.last_acked.as_ref())
            .unwrap_or(&self.current)
    }

    fn has_pending_changes(&self) -> bool {
        self.server_pending
            .as_ref()
            .map(|s| s != self.current_server_state())
            .unwrap_or(false)
    }
}

/// Shell global state
///
/// This state allows you to retrieve a list of surfaces
/// currently known to the shell global.
#[derive(Clone)]
pub struct ScLayerShellState {
    known_layers: Arc<Mutex<Vec<LayerSurface>>>,
    shell_global: GlobalId,
}

/// User data for wlr layer surface
pub struct ScLayerSurfaceUserData {
    shell_data: ScLayerShellState,
    wl_surface: wl_surface::WlSurface,
    // alive_tracker: AliveTracker,
}

pub struct ScAnimationUserData {
    shell_data: ScLayerShellState,
}
impl ScLayerShellState {
    /// Create a new `wlr_layer_shell` globals
    pub fn new<D>(display: &DisplayHandle) -> ScLayerShellState
    where
        D: GlobalDispatch<ZscLayerShellV1, ()>,
        D: 'static,
    {
        let shell_global = display.create_global::<D, ZscLayerShellV1, _>(1, ());

        ScLayerShellState {
            known_layers: Default::default(),
            shell_global,
        }
    }

    /// Get shell global id
    pub fn shell_global(&self) -> GlobalId {
        self.shell_global.clone()
    }

    /// Access all the shell surfaces known by this handler
    pub fn layer_surfaces(&self) -> impl DoubleEndedIterator<Item = LayerSurface> {
        self.known_layers.lock().unwrap().clone().into_iter()
    }
}

impl<BackendData: Backend> Dispatch<ZscLayerShellV1, ()> for ScreenComposer<BackendData> {
    fn request(
        state: &mut Self,
        client: &Client,
        shell: &ZscLayerShellV1,
        request: <ZscLayerShellV1 as Resource>::Request,
        data: &(),
        dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            sc_shell_unstable_v1::Request::GetLayerSurface {
                id,
                surface: wl_surface,
                output,
            } => {
                // if compositor::give_role(&wl_surface, SCLAYER_SURFACE_ROLE).is_err() {
                //     shell.post_error(
                //         sc_shell_unstable_v1::Error::Role,
                //         "Surface already has a role.",
                //     );
                //     return;
                // }

                let id = data_init.init(
                    id,
                    ScLayerSurfaceUserData {
                        shell_data: state.shell_state().clone(),
                        wl_surface: wl_surface.clone(),
                        // alive_tracker: Default::default(),
                    },
                );

                println!("new surface {:?}", wl_surface.id());

                let initial = compositor::with_states(&wl_surface, |states| {
                    let inserted = states.data_map.insert_if_missing_threadsafe(|| {
                        Mutex::new(LayerSurfaceAttributes::new(id.clone()))
                    });

                    if !inserted {
                        let mut attributes = states
                            .data_map
                            .get::<Mutex<LayerSurfaceAttributes>>()
                            .unwrap()
                            .lock()
                            .unwrap();
                        attributes.surface = id.clone();
                    }

                    // states
                    //     .cached_state
                    //     .pending::<LayerSurfaceCachedState>()
                    //     .layer = layer;

                    inserted
                });

                if initial {
                    compositor::add_pre_commit_hook::<ScreenComposer<BackendData>, _>(
                        &wl_surface,
                        |_state, _dh, surface| {
                            compositor::with_states(surface, |states| {
                                let mut guard = states
                                    .data_map
                                    .get::<Mutex<LayerSurfaceAttributes>>()
                                    .unwrap()
                                    .lock()
                                    .unwrap();

                                // let pending = states.cached_state.pending::<LayerSurfaceCachedState>();

                                // if pending.size.w == 0 && !pending.anchor.anchored_horizontally() {
                                //     guard.surface.post_error(
                                //         zwlr_layer_surface_v1::Error::InvalidSize,
                                //         "width 0 requested without setting left and right anchors",
                                //     );
                                //     return;
                                // }

                                // if pending.size.h == 0 && !pending.anchor.anchored_vertically() {
                                //     guard.surface.post_error(
                                //         zwlr_layer_surface_v1::Error::InvalidSize,
                                //         "height 0 requested without setting top and bottom anchors",
                                //     );
                                //     return;
                                // }

                                if let Some(state) = guard.last_acked.clone() {
                                    guard.current = state;
                                }
                            });
                        },
                    );
                }

                let layer = state.engine.new_layer();
                state.map_layer(id.id(), layer.clone(), CommitCounter::from(0), None);

                // state
                //     .shell_state()
                //     .known_layers
                //     .lock()
                //     .unwrap()
                //     .push(handle.clone());
                let ls = LayerSurface {
                    wl_surface: wl_surface.clone(),
                    sc_layer: id.clone(),
                    layer,
                };

                ScLayerShellHandler::new_layer_surface(state, ls, output);
            }
            sc_shell_unstable_v1::Request::GetAnimation {
                id,
                duration,
                speed,
            } => {
                let id = data_init.init(
                    id,
                    ScAnimationUserData {
                        shell_data: state.shell_state().clone(),
                    },
                );
                let animation = state.engine.new_animation(Transition {
                    duration: (duration * speed) as f32,
                    ..Default::default()
                });
                state.animations_map.insert(id.id(), animation);
                // ScLayerShellHandler::get_animation(state, animation);
            }
            sc_shell_unstable_v1::Request::Destroy => {
                // Handled by destructor
            }
            _ => {}
        }
    }
    fn destroyed(state: &mut Self, client: ClientId, resource: ObjectId, data: &()) {
        let layer_surface = state.layer_for(&resource);
        if let Some(layer) = layer_surface {
            // ScLayerShellHandler::destroy_layer_surface(state, layer_surface);
        }
    }
}

fn with_surface_pending_state<F, T>(layer_surface: &ZscLayerShellSurfaceV1, f: F) -> T
where
    F: FnOnce(&mut LayerSurfaceCachedState) -> T,
{
    let data = layer_surface.data::<ScLayerSurfaceUserData>().unwrap();
    compositor::with_states(&data.wl_surface, |states| {
        f(&mut states.cached_state.pending::<LayerSurfaceCachedState>())
    })
}
fn attach_animation_to_transaction<BackendData: Backend>(
    state: &ScreenComposer<BackendData>,
    transaction: TransactionRef,
    animation: Option<ScAnimationV1>,
) {
    if let Some(animation) = animation {
        if let Some(animation) = state.animations_map.get(&animation.id()) {
            state.engine.attach_animation(transaction, *animation);
        }
    }
}

impl<BackendData: Backend> Dispatch<ZscLayerShellSurfaceV1, ScLayerSurfaceUserData>
    for ScreenComposer<BackendData>
{
    fn request(
        state: &mut Self,
        client: &Client,
        layer_surface: &ZscLayerShellSurfaceV1,
        request: <ZscLayerShellSurfaceV1 as Resource>::Request,
        data: &ScLayerSurfaceUserData,
        dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        println!("new request for layer id {:?}", layer_surface.id());
        println!("new request for surface id {:?}", data.wl_surface.id());

        match request {
            sc_layer_surface_v1::Request::SetSize {
                width,
                height,
                animation,
            } => {
                with_surface_pending_state(layer_surface, |data| {
                    data.size = (width as i32, height as i32).into();
                });
                if let Some(SurfaceLayer { layer, .. }) = state.layer_for(&layer_surface.id()) {
                    let transaction = layer.set_size((width as f32, height as f32), None);
                    attach_animation_to_transaction(state, transaction, animation);
                }
            }
            sc_layer_surface_v1::Request::SetPosition { x, y, animation } => {
                with_surface_pending_state(layer_surface, |data| {
                    data.position = (x as f32, y as f32).into();
                });
                if let Some(SurfaceLayer { layer, .. }) = state.layer_for(&layer_surface.id()) {
                    let transaction = layer.set_position((x as f32, y as f32), None);
                    attach_animation_to_transaction(state, transaction, animation);
                }
            }
            sc_layer_surface_v1::Request::SetBackgroundColor {
                red,
                green,
                blue,
                alpha,
                animation,
            } => {
                //         with_surface_pending_state(layer_surface, |data| {
                //             data.position = (x as f32, y as f32).into();
                //         });
                if let Some(SurfaceLayer { layer, .. }) = state.layer_for(&layer_surface.id()) {
                    let transaction = layer.set_background_color(
                        PaintColor::Solid {
                            color: Color::new_rgba255(
                                red as u8,
                                green as u8,
                                blue as u8,
                                alpha as u8,
                            ),
                        },
                        None,
                    );
                    attach_animation_to_transaction(state, transaction, animation);
                }
            }
            _ => {}
        }
    }
    fn destroyed(
        state: &mut Self,
        client: ClientId,
        resource: ObjectId,
        data: &ScLayerSurfaceUserData,
    ) {
    }
}
impl<BackendData: Backend> GlobalDispatch<ZscLayerShellV1, ()> for ScreenComposer<BackendData> {
    fn bind(
        state: &mut Self,
        dhandle: &DisplayHandle,
        client: &Client,
        resource: New<ZscLayerShellV1>,
        global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl<BackendData: Backend> Dispatch<ZscLayerShellAnimationV1, ScAnimationUserData>
    for ScreenComposer<BackendData>
{
    fn request(
        state: &mut Self,
        client: &Client,
        resource: &ZscLayerShellAnimationV1,
        request: <ZscLayerShellAnimationV1 as Resource>::Request,
        data: &ScAnimationUserData,
        dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            protocol::sc_animation_v1::Request::SetDuration { duration } => {
                println!("set duration {:?}", duration);
            }
            protocol::sc_animation_v1::Request::Start { delay } => {}
            protocol::sc_animation_v1::Request::SetBezierTimingFunction { c1x, c1y, c2x, c2y } => {
                println!("set easing {:?}", (c1x, c1y, c2x, c2y));
            }
            protocol::sc_animation_v1::Request::SetSpringTimingFunction { .. } => {
                // println!("set progress {:?}", progress);
            }
            // protocol::sc_animation_v1::Request::Stop {  } => {

            // }
            _ => {}
        }
    }
    fn destroyed(
        _state: &mut Self,
        _client: ClientId,
        _resource: ObjectId,
        _data: &ScAnimationUserData,
    ) {
    }
}
