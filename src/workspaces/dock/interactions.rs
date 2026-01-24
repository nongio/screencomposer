use smithay::{backend::input::ButtonState, utils::IsAlive};

use crate::{config::Config, interactive_view::ViewInteractions};

use tracing::warn;

use super::DockView;

// Dock view interactions
impl<Backend: crate::state::Backend> ViewInteractions<Backend> for DockView {
    fn id(&self) -> Option<usize> {
        Some(self.wrap_layer.id.0.into())
    }
    fn is_alive(&self) -> bool {
        self.alive()
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::Otto<Backend>>,
        _data: &mut crate::Otto<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        if self.dragging.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let scale = Config::with(|c| c.screen_scale);

        self.update_magnification_position((event.location.x * scale) as f32);
    }
    fn on_leave(&self, _serial: smithay::utils::Serial, _time: u32) {
        self.update_magnification_position(-500.0);
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::Otto<Backend>>,
        state: &mut crate::Otto<Backend>,
        event: &smithay::input::pointer::ButtonEvent,
    ) {
        match event.state {
            ButtonState::Pressed => {
                // println!("dock Button pressed");
                if let Some(layer_id) = state.layers_engine.current_hover() {
                    if let Some((_identifier, _match_id)) = self.get_app_from_layer(&layer_id) {
                        self.dragging
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            }
            ButtonState::Released => {
                if let Some(layer_id) = state.layers_engine.current_hover() {
                    if let Some((identifier, match_id)) = self.get_app_from_layer(&layer_id) {
                        // if we click on an app icon, focus the app
                        if let Some(wid) = state.workspaces.focus_app(&identifier) {
                            state.set_keyboard_focus_on_surface(&wid);
                        } else if let Some(bookmark) = self.bookmark_config_for(&match_id) {
                            if let Some(app) = self.bookmark_application(&match_id) {
                                if let Some((cmd, args)) = app.command(&bookmark.exec_args) {
                                    state.launch_program(cmd, args);
                                } else {
                                    warn!("bookmark {} has no executable command", identifier);
                                }
                            } else {
                                warn!("bookmark {} not loaded into dock", identifier);
                            }
                        }
                    } else if let Some(wid) = self.get_window_from_layer(&layer_id) {
                        // if we click on a minimized window, unminimize it
                        if let Some(wid) = state.workspaces.unminimize_window(&wid) {
                            state.workspaces.focus_app_with_window(&wid);
                            state.set_keyboard_focus_on_surface(&wid);
                        }
                    }
                }
                self.dragging
                    .store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}
