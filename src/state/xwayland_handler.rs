use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

use crate::focus::KeyboardFocusTarget;

use super::{Backend, ScreenComposer};


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
