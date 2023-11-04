use crate::{
    grabs,
    state::{Backend, ClientState, SurfaceLayer},
    ScreenComposer,
};
use smithay::{
    backend::renderer::utils::{on_commit_buffer_handler, CommitCounter},
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client, Resource,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, with_states, CompositorClientState, CompositorHandler,
            CompositorState,
        },
        shm::{ShmHandler, ShmState},
    },
};
use tracing::{debug, trace};

use super::xdg_shell;

fn is_subsurface(surface: &WlSurface) -> bool {
    with_states(surface, |states| states.role == Some("subsurface"))
}

impl<BackendData: Backend> CompositorHandler for ScreenComposer<BackendData> {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        debug!("Commit on {:?}", surface.id());
        on_commit_buffer_handler::<Self>(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == &root)
            {
                window.on_commit();
            }
        }
        if is_subsurface(surface) {
            let sid = surface.id();

            let layer = self.layer_for(&sid);
            if let Some(SurfaceLayer {
                layer,
                commit_counter: cc,
                parent: layer_parent,
            }) = layer
            {
                let surface_parent = get_parent(surface).map(|p| p.id());
                if layer_parent != surface_parent {
                    if let Some(SurfaceLayer {
                        layer: layer_parent,
                        ..
                    }) = surface_parent.clone().and_then(|p| self.layer_for(&p))
                    {
                        self.engine
                            .scene_add_layer_to(layer.clone(), layer_parent.id());
                        trace!(
                            "Add layer to parent {:?} to {:?}",
                            surface.id(),
                            surface_parent,
                        );

                        // self.layers_map.iter().for_each(|(k, v)| {
                        //     println!(">> layer {:?} parent: {:?}", k, v.2);
                        // });
                        self.map_layer(sid, layer, cc, surface_parent);
                    } else {
                        trace!("edge case {:?} {:?}", sid, layer_parent);
                    }
                }
            }
        }

        xdg_shell::handle_commit(
            &mut self.popups,
            &self.space,
            surface,
            &mut self.layers_map,
            &self.engine,
        );
        // grabs::handle_commit(&mut self.space, surface);
    }
    fn new_surface(&mut self, surface: &WlSurface) {
        // add_pre_commit_hook::<Self, _>(surface, move |state, _dh, surface| {
        //     let layer = state.layers_map.get(&surface.id());
        //     let parent = get_parent(surface);
        //     if let Some((layer, cc, layer_parent)) = layer {
        //         if layer_parent.is_none() && parent.is_some() {
        //             // the layer exists but is not added to the scene
        //             let parent = parent.unwrap();
        //             println!("layer {:?} with parent {:?}", surface.id(), parent.id());
        //             if let Some((layer_parent, ..), ..) = state.layers_map.get(&parent.id()) {
        //                 state
        //                     .engine
        //                     .scene_add_layer_to(layer.clone(), layer_parent.id());
        //                 println!(
        //                     "add layer to parent {:?} to {:?}",
        //                     layer.id(),
        //                     layer_parent.id()
        //                 );
        //                 state
        //                     .layers_map
        //                     .insert(surface.id(), (layer.clone(), *cc, Some(parent.id())));
        //             }
        //         }
        //     }
        // });
        let layer = self.layer_for(&surface.id());
        if let Some(SurfaceLayer { layer: _, .. }) = layer {
            // NOOP
        } else {
            let layer = self.engine.new_layer();
            println!("New surface created new layer {:?}", surface.id());
            layer.set_border_width(1.0, None);

            self.map_layer(surface.id(), layer, CommitCounter::from(0), None);
        }
    }
    fn destroyed(&mut self, surface: &WlSurface) {
        // self.unmap_layer(&surface.id());
        let ssid = surface.id();
        let layer_map = self.layer_for(&ssid);
        if let Some(SurfaceLayer { layer, .. }) = layer_map {
            trace!("removing layer {:?}", ssid);
            self.engine.scene_remove_layer(layer.id());
        }
        self.unmap_layer(&ssid);
        // compositor::with_surface_tree_upward(
        //     surface,
        //     (),
        //     |_, _, _| TraversalAction::DoChildren(()),
        //     |subsurface, _states, _| {
        //         let ssid = subsurface.id();
        //         let layer_map = self.layer_for(&ssid);
        //         if let Some(SurfaceLayer { layer, .. }) = layer_map {
        //             trace!("removing layer {:?}", ssid);
        //             self.engine.scene_remove_layer(layer.id());
        //         }
        //         self.unmap_layer(&ssid);
        //     },
        //     |_, _, _| true,
        // );
        println!("Destroyed surface {:?}", surface.id());
    }
}

impl<BackendData: Backend> BufferHandler for ScreenComposer<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl<BackendData: Backend> ShmHandler for ScreenComposer<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
delegate_shm!(@<BackendData: Backend + 'static> ScreenComposer<BackendData>);
