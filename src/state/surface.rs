use layers::prelude::Layer;
use smithay::{
    backend::renderer::utils::CommitCounter,
    desktop::{Window, WindowSurfaceType},
    utils::{Logical, Point},
};
use wayland_backend::server::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

use tracing::{debug, error, info, trace, warn};

use crate::state::SurfaceLayer;

use super::Backend;

impl<BackendData: Backend> super::ScreenComposer<BackendData> {
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p + location))
            })
    }
    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<Window> {
        self.space
            .elements()
            .find(|window| *window.toplevel().wl_surface() == *surface)
            .cloned()
    }

    pub fn map_layer(
        &mut self,
        sid: ObjectId,
        layer: Layer,
        commit_counter: CommitCounter,
        parent: Option<ObjectId>,
    ) {
        trace!(
            "map_layer {:?} commit {:?} with parent {:?} ",
            sid,
            commit_counter,
            parent
        );

        self.layers_map.insert(
            sid,
            SurfaceLayer {
                layer,
                commit_counter,
                parent,
            },
        );
    }

    pub fn unmap_layer(&mut self, sid: &ObjectId) {
        self.layers_map.remove(sid);
    }
    pub fn layer_for(&self, sid: &ObjectId) -> Option<SurfaceLayer> {
        self.layers_map.get(sid).cloned()
    }
}
