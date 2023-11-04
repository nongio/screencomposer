mod compositor;
mod decoration;
pub mod element;
pub mod ssd;
pub mod xdg_shell;
use crate::ScreenComposer;
use crate::{focus::FocusTarget, state::Backend};

//
// Wl Seat
//

use smithay::reexports::wayland_server::Resource;
use smithay::{
    input::{Seat, SeatHandler, SeatState},
    wayland::seat::WaylandFocus,
};
use smithay::{
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::selection::primary_selection::set_primary_focus,
};

use smithay::wayland::selection::{
    data_device::{
        set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, ServerDndGrabHandler,
    },
    primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
    wlr_data_control::{DataControlHandler, DataControlState},
    SelectionHandler,
};
use smithay::{
    delegate_data_control, delegate_data_device, delegate_output, delegate_primary_selection,
    delegate_seat,
};
// use tracing::{debug, error, info, trace, warn};

impl<BackendData: Backend> SeatHandler for ScreenComposer<BackendData> {
    type KeyboardFocus = FocusTarget;
    type PointerFocus = FocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<ScreenComposer<BackendData>> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, target: Option<&FocusTarget>) {
        let dh = &self.display_handle;

        let wl_surface = target.and_then(WaylandFocus::wl_surface);

        let focus = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }
}

delegate_seat!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

impl<BackendData: Backend> SelectionHandler for ScreenComposer<BackendData> {
    type SelectionUserData = ();

    // #[cfg(feature = "xwayland")]
    // fn new_selection(
    //     &mut self,
    //     ty: SelectionTarget,
    //     source: Option<SelectionSource>,
    //     _seat: Seat<Self>,
    // ) {
    //     // use smithay::wayland::selection::{SelectionSource, SelectionTarget};

    //     if let Some(xwm) = self.xwm.as_mut() {
    //         if let Err(err) = xwm.new_selection(ty, source.map(|source| source.mime_types())) {
    //             warn!(?err, ?ty, "Failed to set Xwayland selection");
    //         }
    //     }
    // }

    // #[cfg(feature = "xwayland")]
    // fn send_selection(
    //     &mut self,
    //     ty: SelectionTarget,
    //     mime_type: String,
    //     fd: OwnedFd,
    //     _seat: Seat<Self>,
    //     _user_data: &(),
    // ) {
    //     use smithay::wayland::selection::SelectionTarget;
    //     use wayland_backend::io_lifetimes::OwnedFd;

    //     if let Some(xwm) = self.xwm.as_mut() {
    //         if let Err(err) = xwm.send_selection(ty, mime_type, fd, self.handle.clone()) {
    //             warn!(?err, "Failed to send primary (X11 -> Wayland)");
    //         }
    //     }
    // }
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

//
// Wl Data Device
//

impl<BackendData: Backend> DataDeviceHandler for ScreenComposer<BackendData> {
    fn data_device_state(&self) -> &smithay::wayland::selection::data_device::DataDeviceState {
        &self.data_device_state
    }
}

impl<BackendData: Backend> ClientDndGrabHandler for ScreenComposer<BackendData> {}
impl<BackendData: Backend> ServerDndGrabHandler for ScreenComposer<BackendData> {}

delegate_data_device!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);

//
// Wl Output & Xdg Output
//

delegate_output!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
