use std::cell::RefCell;

use lay_rs::prelude::Transition;
use smithay::{
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, layer_map_for_output,
        PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy, Window,
        WindowSurfaceType,
    },
    input::{pointer::Focus, Seat},
    output::Output,
    reexports::{
        wayland_protocols::xdg::{decoration as xdg_decoration, shell::server::xdg_toplevel},
        wayland_server::{
            protocol::{wl_output, wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::Serial,
    wayland::{
        compositor::with_states,
        seat::WaylandFocus,
        shell::xdg::{
            Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
        },
    },
};
use tracing::{trace, warn};

use crate::{
    focus::KeyboardFocusTarget,
    shell::TouchResizeSurfaceGrab,
    state::{Backend, ScreenComposer},
};

use super::{
    fullscreen_output_geometry, FullscreenSurface, PointerMoveSurfaceGrab,
    PointerResizeSurfaceGrab, ResizeData, ResizeState, SurfaceData, TouchMoveSurfaceGrab,
    WindowElement,
};

impl<BackendData: Backend> XdgShellHandler for ScreenComposer<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // Do not send a configure here, the initial configure
        // of a xdg_surface has to be sent during the commit if
        // the surface is not already configured

        let window_layer = self.layers_engine.new_layer();
        let window_element = WindowElement::new(
            Window::new_wayland_window(surface.clone()),
            window_layer.clone(),
        );
        let wid = window_element.id();

        let pointer_location = self.pointer.current_location();
        self.workspaces
            .place_new_window(window_element, pointer_location);

        self.workspaces.refresh_space();

        let window_element = self.workspaces.get_window_for_surface(&wid).unwrap();

        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(window_element.clone().into()), Serial::from(0));

        self.workspaces.update_workspace_model();
    }

    fn toplevel_destroyed(&mut self, toplevel: ToplevelSurface) {
        let id = toplevel.wl_surface().id();
        // let workspace = self.workspaces.get_current_workspace();
        self.workspaces.unmap_window(&id);
        self.workspaces.update_workspace_model();
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // Do not send a configure here, the initial configure
        // of a xdg_surface has to be sent during the commit if
        // the surface is not already configured

        self.unconstrain_popup(&surface);

        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            warn!("Failed to track popup: {}", err);
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
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

        if let Some(touch) = seat.get_touch() {
            if touch.has_grab(serial) {
                let start_data = touch.grab_start_data().unwrap();
                tracing::info!(?start_data);

                // If the client disconnects after requesting a move
                // we can just ignore the request
                let Some(window) = self.window_for_surface(surface.wl_surface()) else {
                    tracing::info!("no window");
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
                    tracing::info!("different surface");
                    return;
                }
                let geometry = window.geometry();
                let loc = self.workspaces.element_location(&window).unwrap();
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

                let grab = TouchResizeSurfaceGrab {
                    start_data,
                    window,
                    edges: edges.into(),
                    initial_window_location,
                    initial_window_size,
                    last_window_size: initial_window_size,
                };

                touch.set_grab(self, grab, serial);
                return;
            }
        }

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
        let loc = self.workspaces.element_location(&window).unwrap();
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

        let grab = PointerResizeSurfaceGrab {
            start_data,
            window,
            edges: edges.into(),
            initial_window_location,
            initial_window_size,
            last_window_size: initial_window_size,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        if let Configure::Toplevel(configure) = configure {
            if let Some(serial) = with_states(&surface, |states| {
                if let Some(data) = states.data_map.get::<RefCell<SurfaceData>>() {
                    if let ResizeState::WaitingForFinalAck(_, serial) = data.borrow().resize_state {
                        return Some(serial);
                    }
                }

                None
            }) {
                // When the resize grab is released the surface
                // resize state will be set to WaitingForFinalAck
                // and the client will receive a configure request
                // without the resize state to inform the client
                // resizing has finished. Here we will wait for
                // the client to acknowledge the end of the
                // resizing. To check if the surface was resizing
                // before sending the configure we need to use
                // the current state as the received acknowledge
                // will no longer have the resize state set
                let is_resizing = with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .current
                        .states
                        .contains(xdg_toplevel::State::Resizing)
                });

                if configure.serial >= serial && is_resizing {
                    with_states(&surface, |states| {
                        let mut data = states
                            .data_map
                            .get::<RefCell<SurfaceData>>()
                            .unwrap()
                            .borrow_mut();
                        if let ResizeState::WaitingForFinalAck(resize_data, _) = data.resize_state {
                            data.resize_state = ResizeState::WaitingForCommit(resize_data);
                        } else {
                            unreachable!()
                        }
                    });
                }
            }

            let _surface_clone = surface.clone();
            use std::borrow::Cow;

            let window = self
                .workspaces
                .spaces_elements()
                .find(|element| element.wl_surface() == Some(Cow::Borrowed(&surface)));
            if let Some(_window) = window {
                use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
                let _is_ssd = configure
                    .state
                    .decoration_mode
                    .map(|mode| mode == Mode::ServerSide)
                    .unwrap_or(false);
                // window.set_ssd(false);
                self.workspaces.update_workspace_model();
            }
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        mut wl_output: Option<wl_output::WlOutput>,
    ) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Fullscreen)
        {
            // NOTE: This is only one part of the solution. We can set the
            // location and configure size here, but the surface should be rendered fullscreen
            // independently from its buffer size
            let wl_surface = surface.wl_surface();

            let output_geometry =
                fullscreen_output_geometry(wl_surface, wl_output.as_ref(), &mut self.workspaces);

            if let Some(geometry) = output_geometry {
                let output = wl_output
                    .as_ref()
                    .and_then(Output::from_resource)
                    .unwrap_or_else(|| self.workspaces.outputs().next().unwrap().clone());
                let client = self.display_handle.get_client(wl_surface.id()).unwrap();
                for output in output.client_outputs(&client) {
                    wl_output = Some(output);
                }
                let window = self
                    .workspaces
                    .spaces_elements()
                    .find(|window| {
                        window
                            .wl_surface()
                            .map(|s| &*s == wl_surface)
                            .unwrap_or(false)
                    })
                    .unwrap();

                surface.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                    state.size = Some(geometry.size);
                    state.fullscreen_output = wl_output;
                });
                output
                    .user_data()
                    .insert_if_missing(FullscreenSurface::default);
                output
                    .user_data()
                    .get::<FullscreenSurface>()
                    .unwrap()
                    .set(window.clone());
                trace!("Fullscreening: {:?}", window);
                unimplemented!("fullscreen on workspaces");
                // let id = surface.wl_surface().id();
                // if let Some(_window_layer_id) = self.workspaces.windows_layer.id() {
                //     if let Some(view) = self.workspaces.get_window_view(&id) {
                //         view.window_layer.set_position(
                //             lay_rs::types::Point { x: 0.0, y: 0.0 },
                //             Some(Transition::ease_in_out_quad(0.4)),
                //         );

                //         self.workspaces.overlay_layer.add_sublayer(view.window_layer.clone());
                //     }
                // }
            }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Fullscreen)
        {
            return;
        }

        let ret = surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
            state.size = None;
            state.fullscreen_output.take()
        });
        if let Some(output) = ret {
            let output = Output::from_resource(&output).unwrap();
            if let Some(fullscreen) = output.user_data().get::<FullscreenSurface>() {
                trace!("Unfullscreening: {:?}", fullscreen.get());
                fullscreen.clear();
                self.backend_data.reset_buffers(&output);

                unimplemented!("unfullscreen");
                // let id = surface.wl_surface().id();
                // if let Some(_window_layer_id) = self.workspaces.windows_layer.id() {
                //     if let Some(view) = self.workspaces.get_window_view(&id) {
                //         let state = view.view_base.get_state();
                //         view.window_layer.set_position(
                //             lay_rs::types::Point {
                //                 x: state.x,
                //                 y: state.y,
                //             },
                //             Some(Transition::ease_out_quad(0.4)),
                //         );

                //         self.workspaces.windows_layer.add_sublayer(view.window_layer.clone());
                //     }
                // }
            }
        }

        surface.send_pending_configure();
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

            let current_element_geometry = self.workspaces.element_geometry(&window).unwrap();
            let id = surface.wl_surface().id();
            if let Some(mut view) = self.workspaces.get_window_view(&id) {
                view.unmaximized_rect = lay_rs::prelude::Rectangle {
                    x: current_element_geometry.loc.x as f32,
                    y: current_element_geometry.loc.y as f32,
                    width: current_element_geometry.size.w as f32,
                    height: current_element_geometry.size.h as f32,
                };
                self.workspaces.set_window_view(&id, view);
            }
            let outputs_for_window = self.workspaces.outputs_for_element(&window);
            let output = outputs_for_window
                .first()
                // The window hasn't been mapped yet, use the primary output instead
                .or_else(|| self.workspaces.outputs().next())
                // Assumes that at least one output exists
                .expect("No outputs found");
            let new_geometry = self.workspaces.output_geometry(output).unwrap();

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
                state.size = Some(new_geometry.size);
            });

            let new_location = new_geometry
                .loc
                .to_f64()
                .to_physical(output.current_scale().fractional_scale());
            self.workspaces.map_element(window, new_geometry.loc, true);

            // unimplemented!("maximize on workspaces");
            let workspace = self.workspaces.get_current_workspace();
            if let Some(_window_layer_id) = workspace.windows_layer.id() {
                if let Some(view) = self.workspaces.get_window_view(&id) {
                    view.window_layer.set_position(
                        lay_rs::types::Point {
                            x: new_location.x as f32,
                            y: new_location.y as f32,
                        },
                        Some(Transition::default()),
                    );
                }
            }
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

        let id = surface.wl_surface().id();
        let window = self.window_for_surface(surface.wl_surface()).unwrap();
        if let Some(view) = self.workspaces.get_window_view(&id) {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.size = Some(
                    (
                        view.unmaximized_rect.width as i32,
                        view.unmaximized_rect.height as i32,
                    )
                        .into(),
                );
            });
            // unimplemented!("unmaximize");
            let workspace = self.workspaces.get_current_workspace();
            if let Some(_window_layer_id) = workspace.windows_layer.id() {
                let scale = self
                    .workspaces
                    .outputs_for_element(&window)
                    .first()
                    .unwrap()
                    .current_scale()
                    .fractional_scale();

                self.workspaces.map_element(
                    window,
                    (
                        view.unmaximized_rect.x as i32,
                        view.unmaximized_rect.y as i32,
                    ),
                    true,
                );

                if let Some(view) = self.workspaces.get_window_view(&id) {
                    view.window_layer.set_position(
                        lay_rs::types::Point {
                            x: view.unmaximized_rect.x * scale as f32,
                            y: view.unmaximized_rect.y * scale as f32,
                        },
                        Some(Transition::default()),
                    );
                }
            }
        }
        surface.send_pending_configure();
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Minimize)
        {
            let window = self.window_for_surface(surface.wl_surface()).unwrap();

            let current_element_geometry = self.workspaces.element_geometry(&window).unwrap();
            let id = surface.wl_surface().id();
            if let Some(mut view) = self.workspaces.get_window_view(&id) {
                view.unmaximized_rect = lay_rs::prelude::Rectangle {
                    x: current_element_geometry.loc.x as f32,
                    y: current_element_geometry.loc.y as f32,
                    width: current_element_geometry.size.w as f32,
                    height: current_element_geometry.size.h as f32,
                };
                self.workspaces.set_window_view(&id, view);
            }

            self.workspaces.minimize_window(&window);
            self.focus_keyboard_on_surface(&id);
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        // surface.send_configure();
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat: Seat<ScreenComposer<BackendData>> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        if let Some(root) = find_popup_root_surface(&kind).ok().and_then(|root| {
            self.workspaces
                .spaces_elements()
                .find(|w| w.wl_surface().map(|s| *s == root).unwrap_or(false))
                .cloned()
                .map(KeyboardFocusTarget::from)
                .or_else(|| {
                    self.workspaces
                        .outputs()
                        .find_map(|o| {
                            let map = layer_map_for_output(o);
                            map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
                                .cloned()
                        })
                        .map(KeyboardFocusTarget::LayerSurface)
                })
        }) {
            let ret = self.popups.grab_popup(root, kind, &seat, serial);

            if let Ok(mut grab) = ret {
                if let Some(keyboard) = seat.get_keyboard() {
                    if keyboard.is_grabbed()
                        && !(keyboard.has_grab(serial)
                            || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    keyboard.set_focus(self, grab.current_grab(), serial);
                    keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
                }
                if let Some(pointer) = seat.get_pointer() {
                    if pointer.is_grabbed()
                        && !(pointer.has_grab(serial)
                            || pointer
                                .has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
                }
            }
        }
    }
}

impl<BackendData: Backend> ScreenComposer<BackendData> {
    pub fn move_request_xdg(
        &mut self,
        surface: &ToplevelSurface,
        seat: &Seat<Self>,
        serial: Serial,
    ) {
        if let Some(touch) = seat.get_touch() {
            if touch.has_grab(serial) {
                let start_data = touch.grab_start_data().unwrap();

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

                let mut initial_window_location =
                    self.workspaces.element_location(&window).unwrap();

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
                    initial_window_location = start_data.location.to_i32_round();
                }

                let grab = TouchMoveSurfaceGrab {
                    start_data,
                    window,
                    initial_window_location,
                };

                touch.set_grab(self, grab, serial);
                return;
            }
        }

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

        let mut initial_window_location = self.workspaces.element_location(&window).unwrap();

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

        let grab = PointerMoveSurfaceGrab {
            start_data,
            window,
            initial_window_location,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.window_for_surface(&root) else {
            return;
        };

        let mut outputs_for_window = self.workspaces.outputs_for_element(&window);
        if outputs_for_window.is_empty() {
            return;
        }

        // Get a union of all outputs' geometries.
        let mut outputs_geo = self
            .workspaces
            .output_geometry(&outputs_for_window.pop().unwrap())
            .unwrap();
        for output in outputs_for_window {
            outputs_geo = outputs_geo.merge(self.workspaces.output_geometry(&output).unwrap());
        }

        let window_geo = self.workspaces.element_geometry(&window).unwrap();

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = outputs_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
