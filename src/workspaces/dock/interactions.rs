use smithay::{backend::input::ButtonState, utils::IsAlive};

use crate::{config::Config, interactive_view::ViewInteractions};

use super::DockView;

// Dock view interactions
impl<Backend: crate::state::Backend> ViewInteractions<Backend> for DockView {
    fn id(&self) -> Option<usize> {
        self.wrap_layer.id().map(|id| id.0.into())
    }
    fn is_alive(&self) -> bool {
        self.alive()
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        _data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        let scale = Config::with(|c| c.screen_scale);

        self.update_magnification_position((event.location.x * scale) as f32);
    }
    fn on_leave(&self, _serial: smithay::utils::Serial, _time: u32) {
        self.update_magnification_position(-500.0);
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::ButtonEvent,
    ) {
        match event.state {
            ButtonState::Pressed => {
                // println!("dock Button pressed");
            }
            ButtonState::Released => {
                if let Some(layer_id) = data.layers_engine.current_hover() {
                    if let Some(identifier) = self.get_appid_from_layer(&layer_id) {
                        data.raise_app_elements(&identifier, true, Some(event.serial));
                    } else if let Some(window) = self.get_window_from_layer(&layer_id) {
                        data.unminimize_window(&window.window_element.unwrap().clone());
                    }
                }
            }
        }
    }
}
