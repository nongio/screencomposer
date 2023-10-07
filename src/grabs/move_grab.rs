use crate::{
    state::{Backend, SurfaceLayer},
    ScreenComposer,
};
use layers::prelude::Point as LayersPoint;
use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{Logical, Point},
};

pub struct MoveSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: PointerGrabStartData<ScreenComposer<BackendData>>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}

impl<BackendData: Backend> PointerGrab<ScreenComposer<BackendData>>
    for MoveSurfaceGrab<BackendData>
{
    fn motion(
        &mut self,
        data: &mut ScreenComposer<BackendData>,
        handle: &mut PointerInnerHandle<'_, ScreenComposer<BackendData>>,
        _focus: Option<(WlSurface, Point<i32, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;
        let sid = self.window.toplevel().wl_surface().id();
        if let Some(SurfaceLayer { layer, .. }) = data.layer_for(&sid) {
            let position = LayersPoint {
                x: (new_location.x as f32),
                y: (new_location.y as f32),
            };

            layer.set_position(position, None);
        }
        data.space
            .map_element(self.window.clone(), new_location.to_i32_round(), true);
    }

    fn relative_motion(
        &mut self,
        data: &mut ScreenComposer<BackendData>,
        handle: &mut PointerInnerHandle<'_, ScreenComposer<BackendData>>,
        focus: Option<(WlSurface, Point<i32, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut ScreenComposer<BackendData>,
        handle: &mut PointerInnerHandle<'_, ScreenComposer<BackendData>>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        // The button is a button code as defined in the
        // Linux kernel's linux/input-event-codes.h header file, e.g. BTN_LEFT.
        const BTN_LEFT: u32 = 0x110;

        if !handle.current_pressed().contains(&BTN_LEFT) {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(data, event.serial, event.time);
        }
    }

    fn axis(
        &mut self,
        data: &mut ScreenComposer<BackendData>,
        handle: &mut PointerInnerHandle<'_, ScreenComposer<BackendData>>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn start_data(&self) -> &PointerGrabStartData<ScreenComposer<BackendData>> {
        &self.start_data
    }
}
