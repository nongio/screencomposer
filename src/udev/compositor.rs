use std::sync::Mutex;

use smithay::{
    backend::{
        allocator::{dmabuf::Dmabuf, gbm::GbmAllocator},
        drm::{DrmDeviceFd, DrmNode, DrmSurface, GbmBufferedSurface},
        renderer::{
            damage::{Error as OutputDamageTrackerError, OutputDamageTracker},
            element::{
                texture::TextureBuffer, AsRenderElements, RenderElement, RenderElementStates,
            },
            gles::GlesTexture,
            multigpu::MultiTexture,
            sync::SyncPoint,
            Bind, DebugFlags, ExportMem, Offscreen, Renderer,
        },
        SwapBuffersError,
    },
    desktop::{space::SurfaceTree, utils::OutputPresentationFeedback, Space, Window},
    input::pointer::{CursorImageAttributes, CursorImageStatus},
    output::Output,
    reexports::wayland_server::{backend::GlobalId, protocol::wl_surface, DisplayHandle},
    utils::{Clock, IsAlive, Logical, Monotonic, Physical, Point, Rectangle, Scale},
    wayland::compositor,
};

use crate::{
    cursor::PointerElement,
    debug::fps::FpsElement,
    renderer::{
        layers_renderer::{LayersRenderer, LayersTexture},
        output_elements, CustomRenderElements, CLEAR_COLOR,
    },
    state::{post_repaint, take_presentation_feedback, ScreenComposer, SurfaceDmabufFeedback},
};

use super::{DrmSurfaceDmabufFeedback, GbmDrmCompositor, UdevData, UdevRenderer};

pub type RenderSurface =
    GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, Option<OutputPresentationFeedback>>;

pub enum SurfaceComposition {
    Surface {
        surface: RenderSurface,
        damage_tracker: OutputDamageTracker,
        debug_flags: DebugFlags,
    },
    Compositor(GbmDrmCompositor),
}

struct SurfaceCompositorRenderResult {
    rendered: bool,
    states: RenderElementStates,
    sync: Option<SyncPoint>,
    damage: Option<Vec<Rectangle<i32, Physical>>>,
}

impl SurfaceComposition {
    #[profiling::function]
    pub fn frame_submitted(
        &mut self,
    ) -> Result<Option<Option<OutputPresentationFeedback>>, SwapBuffersError> {
        match self {
            SurfaceComposition::Compositor(c) => {
                c.frame_submitted().map_err(Into::<SwapBuffersError>::into)
            }
            SurfaceComposition::Surface { surface, .. } => surface
                .frame_submitted()
                .map_err(Into::<SwapBuffersError>::into),
        }
    }

    pub fn format(&self) -> smithay::reexports::gbm::Format {
        match self {
            SurfaceComposition::Compositor(c) => c.format(),
            SurfaceComposition::Surface { surface, .. } => surface.format(),
        }
    }

    pub fn surface(&self) -> &DrmSurface {
        match self {
            SurfaceComposition::Compositor(c) => c.surface(),
            SurfaceComposition::Surface { surface, .. } => surface.surface(),
        }
    }

    pub fn reset_buffers(&mut self) {
        match self {
            SurfaceComposition::Compositor(c) => c.reset_buffers(),
            SurfaceComposition::Surface { surface, .. } => surface.reset_buffers(),
        }
    }

    #[profiling::function]
    fn queue_frame(
        &mut self,
        sync: Option<SyncPoint>,
        damage: Option<Vec<Rectangle<i32, Physical>>>,
        user_data: Option<OutputPresentationFeedback>,
    ) -> Result<(), SwapBuffersError> {
        match self {
            SurfaceComposition::Surface { surface, .. } => surface
                .queue_buffer(sync, damage, user_data)
                .map_err(Into::<SwapBuffersError>::into),
            SurfaceComposition::Compositor(c) => c
                .queue_frame(user_data)
                .map_err(Into::<SwapBuffersError>::into),
        }
    }

    #[profiling::function]
    fn render_frame<R, E, Target>(
        &mut self,
        renderer: &mut R,
        elements: &[E],
        clear_color: [f32; 4],
    ) -> Result<SurfaceCompositorRenderResult, SwapBuffersError>
    where
        R: Renderer + Bind<Dmabuf> + Bind<Target> + Offscreen<Target> + ExportMem,
        <R as Renderer>::TextureId: 'static,
        <R as Renderer>::Error: Into<SwapBuffersError>,
        E: RenderElement<R>,
    {
        match self {
            SurfaceComposition::Surface {
                surface,
                damage_tracker,
                debug_flags,
            } => {
                let (dmabuf, age) = surface
                    .next_buffer()
                    .map_err(Into::<SwapBuffersError>::into)?;
                renderer
                    .bind(dmabuf)
                    .map_err(Into::<SwapBuffersError>::into)?;
                let current_debug_flags = renderer.debug_flags();
                renderer.set_debug_flags(*debug_flags);
                let res = damage_tracker
                    .render_output(renderer, age.into(), elements, clear_color)
                    .map(|res| {
                        #[cfg(feature = "renderer_sync")]
                        res.sync.wait();
                        let rendered = res.damage.is_some();
                        SurfaceCompositorRenderResult {
                            rendered,
                            damage: res.damage,
                            states: res.states,
                            sync: rendered.then_some(res.sync),
                        }
                    })
                    .map_err(|err| match err {
                        OutputDamageTrackerError::Rendering(err) => err.into(),
                        _ => unreachable!(),
                    });
                renderer.set_debug_flags(current_debug_flags);
                res
            }
            SurfaceComposition::Compositor(compositor) => compositor
                .render_frame(renderer, elements, clear_color)
                .map(|render_frame_result| {
                    #[cfg(feature = "renderer_sync")]
                    if let PrimaryPlaneElement::Swapchain(element) =
                        render_frame_result.primary_element
                    {
                        element.sync.wait();
                    }
                    SurfaceCompositorRenderResult {
                        rendered: render_frame_result.damage.is_some(),
                        damage: None,
                        states: render_frame_result.states,
                        sync: None,
                    }
                })
                .map_err(|err| match err {
                    smithay::backend::drm::compositor::RenderFrameError::PrepareFrame(err) => {
                        err.into()
                    }
                    smithay::backend::drm::compositor::RenderFrameError::RenderFrame(
                        OutputDamageTrackerError::Rendering(err),
                    ) => err.into(),
                    _ => unreachable!(),
                }),
        }
    }

    pub fn set_debug_flags(&mut self, flags: DebugFlags) {
        match self {
            SurfaceComposition::Surface {
                surface,
                debug_flags,
                ..
            } => {
                *debug_flags = flags;
                surface.reset_buffers();
            }
            SurfaceComposition::Compositor(c) => c.set_debug_flags(flags),
        }
    }
}

pub struct SurfaceData {
    pub dh: DisplayHandle,
    pub device_id: DrmNode,
    pub render_node: DrmNode,
    pub global: Option<GlobalId>,
    pub compositor: SurfaceComposition,
    #[cfg(feature = "debug")]
    pub fps: fps_ticker::Fps,
    #[cfg(feature = "debug")]
    pub fps_element: Option<FpsElement<MultiTexture>>,
    pub dmabuf_feedback: Option<DrmSurfaceDmabufFeedback>,
}

impl Drop for SurfaceData {
    fn drop(&mut self) {
        if let Some(global) = self.global.take() {
            self.dh.remove_global::<ScreenComposer<UdevData>>(global);
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[profiling::function]
pub fn render_surface<'a, 'b>(
    surface: &'a mut SurfaceData,
    renderer: &mut UdevRenderer<'a, 'b>,
    space: &Space<Window>,
    output: &Output,
    pointer_location: Point<f64, Logical>,
    pointer_image: &TextureBuffer<MultiTexture>,
    pointer_element: &mut PointerElement<MultiTexture>,
    dnd_icon: &Option<wl_surface::WlSurface>,
    cursor_status: &mut CursorImageStatus,
    clock: &Clock<Monotonic>,
    // show_window_preview: bool,
) -> Result<bool, SwapBuffersError> {
    let output_geometry = space.output_geometry(output).unwrap();
    let scale = Scale::from(output.current_scale().fractional_scale());

    let mut custom_elements: Vec<CustomRenderElements<_>> = Vec::new();

    if output_geometry.to_f64().contains(pointer_location) {
        let cursor_hotspot = if let CursorImageStatus::Surface(ref surface) = cursor_status {
            compositor::with_states(surface, |states| {
                states
                    .data_map
                    .get::<Mutex<CursorImageAttributes>>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .hotspot
            })
        } else {
            (0, 0).into()
        };
        let cursor_pos = pointer_location - output_geometry.loc.to_f64() - cursor_hotspot.to_f64();
        let cursor_pos_scaled = cursor_pos.to_physical(scale).to_i32_round();

        // set cursor
        pointer_element.set_texture(pointer_image.clone());

        // draw the cursor as relevant
        {
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Surface(ref surface) = *cursor_status {
                reset = !surface.alive();
            }
            if reset {
                *cursor_status = CursorImageStatus::default_named();
            }

            pointer_element.set_status(cursor_status.clone());
        }

        custom_elements.extend(pointer_element.render_elements(
            renderer,
            cursor_pos_scaled,
            scale,
            1.0,
        ));

        // draw the dnd icon if applicable
        {
            if let Some(wl_surface) = dnd_icon.as_ref() {
                if wl_surface.alive() {
                    custom_elements.extend(
                        AsRenderElements::<UdevRenderer<'a, 'b>>::render_elements(
                            &SurfaceTree::from_surface(wl_surface),
                            renderer,
                            cursor_pos_scaled,
                            scale,
                            1.0,
                        ),
                    );
                }
            }
        }
    }

    #[cfg(feature = "debug")]
    if let Some(element) = surface.fps_element.as_mut() {
        element.update_fps(surface.fps.avg().round() as u32);
        surface.fps.tick();
        custom_elements.push(CustomRenderElements::Fps(element.clone()));
    }

    let (elements, clear_color) = output_elements(
        output,
        space,
        custom_elements,
        renderer,
        // show_window_preview,
    );
    let res =
        surface
            .compositor
            .render_frame::<_, _, GlesTexture>(renderer, &elements, clear_color)?;

    post_repaint(
        output,
        &res.states,
        space,
        surface
            .dmabuf_feedback
            .as_ref()
            .map(|feedback| SurfaceDmabufFeedback {
                render_feedback: &feedback.render_feedback,
                scanout_feedback: &feedback.scanout_feedback,
            }),
        clock.now(),
    );

    if res.rendered {
        let output_presentation_feedback = take_presentation_feedback(output, space, &res.states);
        surface
            .compositor
            .queue_frame(res.sync, res.damage, Some(output_presentation_feedback))
            .map_err(Into::<SwapBuffersError>::into)?;
    }

    Ok(res.rendered)
}

pub fn initial_render(
    surface: &mut SurfaceData,
    renderer: &mut UdevRenderer<'_, '_>,
) -> Result<(), SwapBuffersError> {
    surface
        .compositor
        .render_frame::<_, CustomRenderElements<_>, GlesTexture>(renderer, &[], CLEAR_COLOR)?;
    surface.compositor.queue_frame(None, None, None)?;
    surface.compositor.reset_buffers();

    Ok(())
}
