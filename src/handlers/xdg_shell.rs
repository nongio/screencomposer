use std::{cell::RefCell, collections::HashMap};

use layers::{
    prelude::{LayersEngine, TimingFunction, Transition},
    types::{Color, PaintColor, Point},
};
use smithay::{
    backend::renderer::utils::{CommitCounter, RendererSurfaceStateUserData},
    delegate_xdg_shell,
    desktop::{PopupKind, PopupManager, Space, Window},
    input::{
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        Seat,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::ObjectId,
            protocol::{wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Logical, Rectangle, Serial},
    wayland::{
        compositor::{self, with_states, SubsurfaceCachedState, TraversalAction},
        seat::WaylandFocus,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    grabs::{MoveSurfaceGrab, ResizeData, ResizeState, ResizeSurfaceGrab},
    state::{Backend, SurfaceLayer},
    ScreenComposer,
};
use tracing::trace;

use super::element::WindowElement;

#[derive(Default)]
pub struct SurfaceData {
    pub geometry: Option<Rectangle<i32, Logical>>,
    pub resize_state: ResizeState,
}

#[derive(Default)]
pub struct FullscreenSurface(RefCell<Option<WindowElement>>);

impl FullscreenSurface {
    pub fn set(&self, window: WindowElement) {
        *self.0.borrow_mut() = Some(window);
    }

    pub fn get(&self) -> Option<WindowElement> {
        self.0.borrow().clone()
    }

    pub fn clear(&self) -> Option<WindowElement> {
        self.0.borrow_mut().take()
    }
}

impl<BackendData: Backend> XdgShellHandler for ScreenComposer<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let sid = surface.wl_surface().id();

        trace!("new toplevel {}", sid);
        let window = Window::new(surface);
        let layer_map = self.layer_for(&sid);
        if let Some(SurfaceLayer {
            layer: window_layer,
            ..
        }) = layer_map
        {
            window_layer.set_opacity(0.0, None);
            window_layer.set_size(
                layers::types::Size {
                    x: window.geometry().size.w as f32,
                    y: window.geometry().size.h as f32,
                },
                None,
            );

            window_layer.set_position(layers::types::Point { x: 0.0, y: 0.0 }, None);
            window_layer.set_border_width(1.0, None);
            window_layer.set_background_color(
                PaintColor::Solid {
                    color: Color::new_rgba255(0, 0, 0, 0),
                },
                None,
            );

            // window decorations
            window_layer.set_shadow_offset((10.0, 10.0), None);
            window_layer.set_shadow_color(Color::new_rgba255(0, 0, 0, 255), None);
            window_layer.set_shadow_radius(20.0, None);
            window_layer.set_shadow_spread(5.0, None);
            window_layer.set_border_corner_radius(10.0, None);

            self.engine.scene_add_layer(window_layer.clone());
            window_layer.set_opacity(
                1.0,
                Some(Transition {
                    duration: 1.0,
                    delay: 0.0,
                    timing: TimingFunction::default(),
                }),
            );
            self.space.map_element(window, (0, 0), false);
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        trace!("new popup {:?}", surface.wl_surface().id());
        let _ = self.popups.track_popup(PopupKind::Xdg(surface.clone()));
        let parent_sid = surface.get_parent_surface().unwrap().id();
        let sid = surface.wl_surface().id();
        let layer_map = self.layer_for(&sid);

        if let Some(SurfaceLayer {
            layer: popup_layer,
            commit_counter: cc,
            parent: _,
        }) = layer_map
        {
            trace!("{:?} the popup is already mapped, adding to scene", sid);
            popup_layer.set_opacity(0.0, None);
            popup_layer.set_background_color(
                PaintColor::Solid {
                    color: Color::new_rgba255(0, 0, 0, 0),
                },
                None,
            );
            popup_layer.set_border_corner_radius(10.0, None);
            let parent_layer = self
                .layer_for(&parent_sid)
                .map(|SurfaceLayer { layer: l, .. }| l.id().unwrap());
            self.engine
                .scene_add_layer_to(popup_layer.clone(), parent_layer);

            self.map_layer(sid, popup_layer.clone(), cc, Some(parent_sid));
            popup_layer.set_opacity(
                1.0,
                Some(Transition {
                    duration: 0.2,
                    delay: 0.0,
                    timing: TimingFunction::default(),
                }),
            );
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            // NOTE: This is again a simplification, a proper compositor would
            // calculate the geometry of the popup here. For simplicity we just
            // use the default implementation here that does not take the
            // window position and output constraints into account.
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
            trace!("reposition request popup geometry {:?}", geometry);
        });
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat: Seat<ScreenComposer<BackendData>> = Seat::from_resource(&seat).unwrap();
        self.move_request_xdg(&surface, &seat, serial)
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat: Seat<ScreenComposer<BackendData>> = Seat::from_resource(&seat).unwrap();
        // TODO: touch resize.
        let pointer = seat.get_pointer().unwrap();

        // Check that this surface has a click grab.
        if !pointer.has_grab(serial) {
            return;
        }

        let start_data = pointer.grab_start_data().unwrap();

        let window = self.window_for_surface(surface.wl_surface()).unwrap();

        // If the focus was for a different surface, ignore the request.
        if start_data.focus.is_none()
            || !start_data
                .focus
                .as_ref()
                .unwrap()
                .0
                .same_client_as(&surface.wl_surface().id())
        {
            return;
        }

        let geometry = window.geometry();
        let loc = self.space.element_location(&window).unwrap();
        let (initial_window_location, initial_window_size) = (loc, geometry.size);

        with_states(surface.wl_surface(), move |states| {
            states
                .data_map
                .get::<RefCell<SurfaceData>>()
                .unwrap()
                .borrow_mut()
                .resize_state = ResizeState::Resizing(ResizeData {
                edges: edges.into(),
                initial_window_location,
                initial_window_size,
            });
        });

        let grab = ResizeSurfaceGrab {
            start_data,
            window,
            edges: edges.into(),
            initial_window_location,
            initial_window_size,
            last_window_size: initial_window_size,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // TODO popup grabs
    }
    fn maximize_request(&mut self, surface: ToplevelSurface) {
        // NOTE: This should use layer-shell when it is implemented to
        // get the correct maximum size
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Maximize)
        {
            let window = self.window_for_surface(surface.wl_surface()).unwrap();
            let outputs_for_window = self.space.outputs_for_element(&window);
            let output = outputs_for_window
                .first()
                // The window hasn't been mapped yet, use the primary output instead
                .or_else(|| self.space.outputs().next())
                // Assumes that at least one output exists
                .expect("No outputs found");
            let geometry = self.space.output_geometry(output).unwrap();

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
                state.size = Some(geometry.size);
            });
            if let Some(SurfaceLayer { layer, .. }) =
                self.layer_for(&window.toplevel().wl_surface().id())
            {
                layer.set_size(
                    layers::types::Size {
                        x: (geometry.size.w as f32),
                        y: (geometry.size.h as f32),
                    },
                    Some(Transition {
                        duration: 2.0,
                        delay: 0.0,
                        timing: TimingFunction::default(),
                    }),
                );
                layer.set_position(
                    Point {
                        x: -(geometry.loc.x as f32),
                        y: -(geometry.loc.y as f32),
                    },
                    Some(Transition {
                        duration: 1.0,
                        delay: 0.0,
                        timing: TimingFunction::default(),
                    }),
                );
            }
            self.space.map_element(window, geometry.loc, true);
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Maximized)
        {
            return;
        }

        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Maximized);
            state.size = None;
        });
        surface.send_pending_configure();
    }
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let sid = surface.wl_surface().id();
        println!("toplevel destroyed {}", sid);
        // let layer_map = self.layer_for(&sid);
        // if let Some(SurfaceLayer { layer, .. }) = layer_map {
        //     trace!("removing layer from scene {:?}", sid);
        //     self.engine.scene_remove_layer(layer.id());
        // }
        compositor::with_surface_tree_upward(
            surface.wl_surface(),
            (),
            |_, _, _| TraversalAction::DoChildren(()),
            |subsurface, _states, _| {
                let ssid = subsurface.id();
                let layer_map = self.layer_for(&ssid);
                if let Some(SurfaceLayer { layer, .. }) = layer_map {
                    println!("removing sublayer from scene {:?}", sid);
                    self.engine.scene_remove_layer(layer.id());
                }
                self.unmap_layer(&ssid);
            },
            |_, _, _| true,
        );
    }
    fn popup_destroyed(&mut self, surface: PopupSurface) {
        let ssid = surface.wl_surface().id();
        println!("popup destroyed {}", ssid);
        let layer_map = self.layer_for(&ssid);
        if let Some(SurfaceLayer { layer, .. }) = layer_map {
            trace!("removing layer {:?}", ssid);
            self.engine.scene_remove_layer(layer.id());
        }
        self.unmap_layer(&ssid);
        // compositor::with_surface_tree_upward(
        //     surface.wl_surface(),
        //     (),
        //     |_, _, _| TraversalAction::DoChildren(()),
        //     |subsurface, _states, _| {
        //         let ssid = subsurface.id();
        //         let layer_map = self.layer_for(&ssid);
        //         if let Some(SurfaceLayer { layer, .. }) = layer_map {
        //             trace!("removing layer {:?}", ssid);
        //             self.engine.scene_remove_layer(layer.id());
        //         }
        //         self.unmap_layer(&ssid);
        //     },
        //     |_, _, _| true,
        // );
    }
}

// Xdg Shell
delegate_xdg_shell!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

// fn check_grab<BackendData: Backend>(
//     seat: &Seat<ScreenComposer<BackendData>>,
//     surface: &WlSurface,
//     serial: Serial,
// ) -> Option<PointerGrabStartData<ScreenComposer<BackendData>>> {
//     let pointer = seat.get_pointer()?;

//     // Check that this surface has a click grab.
//     if !pointer.has_grab(serial) {
//         return None;
//     }

//     let start_data = pointer.grab_start_data()?;

//     let (focus, _) = start_data.focus.as_ref()?;
//     // If the focus was for a different surface, ignore the request.
//     if !focus.id().same_client_as(&surface.id()) {
//         return None;
//     }

//     Some(start_data)
// }

/// Should be called on `WlSurface::commit`
pub fn handle_commit(
    popups: &mut PopupManager,
    space: &Space<Window>,
    surface: &WlSurface,
    layers_map: &mut HashMap<ObjectId, SurfaceLayer>,
    _engine: &LayersEngine,
) {
    // Handle toplevel commits.
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().wl_surface() == surface)
        .cloned()
    {
        trace!("handle_commit toplevel {:?}", surface.id());
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().send_configure();
        }

        if let Some(SurfaceLayer { layer, .. }) =
            layers_map.get(&window.toplevel().wl_surface().id())
        {
            layer.set_size(
                layers::types::Size {
                    x: (window.bbox().size.w as f32),
                    y: (window.bbox().size.h as f32),
                },
                None,
            );

            // layer.set_position(
            //     Point {
            //         x: -(window.geometry().loc.x as f32) / 1.5,
            //         y: -(window.geometry().loc.y as f32) / 1.5,
            //     },
            //     None,
            // );
            if window.bbox().size.w != 0 && window.bbox().size.h != 0 {
                layer.set_anchor_point(
                    Point {
                        x: (window.geometry().loc.x as f32 / window.bbox().size.w as f32),
                        y: (window.geometry().loc.y as f32 / window.bbox().size.h as f32),
                    },
                    None,
                );
                layer.set_size(
                    (window.bbox().size.w as f32, window.bbox().size.h as f32),
                    None,
                );
            }
            layers_map.insert(
                window.toplevel().wl_surface().id(),
                SurfaceLayer {
                    layer: layer.clone(),
                    commit_counter: CommitCounter::from(0),
                    parent: None,
                },
            );
        }
    }

    // Handle popup commits.
    popups.commit(surface);
    if let Some(popupkind) = popups.find_popup(surface) {
        trace!("handle_commit popup {:?}", surface.id());

        let (initial_configure_sent, loc) = with_states(surface, |states| {
            let states = states
                .data_map
                .get::<XdgPopupSurfaceData>()
                .unwrap()
                .lock()
                .unwrap();
            (states.initial_configure_sent, states.current.geometry.loc)
        });

        if let Some(SurfaceLayer { layer, .. }) = layers_map.get(&surface.id()) {
            // println!("popupkind.geometry() {:?}", popupkind.geometry());

            // println!("popupkind.location() {:?}", loc);
            layer.set_position(
                Point {
                    x: (loc.x as f32),
                    y: (loc.y as f32),
                },
                None,
            );
            layer.set_background_color(
                PaintColor::Solid {
                    color: Color::new_rgba255(0, 0, 0, 0),
                },
                None,
            );
            layer.set_size(
                layers::types::Size {
                    x: (popupkind.geometry().size.w as f32),
                    y: (popupkind.geometry().size.h as f32),
                },
                None,
            );
            // if !(*appended) {
            layers_map.insert(
                surface.id(),
                SurfaceLayer {
                    layer: layer.clone(),
                    commit_counter: CommitCounter::from(0),
                    parent: None,
                },
            );
            // }
        }
        if !initial_configure_sent {
            // NOTE: This should never fail as the initial configure is always
            // allowed.
            if let PopupKind::Xdg(ref popup) = popupkind {
                popup.send_configure().expect("initial configure failed");
            }
        }
    }
    compositor::with_surface_tree_upward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |subsurface, states, _| {
            if subsurface.id() == surface.id() {
                return;
            }
            trace!("handle_commit subsurface {:?}", subsurface.id());

            let pending_location = states
                .cached_state
                .pending::<SubsurfaceCachedState>()
                .location;

            let surface_size = states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .map(|d| d.borrow().surface_size().unwrap_or_default())
                .unwrap_or_default();

            println!(">> location {:?}", pending_location);
            println!(">> surface_size {:?}", surface_size);

            if let Some(SurfaceLayer { layer, .. }) = layers_map.get(&subsurface.id()) {
                layer.set_size((surface_size.w as f32, surface_size.h as f32), None);
                layer.set_position((pending_location.x as f32, pending_location.y as f32), None);
            }
        },
        |_, _, _| true,
    );
}

impl<BackendData: Backend> ScreenComposer<BackendData> {
    pub fn move_request_xdg(
        &mut self,
        surface: &ToplevelSurface,
        seat: &Seat<Self>,
        serial: Serial,
    ) {
        // TODO: touch move.
        let pointer = seat.get_pointer().unwrap();

        // Check that this surface has a click grab.
        if !pointer.has_grab(serial) {
            return;
        }

        let start_data = pointer.grab_start_data().unwrap();

        // If the client disconnects after requesting a move
        // we can just ignore the request
        let Some(window) = self.window_for_surface(surface.wl_surface()) else {
            return;
        };

        // If the focus was for a different surface, ignore the request.
        if start_data.focus.is_none()
            || !start_data
                .focus
                .as_ref()
                .unwrap()
                .0
                .same_client_as(&surface.wl_surface().id())
        {
            return;
        }

        let mut initial_window_location = self.space.element_location(&window).unwrap();

        // If surface is maximized then unmaximize it
        let current_state = surface.current_state();
        if current_state
            .states
            .contains(xdg_toplevel::State::Maximized)
        {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.size = None;
            });

            surface.send_configure();

            // NOTE: In real compositor mouse location should be mapped to a new window size
            // For example, you could:
            // 1) transform mouse pointer position from compositor space to window space (location relative)
            // 2) divide the x coordinate by width of the window to get the percentage
            //   - 0.0 would be on the far left of the window
            //   - 0.5 would be in middle of the window
            //   - 1.0 would be on the far right of the window
            // 3) multiply the percentage by new window width
            // 4) by doing that, drag will look a lot more natural
            //
            // but for anvil needs setting location to pointer location is fine
            let pos = pointer.current_location();
            initial_window_location = (pos.x as i32, pos.y as i32).into();
        }

        let grab = MoveSurfaceGrab {
            start_data,
            window,
            initial_window_location,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }
}
