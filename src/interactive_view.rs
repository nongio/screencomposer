use std::{hash::Hash, sync::{Arc, RwLock}};

use smithay::{input::{keyboard::KeyboardTarget, pointer::PointerTarget}, utils::IsAlive};

use crate::ScreenComposer;


pub trait ViewInteractions<Backend: crate::state::Backend>: Sync + Send {
    fn id(&self) -> Option<usize>;
    fn is_alive(&self) -> bool;
    fn on_motion(&self, 
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>, 
        _event: &smithay::input::pointer::MotionEvent) {}
    fn on_relative_motion(&self, _event: &smithay::input::pointer::RelativeMotionEvent) {}
    fn on_button(&self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        _event: &smithay::input::pointer::ButtonEvent) {}
    fn on_axis(&self, _event: &smithay::input::pointer::AxisFrame) {}
    fn on_enter(&self, _event: &smithay::input::pointer::MotionEvent) {}
    fn on_leave(&self, _serial: smithay::utils::Serial, _time: u32) {}
    fn on_frame(&self) {}
    fn on_gesture_hold_begin(&self, _event: &smithay::input::pointer::GestureHoldBeginEvent) {}
    fn on_gesture_hold_end(&self, _event: &smithay::input::pointer::GestureHoldEndEvent) {}
    fn on_gesture_pinch_begin(&self, _event: &smithay::input::pointer::GesturePinchBeginEvent) {}
    fn on_gesture_pinch_end(&self, _event: &smithay::input::pointer::GesturePinchEndEvent) {}
    fn on_gesture_pinch_update(&self, _event: &smithay::input::pointer::GesturePinchUpdateEvent) {}
    fn on_gesture_swipe_begin(&self, _event: &smithay::input::pointer::GestureSwipeBeginEvent) {}
    fn on_gesture_swipe_end(&self, _event: &smithay::input::pointer::GestureSwipeEndEvent) {}
    fn on_gesture_swipe_update(&self, _event: &smithay::input::pointer::GestureSwipeUpdateEvent) {}
    fn on_key(&self, _event: &smithay::input::keyboard::KeysymHandle<'_>) {}
    fn on_modifiers(&self, _modifiers: smithay::input::keyboard::ModifiersState) {}
}
pub trait CloneBoxInteractions<Backend: crate::state::Backend>: ViewInteractions<Backend> {
    fn clone_box(&self) -> Box<dyn CloneBoxInteractions<Backend>>;
}

impl<T, Backend: crate::state::Backend> CloneBoxInteractions<Backend> for T
where
    T: 'static + ViewInteractions<Backend> + Clone,
{
    fn clone_box(&self) -> Box<dyn CloneBoxInteractions<Backend>> {
        Box::new(self.clone())
    }
}

pub struct InteractiveView<Backend: crate::state::Backend> {
    pub view: Box<dyn CloneBoxInteractions<Backend>>,
}

impl<Backend: crate::state::Backend> Clone for InteractiveView<Backend> {
    fn clone(&self) -> Self {
        InteractiveView {
            view: self.view.clone_box(),
        }
    }
}

impl<Backend: crate::state::Backend> std::fmt::Debug for InteractiveView<Backend> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractiveView")
            .field("id", &self.view.id())
            .finish()
    }
}

impl<Backend: crate::state::Backend> PartialEq for InteractiveView<Backend> {
    fn eq(&self, other: &Self) -> bool {
        self.view.id() == other.view.id()
    }
}
impl<S:Hash + Clone + 'static, Backend: crate::state::Backend> ViewInteractions<Backend> for layers::prelude::View<S> where 
 Arc<RwLock<S>>: Send + Sync {
    fn id(&self) -> Option<usize> {
        self.layer.id().map(|id| id.0.into())
    }
    fn is_alive(&self) -> bool {
        !self.layer.hidden()
    }
}

impl<Backend: crate::state::Backend> IsAlive for InteractiveView<Backend> {
    fn alive(&self) -> bool {
        self.view.is_alive()
    }
}


impl<Backend: crate::state::Backend> PointerTarget<ScreenComposer<Backend>> for InteractiveView<Backend> {
    fn axis(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        frame: smithay::input::pointer::AxisFrame,
    ) {
        self.view.on_axis(&frame);
    }
    fn button(
        &self,
        seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::ButtonEvent,
    ) {
        self.view.on_button(seat, data, event);
    }
    fn enter(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        self.view.on_enter(event);
    }
    fn frame(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
    ) {
        self.view.on_frame();
    }
    fn gesture_hold_begin(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GestureHoldBeginEvent,
    ) {
        self.view.on_gesture_hold_begin(event);
    }
    fn gesture_hold_end(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GestureHoldEndEvent,
    ) {
        self.view.on_gesture_hold_end(event);
    }
    fn gesture_pinch_begin(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GesturePinchBeginEvent,
    ) {
        self.view.on_gesture_pinch_begin(event);
    }
    fn gesture_pinch_end(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GesturePinchEndEvent,
    ) {
        self.view.on_gesture_pinch_end(event);
    }
    fn gesture_pinch_update(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GesturePinchUpdateEvent,
    ) {
        self.view.on_gesture_pinch_update(event);
    }
    fn gesture_swipe_begin(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GestureSwipeBeginEvent,
    ) {
        self.view.on_gesture_swipe_begin(event);
    }
    fn gesture_swipe_end(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GestureSwipeEndEvent,
    ) {
        self.view.on_gesture_swipe_end(event);
    }
    fn gesture_swipe_update(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::GestureSwipeUpdateEvent,
    ) {
        self.view.on_gesture_swipe_update(event);
    }
    fn leave(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        serial: smithay::utils::Serial,
        time: u32,
    ) {
        self.view.on_leave(serial, time);
    }
    fn motion(
        &self,
        seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        self.view.on_motion(seat, data, event);
    }
    fn relative_motion(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        event: &smithay::input::pointer::RelativeMotionEvent,
    ) {
        self.view.on_relative_motion(event);
    }
}

impl<Backend: crate::state::Backend> KeyboardTarget<ScreenComposer<Backend>> for InteractiveView<Backend> {
    fn enter(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        _keys: Vec<smithay::input::keyboard::KeysymHandle<'_>>,
        _serial: smithay::utils::Serial,
    ) {
        
    }
    fn key(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        key: smithay::input::keyboard::KeysymHandle<'_>,
        _state: smithay::backend::input::KeyState,
        _serial: smithay::utils::Serial,
        _time: u32,
    ) {
        self.view.on_key(&key);
    }
    fn leave(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        _serial: smithay::utils::Serial,
    ) {
        
    }
    fn modifiers(
        &self,
        _seat: &smithay::input::Seat<ScreenComposer<Backend>>,
        _data: &mut ScreenComposer<Backend>,
        modifiers: smithay::input::keyboard::ModifiersState,
        _serial: smithay::utils::Serial,
    ) {
        self.view.on_modifiers(modifiers);
    }
}
