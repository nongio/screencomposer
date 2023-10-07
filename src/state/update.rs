use std::{borrow::BorrowMut, time::Duration};

use layers::prelude::{self, DrawScene};

use smithay::{
    backend::{
        self,
        egl::EGLSurface,
        renderer::{
            element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer,
            utils::RendererSurfaceStateUserData, Renderer,
        },
        winit::WinitGraphicsBackend,
    },
    desktop::PopupManager,
    output::Output,
    utils::Rectangle,
    wayland::compositor::{self, TraversalAction},
};
use tracing::{debug, error, info, trace, warn};
use wayland_server::{protocol::wl_surface::WlSurface, Display, Resource};

use super::{Backend, ScreenComposer, SurfaceLayer};

impl<BackendData: Backend> super::ScreenComposer<BackendData> {
    /// map surfaces buffer content to engine Images
    /// it requires a backend renderer to import the surfaces in GLes
    pub fn map_surface_textures(&mut self, backend: &mut WinitGraphicsBackend<GlesRenderer>) {
        // let egl_surface = backend.egl_surface();
        let renderer: &mut GlesRenderer = backend.renderer();

        // let space = self.space;
        // let layers_map = self.layers_map;
        // let skia_renderer = self.skia_renderer;
        // let engine = self.engine;
        // let popups = self.popups;

        let elements = self.space.elements();
        let elements = elements.flat_map(|window| {
            let surface: &WlSurface = window.toplevel().wl_surface();
            let mut subsurfaces = Vec::new();

            let mut surfaces: Vec<WlSurface> = PopupManager::popups_for_surface(surface)
                .map(|(popup, location)| {
                    let surface = popup.wl_surface();
                    compositor::with_surface_tree_downward(
                        surface,
                        (),
                        |_, _, _| TraversalAction::DoChildren(()),
                        |wl_surface, states, _| {
                            let surface_size = states
                                .data_map
                                .get::<RendererSurfaceStateUserData>()
                                .map(|d| d.borrow().surface_size().unwrap_or_default())
                                .unwrap_or_default();

                            if let Some(SurfaceLayer { layer, .. }) =
                                self.layers_map.get(&wl_surface.id())
                            {
                                layer
                                    .set_size((surface_size.w as f32, surface_size.h as f32), None);
                            }
                            subsurfaces.push(wl_surface.clone());
                        },
                        |_, _, _| true,
                    );
                    surface.clone()
                })
                .collect();
            compositor::with_surface_tree_downward(
                surface,
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |wl_surface, _, _| {
                    subsurfaces.push(wl_surface.clone());
                },
                |_, _, _| true,
            );
            surfaces.push(surface.clone());
            surfaces.extend(subsurfaces);
            surfaces
        });

        let skia_renderer = self.skia_renderer.borrow_mut().as_mut().unwrap().get_mut();
        let mut skia_surface = skia_renderer.surface();
        let canvas = skia_surface.canvas();
        let context = &mut canvas.recording_context().unwrap();
        let elements: Vec<WlSurface> = elements.collect();
        elements.iter().for_each(|surface| {
            compositor::with_states(&surface, |states| {
                WaylandSurfaceRenderElement::<GlesRenderer>::from_surface(
                    renderer,
                    &surface,
                    states,
                    (0.0, 0.0).into(),
                    1.0,
                );

                let surface_id = surface.id();

                let data: Option<
                    &std::cell::RefCell<smithay::backend::renderer::utils::RendererSurfaceState>,
                > = states.data_map.get::<RendererSurfaceStateUserData>();

                if let Some(data) = data {
                    let data = data.borrow();

                    let commit = data.current_commit();

                    if let Some(texture) = data.texture::<GlesRenderer>(renderer.id()) {
                        let gl_target = gl_rs::TEXTURE_2D;

                        if let Some(SurfaceLayer {
                            layer,
                            commit_counter,
                            parent,
                        }) = self.layer_for(&surface_id)
                        {
                            let size = data.buffer_size().unwrap();

                            if commit != commit_counter {
                                layer.set_content_from_texture(
                                    context,
                                    texture.tex_id(),
                                    gl_target,
                                    prelude::Point {
                                        x: size.w as f32,
                                        y: size.h as f32,
                                    },
                                );
                                // println!("set_content {:?} commit {:?}", surface_id, commit);
                                self.map_layer(surface_id, layer, commit, parent);
                            }
                        }
                    }
                }
            });
        });
    }

    pub fn egl_make_current(&self, egl_surface: &mut EGLSurface, renderer: &mut GlesRenderer) {
        // let egl_surface = backend.egl_surface();
        // let renderer: &mut GlesRenderer = backend.renderer();
        let egl_context = renderer.egl_context();
        unsafe {
            let res = egl_context.make_current_with_surface(&egl_surface);
            res.unwrap_or_else(|err| {
                error!("Error making context current: {:?}", err);
            })
        }
    }
    /// update the engine and draw the scene
    pub fn update(
        &mut self,
        backend_data: BackendData,
        display: &mut Display<ScreenComposer<BackendData>>,
        output: &Output,
        // backend: &mut WinitGraphicsBackend<GlesRenderer>,
    ) {
        // let backend = backend_data.;
        // let display = self.display_handle.;
        // let _scale = backend.window().scale_factor();
        // let size = backend.window_size().physical_size;
        let size = output.physical_properties().size;
        // let egl_surface = backend.egl_surface();
        // let renderer: &mut GlesRenderer = backend.renderer();

        // let space = self.space;
        // let layers_map = self.layers_map;
        // let skia_renderer = self.skia_renderer;
        // let engine = self.engine;
        // let popups = self.popups;

        let mut i = 0;
        // let mut elements = Vec::new();

        // DrawScene
        let dt = 0.016;
        // state.needs_redraw =
        self.engine.update(dt);

        let damage = Rectangle::from_loc_and_size((0, 0), size);

        // self.egl_make_current()
        // self.draw_scene();

        // backend.submit(Some(&[damage]))?;

        self.space.elements().for_each(|window| {
            window.send_frame(
                output,
                self.start_time.elapsed(),
                Some(Duration::ZERO),
                |_, _| Some(output.clone()),
            )
        });

        self.space.refresh();
        self.popups.cleanup();
        display.flush_clients().unwrap();
    }

    /// draw the scene using a scene_renderer
    pub fn draw_scene(&mut self) {
        if let Some(root) = self.engine.scene_root() {
            if self.skia_renderer.is_some() && self.needs_redraw {
                let skia_renderer = self.skia_renderer.as_mut().unwrap().get_mut();
                skia_renderer.draw_scene(self.engine.scene(), root);
            }
        }
    }
}
