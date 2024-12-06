use std::{
    borrow::Cow,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use lay_rs::prelude::Layer;
use smithay::{
    backend::renderer::{
        element::{
            solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement, AsRenderElements,
        },
        ImportAll, ImportMem, Renderer, Texture,
    },
    desktop::{
        space::SpaceElement, utils::OutputPresentationFeedback, Window, WindowSurface,
        WindowSurfaceType,
    },
    output::Output,
    reexports::{
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    },
    render_elements,
    utils::{user_data::UserDataMap, IsAlive, Logical, Physical, Point, Rectangle, Scale},
    wayland::{
        compositor::SurfaceData as WlSurfaceData,
        dmabuf::DmabufFeedback,
        seat::WaylandFocus,
        shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData},
    },
};

use crate::{focus::PointerFocusTarget, state::Backend};

#[derive(Debug, Clone)]
pub struct WindowElement(pub Arc<WindowElementInner>);

#[derive(Debug, Clone)]
pub struct WindowElementInner {
    window: Window,
    pub is_maximized: Arc<AtomicBool>,
    pub is_minimized: Arc<AtomicBool>,
    pub app_id: String,
    pub base_layer: Layer,
}

impl PartialEq for WindowElement {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl WindowElement {
    pub fn new(window: Window, layer: Layer) -> Self {
        Self(Arc::new(WindowElementInner {
            window,
            is_maximized: Arc::new(AtomicBool::new(false)),
            is_minimized: Arc::new(AtomicBool::new(false)),
            app_id: "".to_string(),
            base_layer: layer,
        }))
    }
    pub fn id(&self) -> ObjectId {
        self.0.window.wl_surface().unwrap().as_ref().id()
    }
    pub fn surface_under<B: Backend>(
        &self,
        location: Point<f64, Logical>,
        window_type: WindowSurfaceType,
    ) -> Option<(PointerFocusTarget<B>, Point<i32, Logical>)> {
        // let state = self.decoration_state();

        // let offset = if state.is_ssd {
        //     Point::from((0, HEADER_BAR_HEIGHT))
        // } else {
        //     Point::default()
        // };
        let offset = Point::default();

        let surface_under = self
            .0
            .window
            .surface_under(location - offset.to_f64(), window_type);
        let (under, loc) = match self.0.window.underlying_surface() {
            WindowSurface::Wayland(_) => {
                surface_under.map(|(surface, loc)| (PointerFocusTarget::WlSurface(surface), loc))
            }
            #[cfg(feature = "xwayland")]
            WindowSurface::X11(s) => {
                surface_under.map(|(_, loc)| (PointerFocusTarget::X11Surface(s.clone()), loc))
            }
        }?;
        Some((under, loc + offset))
    }

    pub fn with_surfaces<F>(&self, processor: F)
    where
        F: FnMut(&WlSurface, &WlSurfaceData),
    {
        self.0.window.with_surfaces(processor);
    }

    pub fn send_frame<T, F>(
        &self,
        output: &Output,
        time: T,
        throttle: Option<Duration>,
        primary_scan_out_output: F,
    ) where
        T: Into<Duration>,
        F: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
    {
        self.0
            .window
            .send_frame(output, time, throttle, primary_scan_out_output)
    }

    pub fn send_dmabuf_feedback<'a, P, F>(
        &self,
        output: &Output,
        primary_scan_out_output: P,
        select_dmabuf_feedback: F,
    ) where
        P: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
        F: Fn(&WlSurface, &WlSurfaceData) -> &'a DmabufFeedback + Copy,
    {
        self.0
            .window
            .send_dmabuf_feedback(output, primary_scan_out_output, select_dmabuf_feedback)
    }

    pub fn take_presentation_feedback<F1, F2>(
        &self,
        output_feedback: &mut OutputPresentationFeedback,
        primary_scan_out_output: F1,
        presentation_feedback_flags: F2,
    ) where
        F1: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
        F2: FnMut(&WlSurface, &WlSurfaceData) -> wp_presentation_feedback::Kind + Copy,
    {
        self.0.window.take_presentation_feedback(
            output_feedback,
            primary_scan_out_output,
            presentation_feedback_flags,
        )
    }

    #[cfg(feature = "xwayland")]
    #[inline]
    pub fn is_x11(&self) -> bool {
        self.window.is_x11()
    }

    #[inline]
    pub fn is_wayland(&self) -> bool {
        self.0.window.is_wayland()
    }

    #[inline]
    pub fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        self.0.window.wl_surface()
    }

    #[inline]
    pub fn toplevel(&self) -> Option<&ToplevelSurface> {
        self.0.window.toplevel()
    }

    #[inline]
    pub fn user_data(&self) -> &UserDataMap {
        self.0.window.user_data()
    }

    pub fn underlying_surface(&self) -> &WindowSurface {
        self.0.window.underlying_surface()
    }

    pub fn geometry(&self) -> Rectangle<i32, Logical> {
        self.0.window.geometry()
    }

    pub fn bbox(&self) -> Rectangle<i32, Logical> {
        self.0.window.bbox()
    }

    pub fn on_commit(&self) {
        self.0.window.on_commit()
    }

    pub fn base_layer(&self) -> &Layer {
        &self.0.base_layer
    }

    pub fn app_id(&self) -> &str {
        &self.0.app_id
    }

    pub fn is_minimised(&self) -> bool {
        self.0
            .is_minimized
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_is_minimised(&self, is_minimized: bool) {
        self.0
            .is_minimized
            .store(is_minimized, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_maximized(&self) -> bool {
        self.0
            .is_maximized
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_is_maximized(&self, is_maximized: bool) {
        self.0
            .is_maximized
            .store(is_maximized, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_fullscreen(&self) -> bool {
        self.wl_surface()
            .map(|window_surface| {
                smithay::wayland::compositor::with_states(&window_surface, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .current
                        .fullscreen_output
                        .is_some()
                })
            })
            .unwrap_or(false)
    }

    pub fn title(&self) -> String {
        self.wl_surface()
            .map(|window_surface| {
                smithay::wayland::compositor::with_states(&window_surface, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .title
                        .clone()
                })
                .unwrap_or_default()
            })
            .unwrap_or_default()
    }
}

impl IsAlive for WindowElement {
    #[inline]
    fn alive(&self) -> bool {
        self.0.window.alive()
    }
}

impl SpaceElement for WindowElement {
    fn geometry(&self) -> Rectangle<i32, Logical> {
        // if self.decoration_state().is_ssd {
        //     geo.size.h += HEADER_BAR_HEIGHT;
        // }
        SpaceElement::geometry(&self.0.window)
    }
    fn bbox(&self) -> Rectangle<i32, Logical> {
        // if self.decoration_state().is_ssd {
        //     bbox.size.h += HEADER_BAR_HEIGHT;
        // }
        SpaceElement::bbox(&self.0.window)
    }
    fn is_in_input_region(&self, point: &Point<f64, Logical>) -> bool {
        // if self.decoration_state().is_ssd {
        //     point.y < HEADER_BAR_HEIGHT as f64
        //         || SpaceElement::is_in_input_region(
        //             &self.window,
        //             &(*point - Point::from((0.0, HEADER_BAR_HEIGHT as f64))),
        //         )
        // } else {
        SpaceElement::is_in_input_region(&self.0.window, point)
        // }
    }
    fn z_index(&self) -> u8 {
        SpaceElement::z_index(&self.0.window)
    }

    fn set_activate(&self, activated: bool) {
        SpaceElement::set_activate(&self.0.window, activated);
    }
    fn output_enter(&self, output: &Output, overlap: Rectangle<i32, Logical>) {
        SpaceElement::output_enter(&self.0.window, output, overlap);
    }
    fn output_leave(&self, output: &Output) {
        SpaceElement::output_leave(&self.0.window, output);
    }
    #[profiling::function]
    fn refresh(&self) {
        SpaceElement::refresh(&self.0.window);
    }
}

render_elements!(
    pub WindowRenderElement<R> where R: ImportAll + ImportMem;
    Window=WaylandSurfaceRenderElement<R>,
    Decoration=SolidColorRenderElement,
);

impl<R: Renderer> std::fmt::Debug for WindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window(arg0) => f.debug_tuple("Window").field(arg0).finish(),
            Self::Decoration(arg0) => f.debug_tuple("Decoration").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

impl<R> AsRenderElements<R> for WindowElement
where
    R: Renderer + ImportAll + ImportMem,
    <R as Renderer>::TextureId: Clone + Texture + 'static,
{
    type RenderElement = WindowRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {
        let _window_bbox = SpaceElement::bbox(&self.0.window);

        // if self.decoration_state().is_ssd && !window_bbox.is_empty() {
        //     let window_geo = SpaceElement::geometry(&self.window);

        //     let mut state = self.decoration_state();
        //     let width = window_geo.size.w;
        //     state.header_bar.redraw(width as u32);
        //     let mut vec = AsRenderElements::<R>::render_elements::<WindowRenderElement<R>>(
        //         &state.header_bar,
        //         renderer,
        //         location,
        //         scale,
        //         alpha,
        //     );

        //     location.y += (scale.y * HEADER_BAR_HEIGHT as f64) as i32;

        //     let window_elements =
        //         AsRenderElements::render_elements(&self.window, renderer, location, scale, alpha);
        //     vec.extend(window_elements);
        //     vec.into_iter().map(C::from).collect()
        // } else {
        AsRenderElements::render_elements(&self.0.window, renderer, location, scale, alpha)
            .into_iter()
            .map(C::from)
            .collect()
        // }
    }
}
