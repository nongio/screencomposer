use layers::prelude::Layer;
use smithay::{
    backend::renderer::utils::CommitCounter,
    desktop::{layer_map_for_output, Window, WindowSurfaceType},
    reexports::wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface},
    utils::{Logical, Point},
    wayland::shell::{
        wlr_layer::{KeyboardInteractivity, Layer as WlrLayer, LayerSurfaceCachedState},
        xdg::XdgToplevelSurfaceData,
    },
};

use tracing::{debug, error, info, trace, warn};

use crate::{focus::FocusTarget, handlers::xdg_shell::FullscreenSurface, state::SurfaceLayer};

use super::Backend;

impl<BackendData: Backend> super::ScreenComposer<BackendData> {
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(FocusTarget, Point<i32, Logical>)> {
        let output = self.space.outputs().find(|o| {
            let geometry = self.space.output_geometry(o).unwrap();
            geometry.contains(pos.to_i32_round())
        })?;
        let output_geo = self.space.output_geometry(output).unwrap();
        let layers = layer_map_for_output(output);

        let mut under = None;
        if let Some(window) = output
            .user_data()
            .get::<FullscreenSurface>()
            .and_then(|f| f.get())
        {
            under = Some((window.into(), output_geo.loc));
        } else if let Some(layer) = layers
            .layer_under(WlrLayer::Overlay, pos)
            .or_else(|| layers.layer_under(WlrLayer::Top, pos))
        {
            let layer_loc = layers.layer_geometry(layer).unwrap().loc;
            under = Some((layer.clone().into(), output_geo.loc + layer_loc))
        } else if let Some((window, location)) = self.space.element_under(pos) {
            under = Some((window.clone().into(), location));
        } else if let Some(layer) = layers
            .layer_under(WlrLayer::Bottom, pos)
            .or_else(|| layers.layer_under(WlrLayer::Background, pos))
        {
            let layer_loc = layers.layer_geometry(layer).unwrap().loc;
            under = Some((layer.clone().into(), output_geo.loc + layer_loc));
        };
        under
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
