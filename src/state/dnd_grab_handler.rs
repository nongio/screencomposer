use std::os::fd::OwnedFd;

use lay_rs::prelude::Transition;
use smithay::{
    reexports::wayland_server::protocol::{wl_data_source::WlDataSource, wl_surface::WlSurface},
    wayland::selection::data_device::{ClientDndGrabHandler, ServerDndGrabHandler},
};

use super::{Backend, Otto};

impl<BackendData: Backend> ClientDndGrabHandler for Otto<BackendData> {
    fn started(
        &mut self,
        _source: Option<WlDataSource>,
        icon: Option<WlSurface>,
        _seat: smithay::input::Seat<Self>,
    ) {
        self.dnd_icon = icon;
        let p = self.get_cursor_position();
        let p = (p.x as f32, p.y as f32).into();
        self.workspaces.dnd_view.set_initial_position(p);
        self.workspaces.dnd_view.layer.set_scale((1.0, 1.0), None);

        self.workspaces
            .dnd_view
            .layer
            .set_opacity(0.8, Some(Transition::default()));
    }
    fn dropped(
        &mut self,
        _target: Option<WlSurface>,
        _validated: bool,
        _seat: smithay::input::Seat<Self>,
    ) {
        self.dnd_icon = None;
        self.workspaces
            .dnd_view
            .layer
            .set_opacity(0.0, Some(Transition::default()));
        self.workspaces
            .dnd_view
            .layer
            .set_scale((1.2, 1.2), Some(Transition::default()));
        // self.dnd_view.layer.set_position(self.dnd_view.initial_position, Some(Transition::default()));
    }
}
impl<BackendData: Backend> ServerDndGrabHandler for Otto<BackendData> {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: smithay::input::Seat<Self>) {
        unreachable!("Otto doesn't do server-side grabs");
    }
}
