use smithay::{
    backend::input::TabletToolDescriptor,
    delegate_seat, delegate_tablet_manager,
    input::{pointer::CursorImageStatus, SeatHandler, SeatState},
    reexports::wayland_server::Resource,
    wayland::{
        seat::WaylandFocus,
        selection::{data_device::set_data_device_focus, primary_selection::set_primary_focus},
        tablet_manager::TabletSeatHandler,
    },
};

use crate::focus::{KeyboardFocusTarget, PointerFocusTarget};

use super::{Backend, Otto};

impl<BackendData: Backend> SeatHandler for Otto<BackendData> {
    type KeyboardFocus = KeyboardFocusTarget<BackendData>;
    type PointerFocus = PointerFocusTarget<BackendData>;
    type TouchFocus = PointerFocusTarget<BackendData>;

    fn seat_state(&mut self) -> &mut SeatState<Otto<BackendData>> {
        &mut self.seat_state
    }

    fn focus_changed(
        &mut self,
        seat: &smithay::input::Seat<Self>,
        target: Option<&KeyboardFocusTarget<BackendData>>,
    ) {
        let dh = &self.display_handle;

        let wl_surface = target.and_then(WaylandFocus::wl_surface);

        let focus = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }

    fn cursor_image(&mut self, _seat: &smithay::input::Seat<Self>, image: CursorImageStatus) {
        *self.cursor_status.lock().unwrap() = image.clone();
        self.cursor_manager.set_cursor_image(image);
    }
    fn led_state_changed(
        &mut self,
        _seat: &smithay::input::Seat<Self>,
        _led_state: smithay::input::keyboard::LedState,
    ) {
        println!("keyboard led_state_changed {:?}", _led_state);
    }
}

impl<BackendData: Backend> TabletSeatHandler for Otto<BackendData> {
    fn tablet_tool_image(&mut self, _tool: &TabletToolDescriptor, image: CursorImageStatus) {
        let mut cursor_status = self.cursor_status.lock().unwrap();
        *cursor_status = image.clone();
        self.cursor_manager.set_cursor_image(image);
    }
}

delegate_seat!(@<BackendData: Backend + 'static> Otto<BackendData>);
delegate_tablet_manager!(@<BackendData: Backend + 'static> Otto<BackendData>);
