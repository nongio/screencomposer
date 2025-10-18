use std::{convert::TryInto, fs, process::Command, sync::atomic::Ordering};

use crate::{
    config::{
        default_apps,
        shortcuts::{BuiltinAction, ShortcutAction},
        Config,
    },
    focus::PointerFocusTarget,
    shell::FullscreenSurface,
    ScreenComposer,
};

#[cfg(feature = "udev")]
use crate::udev::UdevData;

use smithay::{
    backend::input::{
        self, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent, KeyState,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    desktop::{layer_map_for_output, WindowSurfaceType},
    input::{
        keyboard::{keysyms as xkb, FilterResult, Keysym, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    output::Scale,
    reexports::{
        wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1,
        wayland_server::{protocol::wl_pointer, DisplayHandle, Resource},
    },
    utils::{IsAlive, Logical, Point, Serial, Transform, SERIAL_COUNTER as SCOUNTER},
    wayland::{
        compositor::with_states,
        input_method::InputMethodSeat,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat,
        shell::{
            wlr_layer::{KeyboardInteractivity, Layer as WlrLayer, LayerSurfaceCachedState},
            xdg::XdgToplevelSurfaceData,
        },
    },
};

#[cfg(any(feature = "winit", feature = "x11", feature = "udev"))]
use smithay::backend::input::AbsolutePositionEvent;

#[cfg(any(feature = "winit", feature = "x11"))]
use smithay::output::Output;
use tracing::{debug, error, info, warn};

use crate::state::Backend;
#[cfg(feature = "udev")]
use smithay::{
    backend::{
        input::{
            Device, DeviceCapability, GestureBeginEvent, GestureEndEvent,
            GesturePinchUpdateEvent as _, GestureSwipeUpdateEvent as _, PointerMotionEvent,
            ProximityState, TabletToolButtonEvent, TabletToolEvent, TabletToolProximityEvent,
            TabletToolTipEvent, TabletToolTipState,
        },
        session::Session,
    },
    input::pointer::{
        GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
        GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, RelativeMotionEvent,
    },
    wayland::{
        pointer_constraints::{with_pointer_constraint, PointerConstraint},
        seat::WaylandFocus,
        tablet_manager::{TabletDescriptor, TabletSeatTrait},
    },
};

impl<BackendData: Backend> ScreenComposer<BackendData> {
    pub fn launch_program(&mut self, cmd: String, args: Vec<String>) {
        info!(program = %cmd, args = ?args, "Starting program");

        if let Err(e) = Command::new(&cmd)
            .args(&args)
            .envs(
                self.socket_name
                    .clone()
                    .map(|v| ("WAYLAND_DISPLAY", v))
                    .into_iter()
                    .chain(
                        #[cfg(feature = "xwayland")]
                        self.xdisplay.map(|v| ("DISPLAY", format!(":{}", v))),
                        #[cfg(not(feature = "xwayland"))]
                        None,
                    ),
            )
            .spawn()
        {
            error!(program = %cmd, err = %e, "Failed to start program");
        }
    }
    fn process_common_key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::None => (),

            KeyAction::Quit => {
                info!("Quitting.");
                self.running.store(false, Ordering::SeqCst);
            }

            KeyAction::Run((cmd, args)) => {
                self.launch_program(cmd, args);
            }

            KeyAction::ToggleDecorations => {
                for element in self.workspaces.spaces_elements() {
                    #[allow(irrefutable_let_patterns)]
                    if let Some(toplevel) = element.toplevel() {
                        let mode_changed = toplevel.with_pending_state(|state| {
                            if let Some(current_mode) = state.decoration_mode {
                                let new_mode = if current_mode
                                    == zxdg_toplevel_decoration_v1::Mode::ClientSide
                                {
                                    zxdg_toplevel_decoration_v1::Mode::ServerSide
                                } else {
                                    zxdg_toplevel_decoration_v1::Mode::ClientSide
                                };
                                state.decoration_mode = Some(new_mode);
                                true
                            } else {
                                false
                            }
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
                        if mode_changed && initial_configure_sent {
                            toplevel.send_pending_configure();
                        }
                    }
                }
            }

            KeyAction::SceneSnapshot => {
                let scene = self.layers_engine.scene();

                match scene.serialize_state_pretty() {
                    Ok(json) => {
                        if let Err(err) = fs::write("scene.json", json) {
                            error!(?err, "Failed to write scene snapshot");
                        } else {
                            info!("Scene snapshot saved to scene.json");
                        }
                    }
                    Err(err) => error!(?err, "Failed to serialize scene snapshot"),
                }
            }

            _ => unreachable!(
                "Common key action handler encountered backend specific action {:?}",
                action
            ),
        }
    }

    fn keyboard_key_to_action<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) -> KeyAction {
        let keycode = evt.key_code();
        let state = evt.state();
        debug!(keycode, ?state, "key");
        let serial = SCOUNTER.next_serial();
        let time = Event::time_msec(&evt);
        let mut suppressed_keys = self.suppressed_keys.clone();
        let keyboard = self.seat.get_keyboard().unwrap();

        for layer in self.layer_shell_state.layer_surfaces().rev() {
            let data = with_states(layer.wl_surface(), |states| {
                *states
                    .cached_state
                    .get::<LayerSurfaceCachedState>()
                    .current()
            });
            if data.keyboard_interactivity == KeyboardInteractivity::Exclusive
                && (data.layer == WlrLayer::Top || data.layer == WlrLayer::Overlay)
            {
                let surface = self.workspaces.outputs().find_map(|o| {
                    let map = layer_map_for_output(o);
                    let cloned = map.layers().find(|l| l.layer_surface() == &layer).cloned();
                    cloned
                });
                if let Some(surface) = surface {
                    keyboard.set_focus(self, Some(surface.into()), serial);
                    keyboard.input::<(), _>(self, keycode, state, serial, time, |_, _, _| {
                        FilterResult::Forward
                    });
                    return KeyAction::None;
                };
            }
        }

        let inhibited = self
            .workspaces
            .element_under(self.pointer.current_location())
            .and_then(|(window, _)| {
                let surface = window.wl_surface()?;
                self.seat.keyboard_shortcuts_inhibitor_for_surface(&surface)
            })
            .map(|inhibitor| inhibitor.is_active())
            .unwrap_or(false);

        let action = keyboard
            .input(
                self,
                keycode,
                state,
                serial,
                time,
                |_, modifiers, handle| {
                    let keysym = handle.modified_sym();

                    debug!(
                        ?state,
                        mods = ?modifiers,
                        keysym = ::xkbcommon::xkb::keysym_get_name(keysym),
                        "keysym"
                    );

                    // If the key is pressed and triggered a action
                    // we will not forward the key to the client.
                    // Additionally add the key to the suppressed keys
                    // so that we can decide on a release if the key
                    // should be forwarded to the client or not.
                    if let KeyState::Pressed = state {
                        if !inhibited {
                            let action = process_keyboard_shortcut(*modifiers, keysym);

                            if action.is_some() {
                                suppressed_keys.push(keysym);
                            }

                            action
                                .map(FilterResult::Intercept)
                                .unwrap_or(FilterResult::Forward)
                        } else {
                            FilterResult::Forward
                        }
                    } else {
                        let suppressed = suppressed_keys.contains(&keysym);
                        if suppressed {
                            suppressed_keys.retain(|k| *k != keysym);
                            FilterResult::Intercept(KeyAction::None)
                        } else {
                            FilterResult::Forward
                        }
                    }
                },
            )
            .unwrap_or(KeyAction::None);

        // FIXME this keys handling should be moved into the AppSwitcher
        // by moving the focus to that view
        if KeyState::Released == state && keycode == 56 {
            // App switcher
            if self.workspaces.app_switcher.alive() {
                self.workspaces.app_switcher.hide();
                if let Some(app_id) = self.workspaces.app_switcher.get_current_app_id() {
                    self.focus_app(&app_id);
                    self.workspaces.app_switcher.reset();
                }
            }
        }

        self.suppressed_keys = suppressed_keys;
        action
    }

    fn on_pointer_button<B: InputBackend>(&mut self, evt: B::PointerButtonEvent) {
        let serial = SCOUNTER.next_serial();
        let button = evt.button_code();

        let state = wl_pointer::ButtonState::from(evt.state());

        if !self.workspaces.get_show_all() && wl_pointer::ButtonState::Pressed == state {
            self.focus_window_under_cursor(serial);
        }
        let pointer = self.pointer.clone();
        let button_state = state.try_into().unwrap();
        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);
        match button_state {
            ButtonState::Pressed => {
                self.layers_engine.pointer_button_down();
            }
            ButtonState::Released => {
                self.layers_engine.pointer_button_up();
            }
        }

        // }
    }

    /// Update the focus on the topmost surface under the cursor in the current workspace
    /// The window is raised and the keyboard focus is set to the window.
    fn focus_window_under_cursor(&mut self, serial: Serial) {
        let keyboard = self.seat.get_keyboard().unwrap();
        let input_method = self.seat.input_method();
        // change the keyboard focus unless the pointer or keyboard is grabbed
        // We test for any matching surface type here but always use the root
        // (in case of a window the toplevel) surface for the focus.
        // So for example if a user clicks on a subsurface or popup the toplevel
        // will receive the keyboard focus. Directly assigning the focus to the
        // matching surface leads to issues with clients dismissing popups and
        // subsurface menus (for example firefox-wayland).
        // see here for a discussion about that issue:
        // https://gitlab.freedesktop.org/wayland/wayland/-/issues/294
        if !self.pointer.is_grabbed() && (!keyboard.is_grabbed() || input_method.keyboard_grabbed())
        {
            let output = self
                .workspaces
                .output_under(self.pointer.current_location())
                .next()
                .cloned();
            if let Some(output) = output.as_ref() {
                let output_geo = self.workspaces.output_geometry(output).unwrap();
                if let Some(window) = output
                    .user_data()
                    .get::<FullscreenSurface>()
                    .and_then(|f| f.get())
                {
                    if let Some((_, _)) = window.surface_under::<BackendData>(
                        self.pointer.current_location() - output_geo.loc.to_f64(),
                        WindowSurfaceType::ALL,
                    ) {
                        #[cfg(feature = "xwayland")]
                        if let WindowSurface::X11(surf) = window.underlying_surface() {
                            self.xwm.as_mut().unwrap().raise_window(surf).unwrap();
                        }
                        keyboard.set_focus(self, Some(window.into()), serial);
                        return;
                    }
                }

                // FIXME add back support for WLR layers
                // let layers = layer_map_for_output(output);
                // if let Some(layer) = layers
                //     .layer_under(WlrLayer::Overlay, self.pointer.current_location())
                //     .or_else(|| layers.layer_under(WlrLayer::Top, self.pointer.current_location()))
                // {
                //     if layer.can_receive_keyboard_focus() {
                //         if let Some((_, _)) = layer.surface_under(
                //             self.pointer.current_location()
                //                 - output_geo.loc.to_f64()
                //                 - layers.layer_geometry(layer).unwrap().loc.to_f64(),
                //             WindowSurfaceType::ALL,
                //         ) {
                //             keyboard.set_focus(self, Some(layer.clone().into()), serial);
                //             return;
                //         }
                //     }
                // }
            }
            let scale = output
                .as_ref()
                .map(|o| o.current_scale().fractional_scale())
                .unwrap_or(1.0);
            let position = self.pointer.current_location();
            let scaled_position = position.to_physical(scale);
            if !self
                .workspaces
                .is_cursor_over_dock(scaled_position.x as f32, scaled_position.y as f32)
            {
                let window_under = self
                    .workspaces
                    .element_under(position)
                    .map(|(w, p)| (w.clone(), p));

                if let Some((window, _)) = window_under {
                    if let Some(id) = window.wl_surface().as_ref().map(|s| s.id()) {
                        if let Some(w) = self.workspaces.get_window_for_surface(&id) {
                            if w.is_minimised() {
                                return;
                            }
                            if w.is_fullscreen() {
                                return;
                            }
                            self.workspaces.focus_app_with_window(&id);
                            keyboard.set_focus(self, Some(window.into()), serial);
                            self.workspaces.update_workspace_model();
                        }
                    }

                    #[cfg(feature = "xwayland")]
                    if let WindowSurface::X11(surf) = &window.underlying_surface() {
                        self.xwm.as_mut().unwrap().raise_window(surf).unwrap();
                    }
                }
            }

            // FIXME add back support for WLR layers
            // if let Some(output) = output.as_ref() {
            //     let output_geo = self.workspaces.output_geometry(output).unwrap();
            //     let layers = layer_map_for_output(output);
            //     if let Some(layer) = layers
            //         .layer_under(WlrLayer::Bottom, self.pointer.current_location())
            //         .or_else(|| {
            //             layers.layer_under(WlrLayer::Background, self.pointer.current_location())
            //         })
            //     {
            //         if layer.can_receive_keyboard_focus() {
            //             if let Some((_, _)) = layer.surface_under(
            //                 self.pointer.current_location()
            //                     - output_geo.loc.to_f64()
            //                     - layers.layer_geometry(layer).unwrap().loc.to_f64(),
            //                 WindowSurfaceType::ALL,
            //             ) {
            //                 keyboard.set_focus(self, Some(layer.clone().into()), serial);
            //             }
            //         }
            //     }
            // };
        }
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(PointerFocusTarget<BackendData>, Point<f64, Logical>)> {
        let output = self.workspaces.outputs().find(|o| {
            let geometry = self.workspaces.output_geometry(o).unwrap();
            geometry.contains(pos.to_i32_round())
        })?;
        let output_geo = self.workspaces.output_geometry(output).unwrap();
        let layers = layer_map_for_output(output);
        let scale = output.current_scale().fractional_scale();
        let physical_pos = pos.to_physical(scale);
        let mut under = None;

        // App switcher check
        if self.workspaces.app_switcher.alive() {
            let focus = self.workspaces.app_switcher.as_ref().clone().into();
            return Some((focus, (0.0, 0.0).into()));
        }

        // Workspace selector
        if self.workspaces.get_show_all() {
            let focus = self
                .workspaces
                .workspace_selector_view
                .as_ref()
                .clone()
                .into();

            let layer = self.workspaces.workspace_selector_view.layer.clone();

            if layer.cointains_point((physical_pos.x as f32, physical_pos.y as f32)) {
                let position = self
                    .workspaces
                    .workspace_selector_view
                    .layer
                    .render_position();

                return Some((focus, (position.x as f64, position.y as f64).into()));
            }
        }
        // Window selector check
        if self.workspaces.get_show_all() {
            let workspace = self.workspaces.get_current_workspace();
            let focus = workspace.window_selector_view.as_ref().clone().into();
            let position = workspace.window_selector_view.layer.render_position();

            return Some((focus, (position.x as f64, position.y as f64).into()));
        }
        // FIXME add back support for WLR layers
        // if let Some(window) = output
        //     .user_data()
        //     .get::<FullscreenSurface>()
        //     .and_then(|f| f.get())
        // {
        //     under = Some((window.into(), output_geo.loc));
        // } else if let Some(layer) = layers
        //     .layer_under(WlrLayer::Overlay, pos)
        //     .or_else(|| layers.layer_under(WlrLayer::Top, pos))
        // {
        //     // WLR Layer
        //     let layer_loc = layers.layer_geometry(layer).unwrap().loc;
        //     under = Some((layer.clone().into(), output_geo.loc + layer_loc))
        // } else

        if self
            .workspaces
            .is_cursor_over_dock(physical_pos.x as f32, physical_pos.y as f32)
        {
            // Dock
            under = Some((self.workspaces.dock.as_ref().clone().into(), (0, 0).into()));
        } else if let Some((focus, location)) =
            self.workspaces
                .element_under(pos)
                .and_then(|(window, loc)| {
                    if let Some(id) = window.wl_surface().as_ref().map(|s| s.id()) {
                        if let Some(w) = self.workspaces.get_window_for_surface(&id) {
                            if w.is_minimised() {
                                return None;
                            }
                        }
                    }
                    window
                        .surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
                        .map(|(surface, surf_loc)| (surface, surf_loc + loc))
                })
        {
            under = Some((focus, location));
        } else if let Some(layer) = layers
            .layer_under(WlrLayer::Bottom, pos)
            .or_else(|| layers.layer_under(WlrLayer::Background, pos))
        {
            let layer_loc = layers.layer_geometry(layer).unwrap().loc;
            under = Some((layer.clone().into(), output_geo.loc + layer_loc));
        };
        under.map(|(s, l)| (s, l.to_f64()))
    }

    fn on_pointer_axis<B: InputBackend>(&mut self, evt: B::PointerAxisEvent) {
        let horizontal_amount = evt.amount(input::Axis::Horizontal).unwrap_or_else(|| {
            evt.amount_v120(input::Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.
        });
        let vertical_amount = evt
            .amount(input::Axis::Vertical)
            .unwrap_or_else(|| evt.amount_v120(input::Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
        let horizontal_amount_discrete = evt.amount_v120(input::Axis::Horizontal);
        let vertical_amount_discrete = evt.amount_v120(input::Axis::Vertical);

        {
            let mut frame = AxisFrame::new(evt.time_msec()).source(evt.source());
            if horizontal_amount != 0.0 {
                frame = frame
                    .relative_direction(Axis::Horizontal, evt.relative_direction(Axis::Horizontal));
                frame = frame.value(Axis::Horizontal, horizontal_amount);
                if let Some(discrete) = horizontal_amount_discrete {
                    frame = frame.v120(Axis::Horizontal, discrete as i32);
                }
            }
            if vertical_amount != 0.0 {
                frame = frame
                    .relative_direction(Axis::Vertical, evt.relative_direction(Axis::Vertical));
                frame = frame.value(Axis::Vertical, vertical_amount);
                if let Some(discrete) = vertical_amount_discrete {
                    frame = frame.v120(Axis::Vertical, discrete as i32);
                }
            }
            if evt.source() == AxisSource::Finger {
                if evt.amount(Axis::Horizontal) == Some(0.0) {
                    frame = frame.stop(Axis::Horizontal);
                }
                if evt.amount(Axis::Vertical) == Some(0.0) {
                    frame = frame.stop(Axis::Vertical);
                }
            }
            let pointer = self.pointer.clone();
            pointer.axis(self, frame);
            pointer.frame(self);
        }
    }
}

#[cfg(any(feature = "winit", feature = "x11"))]
impl<Backend: crate::state::Backend> ScreenComposer<Backend> {
    pub fn process_input_event_windowed<B: InputBackend>(
        &mut self,
        event: InputEvent<B>,
        output_name: &str,
    ) {
        match event {
            InputEvent::Keyboard { event } => match self.keyboard_key_to_action::<B>(event) {
                KeyAction::ScaleUp => {
                    let output = self
                        .workspaces
                        .outputs()
                        .find(|o| o.name() == output_name)
                        .unwrap()
                        .clone();

                    let current_scale = output.current_scale().fractional_scale();
                    let new_scale = current_scale + 0.25;
                    output.change_current_state(
                        None,
                        None,
                        Some(Scale::Fractional(new_scale)),
                        None,
                    );
                    let current_location = self.pointer.current_location();

                    crate::shell::fixup_positions(&mut self.workspaces, current_location);
                    self.backend_data.reset_buffers(&output);
                }

                KeyAction::ScaleDown => {
                    let output = self
                        .workspaces
                        .outputs()
                        .find(|o| o.name() == output_name)
                        .unwrap()
                        .clone();

                    let current_scale = output.current_scale().fractional_scale();
                    let new_scale = f64::max(1.0, current_scale - 0.25);
                    output.change_current_state(
                        None,
                        None,
                        Some(Scale::Fractional(new_scale)),
                        None,
                    );
                    let current_location = self.pointer.current_location();
                    crate::shell::fixup_positions(&mut self.workspaces, current_location);
                    self.backend_data.reset_buffers(&output);
                }

                KeyAction::RotateOutput => {
                    let output = self
                        .workspaces
                        .outputs()
                        .find(|o| o.name() == output_name)
                        .unwrap()
                        .clone();

                    let current_transform = output.current_transform();
                    let new_transform = match current_transform {
                        Transform::Normal => Transform::_90,
                        Transform::_90 => Transform::_180,
                        Transform::_180 => Transform::_270,
                        Transform::_270 => Transform::Normal,
                        _ => Transform::Normal,
                    };
                    output.change_current_state(None, Some(new_transform), None, None);
                    let current_location = self.pointer.current_location();

                    crate::shell::fixup_positions(&mut self.workspaces, current_location);
                    self.backend_data.reset_buffers(&output);
                }
                KeyAction::ApplicationSwitchNext => {
                    self.workspaces.app_switcher.next();
                }
                KeyAction::ApplicationSwitchPrev => {
                    self.workspaces.app_switcher.previous();
                }
                KeyAction::ApplicationSwitchQuit => {
                    self.workspaces.quit_appswitcher_app();
                }
                KeyAction::CloseWindow => {
                    self.close_focused_window();
                }
                KeyAction::ApplicationSwitchNextWindow => {
                    self.workspaces.raise_next_app_window();
                }
                KeyAction::ExposeShowDesktop => {
                    if self.workspaces.get_show_desktop() {
                        self.workspaces.expose_show_desktop(-1.0, true);
                    } else {
                        self.workspaces.expose_show_desktop(1.0, true);
                    }
                }
                KeyAction::ExposeShowAll => {
                    if self.workspaces.get_show_all() {
                        self.workspaces.expose_show_all(-1.0, true);
                    } else {
                        self.workspaces.expose_show_all(1.0, true);
                    }
                }
                KeyAction::WorkspaceNum(n) => {
                    self.workspaces.set_current_workspace_index(n, None);
                }

                action => match action {
                    KeyAction::None
                    | KeyAction::Quit
                    | KeyAction::Run(_)
                    | KeyAction::ToggleDecorations
                    | KeyAction::SceneSnapshot => self.process_common_key_action(action),

                    _ => tracing::warn!(
                        ?action,
                        output_name,
                        "Key action unsupported on on output backend.",
                    ),
                },
            },

            InputEvent::PointerMotionAbsolute { event } => {
                let output = self
                    .workspaces
                    .outputs()
                    .find(|o| o.name() == output_name)
                    .unwrap()
                    .clone();
                self.on_pointer_move_absolute_windowed::<B>(event, &output)
            }
            InputEvent::PointerButton { event } => self.on_pointer_button::<B>(event),
            InputEvent::PointerAxis { event } => self.on_pointer_axis::<B>(event),
            _ => (), // other events are not handled in anvil (yet)
        }
    }

    fn on_pointer_move_absolute_windowed<B: InputBackend>(
        &mut self,
        evt: B::PointerMotionAbsoluteEvent,
        output: &Output,
    ) {
        let output_geo = self.workspaces.output_geometry(output).unwrap();

        let pos = evt.position_transformed(output_geo.size) + output_geo.loc.to_f64();
        let serial = SCOUNTER.next_serial();

        let under = self.surface_under(pos);
        let pointer = self.pointer.clone();

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);

        let scale = output.current_scale().fractional_scale();
        let pos = pos.to_physical(scale);
        self.layers_engine
            .pointer_move(&(pos.x as f32, pos.y as f32).into(), None);
    }

    pub fn release_all_keys(&mut self) {
        let keyboard = self.seat.get_keyboard().unwrap();
        for keycode in keyboard.pressed_keys() {
            keyboard.input(
                self,
                keycode.raw(),
                KeyState::Released,
                SCOUNTER.next_serial(),
                0,
                |_, _, _| FilterResult::Forward::<bool>,
            );
        }
    }
}

#[cfg(feature = "udev")]
impl ScreenComposer<UdevData> {
    pub fn process_input_event<B: InputBackend>(
        &mut self,
        dh: &DisplayHandle,
        event: InputEvent<B>,
    ) {
        match event {
            InputEvent::Keyboard { event, .. } => match self.keyboard_key_to_action::<B>(event) {
                #[cfg(feature = "udev")]
                KeyAction::VtSwitch(vt) => {
                    info!(to = vt, "Trying to switch vt");
                    if let Err(err) = self.backend_data.session.change_vt(vt) {
                        error!(vt, "Error switching vt: {}", err);
                    }
                }
                KeyAction::Screen(num) => {
                    let geometry = self
                        .workspaces
                        .outputs()
                        .nth(num)
                        .map(|o| self.workspaces.output_geometry(o).unwrap());

                    if let Some(geometry) = geometry {
                        let x = geometry.loc.x as f64 + geometry.size.w as f64 / 2.0;
                        let y = geometry.size.h as f64 / 2.0;
                        let location = (x, y).into();
                        let pointer = self.pointer.clone();
                        let under = self.surface_under(location);
                        pointer.motion(
                            self,
                            under,
                            &MotionEvent {
                                location,
                                serial: SCOUNTER.next_serial(),
                                time: 0,
                            },
                        );
                        pointer.frame(self);
                    }
                }
                KeyAction::ScaleUp => {
                    let pos = self.pointer.current_location().to_i32_round();
                    let output = self
                        .workspaces
                        .outputs()
                        .find(|o| self.workspaces.output_geometry(o).unwrap().contains(pos))
                        .cloned();

                    if let Some(output) = output {
                        let (output_location, scale) = (
                            self.workspaces.output_geometry(&output).unwrap().loc,
                            output.current_scale().fractional_scale(),
                        );
                        let new_scale = scale + 0.25;
                        output.change_current_state(
                            None,
                            None,
                            Some(Scale::Fractional(new_scale)),
                            None,
                        );

                        let rescale = scale / new_scale;
                        let output_location = output_location.to_f64();
                        let mut pointer_output_location =
                            self.pointer.current_location() - output_location;
                        pointer_output_location.x *= rescale;
                        pointer_output_location.y *= rescale;
                        let pointer_location = output_location + pointer_output_location;
                        crate::shell::fixup_positions(&mut self.workspaces, pointer_location);
                        let pointer = self.pointer.clone();
                        let under = self.surface_under(pointer_location);
                        pointer.motion(
                            self,
                            under,
                            &MotionEvent {
                                location: pointer_location,
                                serial: SCOUNTER.next_serial(),
                                time: 0,
                            },
                        );
                        pointer.frame(self);
                        self.backend_data.reset_buffers(&output);
                    }
                }
                KeyAction::ScaleDown => {
                    let pos = self.pointer.current_location().to_i32_round();
                    let output = self
                        .workspaces
                        .outputs()
                        .find(|o| self.workspaces.output_geometry(o).unwrap().contains(pos))
                        .cloned();

                    if let Some(output) = output {
                        let (output_location, scale) = (
                            self.workspaces.output_geometry(&output).unwrap().loc,
                            output.current_scale().fractional_scale(),
                        );
                        let new_scale = f64::max(1.0, scale - 0.25);
                        output.change_current_state(
                            None,
                            None,
                            Some(Scale::Fractional(new_scale)),
                            None,
                        );

                        let rescale = scale / new_scale;
                        let output_location = output_location.to_f64();
                        let mut pointer_output_location =
                            self.pointer.current_location() - output_location;
                        pointer_output_location.x *= rescale;
                        pointer_output_location.y *= rescale;
                        let pointer_location = output_location + pointer_output_location;

                        crate::shell::fixup_positions(&mut self.workspaces, pointer_location);
                        let pointer = self.pointer.clone();
                        let under = self.surface_under(pointer_location);
                        pointer.motion(
                            self,
                            under,
                            &MotionEvent {
                                location: pointer_location,
                                serial: SCOUNTER.next_serial(),
                                time: 0,
                            },
                        );
                        pointer.frame(self);
                        self.backend_data.reset_buffers(&output);
                    }
                }
                KeyAction::RotateOutput => {
                    let pos = self.pointer.current_location().to_i32_round();
                    let output = self
                        .workspaces
                        .outputs()
                        .find(|o| self.workspaces.output_geometry(o).unwrap().contains(pos))
                        .cloned();

                    if let Some(output) = output {
                        let current_transform = output.current_transform();
                        let new_transform = match current_transform {
                            Transform::Normal => Transform::_90,
                            Transform::_90 => Transform::_180,
                            Transform::_180 => Transform::_270,
                            Transform::_270 => Transform::Normal,
                            _ => Transform::Normal,
                        };
                        output.change_current_state(None, Some(new_transform), None, None);
                        let current_location = self.pointer.current_location();
                        crate::shell::fixup_positions(&mut self.workspaces, current_location);
                        self.backend_data.reset_buffers(&output);
                    }
                }
                KeyAction::ApplicationSwitchNext => {
                    self.workspaces.app_switcher.next();
                }
                KeyAction::ApplicationSwitchPrev => {
                    self.workspaces.app_switcher.previous();
                }
                KeyAction::ApplicationSwitchNextWindow => {
                    self.raise_next_app_window();
                }
                KeyAction::ApplicationSwitchQuit => {
                    self.quit_appswitcher_app();
                }
                KeyAction::CloseWindow => {
                    self.close_focused_window();
                }
                KeyAction::ExposeShowDesktop => {
                    if self.workspaces.get_show_desktop() {
                        self.workspaces.expose_show_desktop(-1.0, true);
                    } else {
                        self.workspaces.expose_show_desktop(1.0, true);
                    }
                }
                KeyAction::ExposeShowAll => {
                    if self.workspaces.get_show_all() {
                        self.workspaces.expose_show_all(-1.0, true);
                    } else {
                        self.workspaces.expose_show_all(1.0, true);
                    }
                }
                action => match action {
                    KeyAction::None
                    | KeyAction::Quit
                    | KeyAction::Run(_)
                    | KeyAction::ToggleDecorations
                    | KeyAction::SceneSnapshot => self.process_common_key_action(action),

                    _ => unreachable!(),
                },
            },
            InputEvent::PointerMotion { event, .. } => self.on_pointer_move::<B>(dh, event),
            InputEvent::PointerMotionAbsolute { event, .. } => {
                self.on_pointer_move_absolute::<B>(dh, event)
            }
            InputEvent::PointerButton { event, .. } => self.on_pointer_button::<B>(event),
            InputEvent::PointerAxis { event, .. } => self.on_pointer_axis::<B>(event),
            InputEvent::TabletToolAxis { event, .. } => self.on_tablet_tool_axis::<B>(event),
            InputEvent::TabletToolProximity { event, .. } => {
                self.on_tablet_tool_proximity::<B>(dh, event)
            }
            InputEvent::TabletToolTip { event, .. } => self.on_tablet_tool_tip::<B>(event),
            InputEvent::TabletToolButton { event, .. } => self.on_tablet_button::<B>(event),
            InputEvent::GestureSwipeBegin { event, .. } => self.on_gesture_swipe_begin::<B>(event),
            InputEvent::GestureSwipeUpdate { event, .. } => {
                self.on_gesture_swipe_update::<B>(event)
            }
            InputEvent::GestureSwipeEnd { event, .. } => self.on_gesture_swipe_end::<B>(event),
            InputEvent::GesturePinchBegin { event, .. } => self.on_gesture_pinch_begin::<B>(event),
            InputEvent::GesturePinchUpdate { event, .. } => {
                self.on_gesture_pinch_update::<B>(event)
            }
            InputEvent::GesturePinchEnd { event, .. } => self.on_gesture_pinch_end::<B>(event),
            InputEvent::GestureHoldBegin { event, .. } => self.on_gesture_hold_begin::<B>(event),
            InputEvent::GestureHoldEnd { event, .. } => self.on_gesture_hold_end::<B>(event),
            InputEvent::DeviceAdded { device } => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    self.seat
                        .tablet_seat()
                        .add_tablet::<Self>(dh, &TabletDescriptor::from(&device));
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    let tablet_seat = self.seat.tablet_seat();

                    tablet_seat.remove_tablet(&TabletDescriptor::from(&device));

                    // If there are no tablets in seat we can remove all tools
                    if tablet_seat.count_tablets() == 0 {
                        tablet_seat.clear_tools();
                    }
                }
            }
            _ => {
                // other events are not handled in anvil (yet)
            }
        }
    }

    fn on_pointer_move<B: InputBackend>(
        &mut self,
        _dh: &DisplayHandle,
        evt: B::PointerMotionEvent,
    ) {
        let mut pointer_location = self.pointer.current_location();
        let serial = SCOUNTER.next_serial();

        let pointer = self.pointer.clone();
        let under = self.surface_under(pointer_location);

        let mut pointer_locked = false;
        let mut pointer_confined = false;
        let mut confine_region = None;
        if let Some((surface, surface_loc)) = under
            .as_ref()
            .and_then(|(target, l)| Some((target.wl_surface()?, l)))
        {
            with_pointer_constraint(&surface, &pointer, |constraint| match constraint {
                Some(constraint) if constraint.is_active() => {
                    // Constraint does not apply if not within region
                    if !constraint.region().is_none_or(|x| {
                        x.contains((pointer_location - *surface_loc).to_i32_round())
                    }) {
                        return;
                    }
                    match &*constraint {
                        PointerConstraint::Locked(_locked) => {
                            pointer_locked = true;
                        }
                        PointerConstraint::Confined(confine) => {
                            pointer_confined = true;
                            confine_region = confine.region().cloned();
                        }
                    }
                }
                _ => {}
            });
        }

        pointer.relative_motion(
            self,
            under.clone(),
            &RelativeMotionEvent {
                delta: evt.delta(),
                delta_unaccel: evt.delta_unaccel(),
                utime: evt.time(),
            },
        );

        // If pointer is locked, only emit relative motion
        if pointer_locked {
            pointer.frame(self);
            return;
        }

        pointer_location += evt.delta();

        // clamp to screen limits
        // this event is never generated by winit
        pointer_location = self.clamp_coords(pointer_location);

        let new_under = self.surface_under(pointer_location);

        // If confined, don't move pointer if it would go outside surface or region
        if pointer_confined {
            if let Some((surface, surface_loc)) = &under {
                if new_under.as_ref().and_then(|(under, _)| under.wl_surface())
                    != surface.wl_surface()
                {
                    pointer.frame(self);
                    return;
                }
                if let Some(region) = confine_region {
                    if !region.contains((pointer_location - *surface_loc).to_i32_round()) {
                        pointer.frame(self);
                        return;
                    }
                }
            }
        }

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pointer_location,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);

        let scale = Config::with(|c| c.screen_scale);
        let pos = pointer_location.to_physical(scale);

        self.layers_engine
            .pointer_move(&(pos.x as f32, pos.y as f32).into(), None);

        // If pointer is now in a constraint region, activate it
        // TODO Anywhere else pointer is moved needs to do this
        if let Some((under, surface_location)) =
            new_under.and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc)))
        {
            with_pointer_constraint(&under, &pointer, |constraint| match constraint {
                Some(constraint) if !constraint.is_active() => {
                    let point = (pointer_location - surface_location).to_i32_round();
                    if constraint
                        .region()
                        .is_none_or(|region| region.contains(point))
                    {
                        constraint.activate();
                    }
                }
                _ => {}
            });
        }
    }

    fn on_pointer_move_absolute<B: InputBackend>(
        &mut self,
        _dh: &DisplayHandle,
        evt: B::PointerMotionAbsoluteEvent,
    ) {
        let serial = SCOUNTER.next_serial();
        let max_x = self.workspaces.outputs().fold(0, |acc, o| {
            acc + self.workspaces.output_geometry(o).unwrap().size.w
        });

        let max_h_output = self
            .workspaces
            .outputs()
            .max_by_key(|o| self.workspaces.output_geometry(o).unwrap().size.h)
            .unwrap()
            .clone();

        let max_y = self
            .workspaces
            .output_geometry(&max_h_output)
            .unwrap()
            .size
            .h;

        let mut pointer_location = (evt.x_transformed(max_x), evt.y_transformed(max_y)).into();

        // clamp to screen limits
        pointer_location = self.clamp_coords(pointer_location);

        let pointer = self.pointer.clone();
        let under = self.surface_under(pointer_location);
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pointer_location,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);
    }

    fn on_tablet_tool_axis<B: InputBackend>(&mut self, evt: B::TabletToolAxisEvent) {
        let tablet_seat = self.seat.tablet_seat();

        let output_geometry = self
            .workspaces
            .outputs()
            .next()
            .map(|o| self.workspaces.output_geometry(o).unwrap());

        if let Some(rect) = output_geometry {
            let pointer_location = evt.position_transformed(rect.size) + rect.loc.to_f64();

            let pointer = self.pointer.clone();
            let under = self.surface_under(pointer_location);
            let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&evt.device()));
            let tool = tablet_seat.get_tool(&evt.tool());

            pointer.motion(
                self,
                under.clone(),
                &MotionEvent {
                    location: pointer_location,
                    serial: SCOUNTER.next_serial(),
                    time: 0,
                },
            );

            if let (Some(tablet), Some(tool)) = (tablet, tool) {
                if evt.pressure_has_changed() {
                    tool.pressure(evt.pressure());
                }
                if evt.distance_has_changed() {
                    tool.distance(evt.distance());
                }
                if evt.tilt_has_changed() {
                    tool.tilt(evt.tilt());
                }
                if evt.slider_has_changed() {
                    tool.slider_position(evt.slider_position());
                }
                if evt.rotation_has_changed() {
                    tool.rotation(evt.rotation());
                }
                if evt.wheel_has_changed() {
                    tool.wheel(evt.wheel_delta(), evt.wheel_delta_discrete());
                }

                tool.motion(
                    pointer_location,
                    under.and_then(|(f, loc)| f.wl_surface().map(|s| (s.into_owned(), loc))),
                    &tablet,
                    SCOUNTER.next_serial(),
                    evt.time_msec(),
                );
            }

            pointer.frame(self);
        }
    }

    fn on_tablet_tool_proximity<B: InputBackend>(
        &mut self,
        _dh: &DisplayHandle,
        evt: B::TabletToolProximityEvent,
    ) {
        let tablet_seat = self.seat.tablet_seat();

        let output_geometry = self
            .workspaces
            .outputs()
            .next()
            .map(|o| self.workspaces.output_geometry(o).unwrap());

        if let Some(rect) = output_geometry {
            let tool = evt.tool();
            // FIXME: tablet handling on proximity
            // tablet_seat.add_tool::<Self>(dh, &tool);

            let pointer_location = evt.position_transformed(rect.size) + rect.loc.to_f64();

            let pointer = self.pointer.clone();
            let under = self.surface_under(pointer_location);
            let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&evt.device()));
            let tool = tablet_seat.get_tool(&tool);

            pointer.motion(
                self,
                under.clone(),
                &MotionEvent {
                    location: pointer_location,
                    serial: SCOUNTER.next_serial(),
                    time: 0,
                },
            );
            pointer.frame(self);

            if let (Some(under), Some(tablet), Some(tool)) = (
                under.and_then(|(f, loc)| f.wl_surface().map(|s| (s.into_owned(), loc))),
                tablet,
                tool,
            ) {
                match evt.state() {
                    ProximityState::In => tool.proximity_in(
                        pointer_location,
                        under,
                        &tablet,
                        SCOUNTER.next_serial(),
                        evt.time_msec(),
                    ),
                    ProximityState::Out => tool.proximity_out(evt.time_msec()),
                }
            }
        }
    }

    fn on_tablet_tool_tip<B: InputBackend>(&mut self, evt: B::TabletToolTipEvent) {
        let tool = self.seat.tablet_seat().get_tool(&evt.tool());

        if let Some(tool) = tool {
            match evt.tip_state() {
                TabletToolTipState::Down => {
                    let serial = SCOUNTER.next_serial();
                    tool.tip_down(serial, evt.time_msec());

                    self.focus_window_under_cursor(serial);
                }
                TabletToolTipState::Up => {
                    tool.tip_up(evt.time_msec());
                }
            }
        }
    }

    fn on_tablet_button<B: InputBackend>(&mut self, evt: B::TabletToolButtonEvent) {
        let tool = self.seat.tablet_seat().get_tool(&evt.tool());

        if let Some(tool) = tool {
            tool.button(
                evt.button(),
                evt.button_state(),
                SCOUNTER.next_serial(),
                evt.time_msec(),
            );
        }
    }

    fn on_gesture_swipe_begin<B: InputBackend>(&mut self, evt: B::GestureSwipeBeginEvent) {
        let serial = SCOUNTER.next_serial();
        let pointer = self.pointer.clone();
        // tracing::error!("on_gesture_swipe_begin: {:?}", self.swipe_gesture);
        if evt.fingers() == 3 && !self.is_pinching {
            self.is_swiping = true;
        }
        // self.background_view.set_debug_text(format!("on_gesture_swipe_begin: {:?}", self.swipe_gesture));

        pointer.gesture_swipe_begin(
            self,
            &GestureSwipeBeginEvent {
                serial,
                time: evt.time_msec(),
                fingers: evt.fingers(),
            },
        );
    }

    fn on_gesture_swipe_update<B: InputBackend>(&mut self, evt: B::GestureSwipeUpdateEvent) {
        let pointer = self.pointer.clone();
        let multiplier = 800.0;
        let delta = evt.delta_y() as f32 / multiplier;

        if self.is_swiping {
            self.workspaces.expose_show_all(-delta, false);
        }
        pointer.gesture_swipe_update(
            self,
            &GestureSwipeUpdateEvent {
                time: evt.time_msec(),
                delta: evt.delta(),
            },
        );
    }

    fn on_gesture_swipe_end<B: InputBackend>(&mut self, evt: B::GestureSwipeEndEvent) {
        let serial = SCOUNTER.next_serial();
        let pointer = self.pointer.clone();

        if self.is_swiping {
            self.workspaces.expose_show_all(0.0, true);
            self.is_swiping = false;
        }
        pointer.gesture_swipe_end(
            self,
            &GestureSwipeEndEvent {
                serial,
                time: evt.time_msec(),
                cancelled: evt.cancelled(),
            },
        );
    }

    fn on_gesture_pinch_begin<B: InputBackend>(&mut self, evt: B::GesturePinchBeginEvent) {
        let serial = SCOUNTER.next_serial();
        let pointer = self.pointer.clone();

        if evt.fingers() == 4 && !self.is_swiping {
            self.is_pinching = true;
        }

        pointer.gesture_pinch_begin(
            self,
            &GesturePinchBeginEvent {
                serial,
                time: evt.time_msec(),
                fingers: evt.fingers(),
            },
        );
    }

    fn on_gesture_pinch_update<B: InputBackend>(&mut self, evt: B::GesturePinchUpdateEvent) {
        let pointer = self.pointer.clone();
        let multiplier = 1.1;
        let delta = evt.scale() as f32 * multiplier;

        // if !self.show_desktop {
        //     delta -= 1.0;
        // }

        // self.pinch_gesture = lay_rs::types::Point {
        //     x: delta,//(self.pinch_gesture.x - delta),
        //     y: delta,//(self.pinch_gesture.y - delta),
        // };
        if self.is_pinching {
            // self.background_view.set_debug_text(format!("on_gesture_pinch_update: {:?}", delta));
            self.workspaces.expose_show_desktop(delta, false);
        }
        pointer.gesture_pinch_update(
            self,
            &GesturePinchUpdateEvent {
                time: evt.time_msec(),
                delta: evt.delta(),
                scale: evt.scale(),
                rotation: evt.rotation(),
            },
        );
    }

    fn on_gesture_pinch_end<B: InputBackend>(&mut self, evt: B::GesturePinchEndEvent) {
        let serial = SCOUNTER.next_serial();
        let pointer = self.pointer.clone();

        // self.background_view.set_debug_text(format!("on_gesture_pinch_end"));
        if self.is_pinching {
            self.workspaces.expose_show_desktop(0.0, true);
        }
        pointer.gesture_pinch_end(
            self,
            &GesturePinchEndEvent {
                serial,
                time: evt.time_msec(),
                cancelled: evt.cancelled(),
            },
        );
    }

    fn on_gesture_hold_begin<B: InputBackend>(&mut self, evt: B::GestureHoldBeginEvent) {
        let serial = SCOUNTER.next_serial();
        let pointer = self.pointer.clone();
        pointer.gesture_hold_begin(
            self,
            &GestureHoldBeginEvent {
                serial,
                time: evt.time_msec(),
                fingers: evt.fingers(),
            },
        );
    }

    fn on_gesture_hold_end<B: InputBackend>(&mut self, evt: B::GestureHoldEndEvent) {
        let serial = SCOUNTER.next_serial();
        let pointer = self.pointer.clone();
        pointer.gesture_hold_end(
            self,
            &GestureHoldEndEvent {
                serial,
                time: evt.time_msec(),
                cancelled: evt.cancelled(),
            },
        );
    }

    fn clamp_coords(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        if self.workspaces.outputs().next().is_none() {
            return pos;
        }

        let (pos_x, pos_y) = pos.into();
        let max_x = self.workspaces.outputs().fold(0, |acc, o| {
            acc + self.workspaces.output_geometry(o).unwrap().size.w
        });
        let clamped_x = pos_x.clamp(0.0, max_x as f64);
        let max_y = self
            .workspaces
            .outputs()
            .find(|o| {
                let geo = self.workspaces.output_geometry(o).unwrap();
                geo.contains((clamped_x as i32, 0))
            })
            .map(|o| self.workspaces.output_geometry(o).unwrap().size.h);

        if let Some(max_y) = max_y {
            let clamped_y = pos_y.clamp(0.0, max_y as f64);
            (clamped_x, clamped_y).into()
        } else {
            (clamped_x, pos_y).into()
        }
    }
}

/// Possible results of a keyboard action
#[allow(dead_code)]
#[derive(Debug)]
enum KeyAction {
    /// Quit the compositor
    Quit,
    /// Trigger a vt-switch
    VtSwitch(i32),
    /// run a command
    Run((String, Vec<String>)),
    /// Switch the current screen
    Screen(usize),
    ScaleUp,
    ScaleDown,
    RotateOutput,
    ToggleDecorations,
    ApplicationSwitchNext,
    ApplicationSwitchPrev,
    ApplicationSwitchQuit,
    CloseWindow,
    ApplicationSwitchNextWindow,
    ExposeShowDesktop,
    ExposeShowAll,
    WorkspaceNum(usize),
    SceneSnapshot,
    /// Do nothing more
    None,
}

fn process_keyboard_shortcut(modifiers: ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    if (xkb::KEY_XF86Switch_VT_1..=xkb::KEY_XF86Switch_VT_12).contains(&keysym.raw()) {
        return Some(KeyAction::VtSwitch(
            (keysym.raw() - xkb::KEY_XF86Switch_VT_1 + 1) as i32,
        ));
    }

    Config::with(|config| {
        config
            .shortcut_bindings()
            .iter()
            .find(|binding| binding.trigger.matches(&modifiers, keysym))
            .and_then(|binding| resolve_shortcut_action(config, &binding.action))
    })
}

fn resolve_shortcut_action(config: &Config, action: &ShortcutAction) -> Option<KeyAction> {
    match action {
        ShortcutAction::Builtin(builtin) => match builtin {
            BuiltinAction::Quit => Some(KeyAction::Quit),
            BuiltinAction::Screen { index } => Some(KeyAction::Screen(*index)),
            BuiltinAction::ScaleUp => Some(KeyAction::ScaleUp),
            BuiltinAction::ScaleDown => Some(KeyAction::ScaleDown),
            BuiltinAction::RotateOutput => Some(KeyAction::RotateOutput),
            BuiltinAction::ToggleDecorations => Some(KeyAction::ToggleDecorations),
            BuiltinAction::ApplicationSwitchNext => Some(KeyAction::ApplicationSwitchNext),
            BuiltinAction::ApplicationSwitchPrev => Some(KeyAction::ApplicationSwitchPrev),
            BuiltinAction::ApplicationSwitchQuit => Some(KeyAction::ApplicationSwitchQuit),
            BuiltinAction::CloseWindow => Some(KeyAction::CloseWindow),
            BuiltinAction::ApplicationSwitchNextWindow => {
                Some(KeyAction::ApplicationSwitchNextWindow)
            }
            BuiltinAction::ExposeShowDesktop => Some(KeyAction::ExposeShowDesktop),
            BuiltinAction::ExposeShowAll => Some(KeyAction::ExposeShowAll),
            BuiltinAction::WorkspaceNum { index } => Some(KeyAction::WorkspaceNum(*index)),
            BuiltinAction::SceneSnapshot => Some(KeyAction::SceneSnapshot),
            BuiltinAction::RunTerminal => {
                Some(KeyAction::Run((config.terminal_bin.clone(), Vec::new())))
            }
            BuiltinAction::RunFileManager => Some(KeyAction::Run((
                config.file_manager_bin.clone(),
                Vec::new(),
            ))),
            BuiltinAction::RunBrowser => Some(KeyAction::Run((
                config.browser_bin.clone(),
                config.browser_args.clone(),
            ))),
            BuiltinAction::RunLayersDebug => {
                Some(KeyAction::Run(("layers_debug".to_string(), vec![])))
            }
        },
        ShortcutAction::RunCommand(run) => {
            Some(KeyAction::Run((run.cmd.clone(), run.args.clone())))
        }
        ShortcutAction::OpenDefaultApp { role, fallback } => {
            match default_apps::resolve(role, fallback.as_deref(), config) {
                Some((cmd, args)) => Some(KeyAction::Run((cmd, args))),
                None => {
                    warn!(
                        role,
                        "no default application found for role; ignoring shortcut action"
                    );
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::shortcuts::RunCommandConfig;

    #[test]
    fn builtin_quit_maps_to_key_action() {
        let config = Config::default();
        let action = ShortcutAction::Builtin(BuiltinAction::Quit);
        assert!(matches!(
            resolve_shortcut_action(&config, &action),
            Some(KeyAction::Quit)
        ));
    }

    #[test]
    fn run_command_maps_to_key_action() {
        let config = Config::default();
        let action = ShortcutAction::RunCommand(RunCommandConfig {
            cmd: "echo".into(),
            args: vec!["hello".into()],
        });
        let result = resolve_shortcut_action(&config, &action).expect("command resolved");
        match result {
            KeyAction::Run((cmd, args)) => {
                assert_eq!(cmd, "echo");
                assert_eq!(args, vec!["hello".to_string()]);
            }
            other => panic!("unexpected key action: {:?}", other),
        }
    }

    #[test]
    fn open_default_uses_fallback_when_unknown() {
        let config = Config::default();
        let action = ShortcutAction::OpenDefaultApp {
            role: "nonexistent-role".into(),
            fallback: Some("xterm".into()),
        };
        let result = resolve_shortcut_action(&config, &action).expect("fallback resolved");
        match result {
            KeyAction::Run((cmd, args)) => {
                assert_eq!(cmd, "xterm");
                assert!(args.is_empty());
            }
            other => panic!("unexpected key action: {:?}", other),
        }
    }
}
