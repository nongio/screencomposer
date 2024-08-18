use std::{hash::Hash, sync::{Arc, RwLock}};

use smithay::input::pointer::{
    GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
    GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
};
pub use smithay::{
    backend::input::KeyState,
    desktop::{LayerSurface, PopupKind},
    input::{
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, PointerTarget, RelativeMotionEvent},
        Seat,
    },
    reexports::wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
};

use crate::{
    interactive_view::InteractiveView, shell::WindowElement, state::{Backend, ScreenComposer}, workspace::{AppSwitcherView, WindowSelectorView}
};

pub enum FocusTarget<Backend: crate::state::Backend> {
    Window(WindowElement),
    LayerSurface(LayerSurface),
    Popup(PopupKind),
    View(InteractiveView<Backend>)
    // LayerView(layers::prelude::ViewLayer)
}
impl<Backend: crate::state::Backend> Clone for FocusTarget<Backend> {
    fn clone(&self) -> Self {
        match self {
            FocusTarget::Window(w) => FocusTarget::Window(w.clone()),
            FocusTarget::LayerSurface(l) => FocusTarget::LayerSurface(l.clone()),
            FocusTarget::Popup(p) => FocusTarget::Popup(p.clone()),
            FocusTarget::View(d) => FocusTarget::View(d.clone()),
        }
    }
} 
impl<Backend: crate::state::Backend> PartialEq for FocusTarget<Backend> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FocusTarget::Window(w1), FocusTarget::Window(w2)) => w1 == w2,
            (FocusTarget::LayerSurface(l1), FocusTarget::LayerSurface(l2)) => l1 == l2,
            (FocusTarget::Popup(p1), FocusTarget::Popup(p2)) => p1 == p2,
            (FocusTarget::View(d1), FocusTarget::View(d2)) => d1 == d2,
            _ => false,
        }
    }
}
impl<Backend: crate::state::Backend> std::fmt::Debug for FocusTarget<Backend> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusTarget::Window(w) => write!(f, "FocusTarget::Window({:?})", w),
            FocusTarget::LayerSurface(l) => write!(f, "FocusTarget::LayerSurface({:?})", l),
            FocusTarget::Popup(p) => write!(f, "FocusTarget::Popup({:?})", p),
            FocusTarget::View(d) => write!(f, "FocusTarget::View({:?})", d),
        }
    }
}
impl<Backend: crate::state::Backend> IsAlive for FocusTarget<Backend> {
    fn alive(&self) -> bool {
        match self {
            FocusTarget::Window(w) => w.alive(),
            FocusTarget::LayerSurface(l) => l.alive(),
            FocusTarget::Popup(p) => p.alive(),
            FocusTarget::View(d) => d.alive(),
            // FocusTarget::LayerView(l) => true,
        }
    }
}

impl<Backend: crate::state::Backend> From<FocusTarget<Backend>> for WlSurface {
    fn from(target: FocusTarget<Backend>) -> Self {
        target.wl_surface().unwrap()
    }
}

impl<BackendData: Backend> PointerTarget<ScreenComposer<BackendData>> for FocusTarget<BackendData> {
    fn enter(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &MotionEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::enter(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::enter(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::enter(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::enter(d, seat, data, event),

        }
    }
    fn motion(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &MotionEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::motion(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::motion(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::motion(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::motion(d, seat, data, event),
        }
    }
    fn relative_motion(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &RelativeMotionEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::relative_motion(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::relative_motion(l.wl_surface(), seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::relative_motion(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::relative_motion(d, seat, data, event),
        }
    }
    fn button(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &ButtonEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::button(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::button(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::button(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::button(d, seat, data, event),
        }
    }
    fn axis(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        frame: AxisFrame,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::axis(w, seat, data, frame),
            FocusTarget::LayerSurface(l) => PointerTarget::axis(l, seat, data, frame),
            FocusTarget::Popup(p) => PointerTarget::axis(p.wl_surface(), seat, data, frame),
            FocusTarget::View(d) => PointerTarget::axis(d, seat, data, frame),
        }
    }
    fn frame(&self, seat: &Seat<ScreenComposer<BackendData>>, data: &mut ScreenComposer<BackendData>) {
        match self {
            FocusTarget::Window(w) => PointerTarget::frame(w, seat, data),
            FocusTarget::LayerSurface(l) => PointerTarget::frame(l, seat, data),
            FocusTarget::Popup(p) => PointerTarget::frame(p.wl_surface(), seat, data),
            FocusTarget::View(d) => PointerTarget::frame(d, seat, data),
        }
    }
    fn leave(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        serial: Serial,
        time: u32,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::leave(w, seat, data, serial, time),
            FocusTarget::LayerSurface(l) => PointerTarget::leave(l, seat, data, serial, time),
            FocusTarget::Popup(p) => PointerTarget::leave(p.wl_surface(), seat, data, serial, time),
            FocusTarget::View(d) => PointerTarget::leave(d, seat, data, serial, time),
        }
    }
    fn gesture_swipe_begin(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureSwipeBeginEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_swipe_begin(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_swipe_begin(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_swipe_begin(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_swipe_begin(d, seat, data, event),
        }
    }
    fn gesture_swipe_update(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureSwipeUpdateEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_swipe_update(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_swipe_update(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_swipe_update(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_swipe_update(d, seat, data, event),
        }
    }
    fn gesture_swipe_end(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureSwipeEndEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_swipe_end(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_swipe_end(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_swipe_end(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_swipe_end(d, seat, data, event),
        }
    }
    fn gesture_pinch_begin(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GesturePinchBeginEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_pinch_begin(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_pinch_begin(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_pinch_begin(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_pinch_begin(d, seat, data, event),
        }
    }
    fn gesture_pinch_update(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GesturePinchUpdateEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_pinch_update(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_pinch_update(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_pinch_update(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_pinch_update(d, seat, data, event),
        }
    }
    fn gesture_pinch_end(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GesturePinchEndEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_pinch_end(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_pinch_end(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_pinch_end(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_pinch_end(d, seat, data, event),
        }
    }
    fn gesture_hold_begin(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureHoldBeginEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_hold_begin(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_hold_begin(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_hold_begin(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_hold_begin(d, seat, data, event),
        }
    }
    fn gesture_hold_end(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureHoldEndEvent,
    ) {
        match self {
            FocusTarget::Window(w) => PointerTarget::gesture_hold_end(w, seat, data, event),
            FocusTarget::LayerSurface(l) => PointerTarget::gesture_hold_end(l, seat, data, event),
            FocusTarget::Popup(p) => PointerTarget::gesture_hold_end(p.wl_surface(), seat, data, event),
            FocusTarget::View(d) => PointerTarget::gesture_hold_end(d, seat, data, event),
        }
    }
}

impl<BackendData: Backend> KeyboardTarget<ScreenComposer<BackendData>> for FocusTarget<BackendData> {
    fn enter(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        match self {
            FocusTarget::Window(w) => KeyboardTarget::enter(w, seat, data, keys, serial),
            FocusTarget::LayerSurface(l) => KeyboardTarget::enter(l, seat, data, keys, serial),
            FocusTarget::Popup(p) => KeyboardTarget::enter(p.wl_surface(), seat, data, keys, serial),
            FocusTarget::View(d) => KeyboardTarget::enter(d, seat, data, keys, serial),
        }
    }
    fn leave(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        serial: Serial,
    ) {
        match self {
            FocusTarget::Window(w) => KeyboardTarget::leave(w, seat, data, serial),
            FocusTarget::LayerSurface(l) => KeyboardTarget::leave(l, seat, data, serial),
            FocusTarget::Popup(p) => KeyboardTarget::leave(p.wl_surface(), seat, data, serial),
            FocusTarget::View(d) => KeyboardTarget::leave(d, seat, data, serial),
        }
    }
    fn key(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        match self {
            FocusTarget::Window(w) => KeyboardTarget::key(w, seat, data, key, state, serial, time),
            FocusTarget::LayerSurface(l) => KeyboardTarget::key(l, seat, data, key, state, serial, time),
            FocusTarget::Popup(p) => {
                KeyboardTarget::key(p.wl_surface(), seat, data, key, state, serial, time)
            },
            FocusTarget::View(d) => KeyboardTarget::key(d, seat, data, key, state, serial, time),
        }
    }
    fn modifiers(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        match self {
            FocusTarget::Window(w) => KeyboardTarget::modifiers(w, seat, data, modifiers, serial),
            FocusTarget::LayerSurface(l) => KeyboardTarget::modifiers(l, seat, data, modifiers, serial),
            FocusTarget::Popup(p) => KeyboardTarget::modifiers(p.wl_surface(), seat, data, modifiers, serial),
            FocusTarget::View(d) => KeyboardTarget::modifiers(d, seat, data, modifiers, serial),
        }
    }
}

impl<Backend: crate::state::Backend> WaylandFocus for FocusTarget<Backend> {
    fn wl_surface(&self) -> Option<WlSurface> {
        match self {
            FocusTarget::Window(w) => w.wl_surface(),
            FocusTarget::LayerSurface(l) => Some(l.wl_surface().clone()),
            FocusTarget::Popup(p) => Some(p.wl_surface().clone()),
            FocusTarget::View(_) => None,
        }
    }
    fn same_client_as(&self, object_id: &ObjectId) -> bool {
        match self {
            FocusTarget::Window(WindowElement::Wayland(w)) => w.same_client_as(object_id),
            #[cfg(feature = "xwayland")]
            FocusTarget::Window(WindowElement::X11(w)) => w.same_client_as(object_id),
            FocusTarget::LayerSurface(l) => l.wl_surface().id().same_client_as(object_id),
            FocusTarget::Popup(p) => p.wl_surface().id().same_client_as(object_id),
            FocusTarget::View(_) => false,
        }
    }
}

impl<Backend: crate::state::Backend> From<WindowElement> for FocusTarget<Backend> {
    fn from(w: WindowElement) -> Self {
        FocusTarget::Window(w)
    }
}

impl<Backend: crate::state::Backend> From<LayerSurface> for FocusTarget<Backend> {
    fn from(l: LayerSurface) -> Self {
        FocusTarget::LayerSurface(l)
    }
}

impl<Backend: crate::state::Backend> From<PopupKind> for FocusTarget<Backend> {
    fn from(p: PopupKind) -> Self {
        FocusTarget::Popup(p)
    }
}

impl<S:Hash + Clone + 'static, Backend: crate::state::Backend> From<layers::prelude::View<S>> for FocusTarget<Backend>
where Arc<RwLock<S>>: Send + Sync {
    fn from(value: layers::prelude::View<S>) -> Self {
        let view = value.clone();
        let d = InteractiveView { view: Box::new(view) };
        FocusTarget::View(d)
    }
}

impl<Backend: crate::state::Backend> From<WindowSelectorView> for FocusTarget<Backend> {
    fn from(value: WindowSelectorView) -> Self {
        let view = value.clone();
        let d = InteractiveView { view: Box::new(view) };
        FocusTarget::View(d)
    }
}

impl<Backend: crate::state::Backend> From<AppSwitcherView> for FocusTarget<Backend> {
    fn from(value: AppSwitcherView) -> Self {
        let view = value.clone();
        let d = InteractiveView { view: Box::new(view) };
        FocusTarget::View(d)
    }
}