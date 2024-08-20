use std::{borrow::Cow, fmt::Debug, hash::Hash, sync::{Arc, RwLock}};

use smithay::{desktop::WindowSurface, input::{pointer::{
    GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
    GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
}, touch::TouchTarget}, reexports::wayland_server::protocol::wl_surface::WlSurface};
pub use smithay::{
    backend::input::KeyState,
    desktop::{LayerSurface, PopupKind},
    input::{
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, PointerTarget, RelativeMotionEvent},
        Seat,
    },
    reexports::wayland_server::{backend::ObjectId, Resource},
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
    xwayland::X11Surface,
};
use wayland_server::protocol::wl_surface;

use crate::{
    interactive_view::InteractiveView, shell::WindowElement, state::{Backend, ScreenComposer}, workspace::{AppSwitcherView, WindowSelectorView}
};

pub enum KeyboardFocusTarget<Backend: crate::state::Backend> {
    Window(WindowElement),
    LayerSurface(LayerSurface),
    Popup(PopupKind),
    View(InteractiveView<Backend>)
}

impl<Backend: crate::state::Backend> PartialEq for KeyboardFocusTarget<Backend> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (KeyboardFocusTarget::Window(w1), KeyboardFocusTarget::Window(w2)) => w1 == w2,
            (KeyboardFocusTarget::LayerSurface(l1), KeyboardFocusTarget::LayerSurface(l2)) => l1 == l2,
            (KeyboardFocusTarget::Popup(p1), KeyboardFocusTarget::Popup(p2)) => p1 == p2,
            (KeyboardFocusTarget::View(d1), KeyboardFocusTarget::View(d2)) => d1 == d2,
            _ => false,
        }
    }
}
impl<Backend: crate::state::Backend> Clone for KeyboardFocusTarget<Backend> {
    fn clone(&self) -> Self {
        match self {
            KeyboardFocusTarget::Window(w) => KeyboardFocusTarget::Window(w.clone()),
            KeyboardFocusTarget::LayerSurface(l) => KeyboardFocusTarget::LayerSurface(l.clone()),
            KeyboardFocusTarget::Popup(p) => KeyboardFocusTarget::Popup(p.clone()),
            KeyboardFocusTarget::View(d) => KeyboardFocusTarget::View(d.clone()),
        }
    }
}

impl<Backend: crate::state::Backend> Debug for KeyboardFocusTarget<Backend> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyboardFocusTarget::Window(w) => write!(f, "KeyboardFocusTarget::Window({:?})", w),
            KeyboardFocusTarget::LayerSurface(l) => write!(f, "KeyboardFocusTarget::LayerSurface({:?})", l),
            KeyboardFocusTarget::Popup(p) => write!(f, "KeyboardFocusTarget::Popup({:?})", p),
            KeyboardFocusTarget::View(d) => write!(f, "KeyboardFocusTarget::View({:?})", d),
        }
    }
}
impl<Backend: crate::state::Backend> IsAlive for KeyboardFocusTarget<Backend> {
    #[inline]
    fn alive(&self) -> bool {
        match self {
            KeyboardFocusTarget::Window(w) => w.alive(),
            KeyboardFocusTarget::LayerSurface(l) => l.alive(),
            KeyboardFocusTarget::Popup(p) => p.alive(),
            KeyboardFocusTarget::View(d) => d.alive(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointerFocusTarget {
    WlSurface(WlSurface),
    // View(InteractiveView<Backend>),
    #[cfg(feature = "xwayland")]
    X11Surface(X11Surface),
    // SSD(SSD),
}

impl IsAlive for PointerFocusTarget {
    #[inline]
    fn alive(&self) -> bool {
        match self {
            PointerFocusTarget::WlSurface(w) => w.alive(),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => w.alive(),
        }
    }
}

impl From<PointerFocusTarget> for WlSurface {
    #[inline]
    fn from(target: PointerFocusTarget) -> Self {
        target.wl_surface().unwrap().into_owned()
    }
}

impl<BackendData: Backend> PointerTarget<ScreenComposer<BackendData>> for PointerFocusTarget {
    fn enter(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &MotionEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::enter(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::enter(w, seat, data, event),
        }
    }
    fn motion(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &MotionEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::motion(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::motion(w, seat, data, event),
        }
    }
    fn relative_motion(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &RelativeMotionEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::relative_motion(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::relative_motion(w, seat, data, event),
        }
    }
    fn button(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &ButtonEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::button(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::button(w, seat, data, event),
        }
    }
    fn axis(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        frame: AxisFrame,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::axis(w, seat, data, frame),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::axis(w, seat, data, frame),
        }
    }
    fn frame(&self, seat: &Seat<ScreenComposer<BackendData>>, data: &mut ScreenComposer<BackendData>) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::frame(w, seat, data),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::frame(w, seat, data),
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
            PointerFocusTarget::WlSurface(w) => PointerTarget::leave(w, seat, data, serial, time),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::leave(w, seat, data, serial, time),
        }
    }
    fn gesture_swipe_begin(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureSwipeBeginEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_swipe_begin(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_swipe_begin(w, seat, data, event),
        }
    }
    fn gesture_swipe_update(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureSwipeUpdateEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_swipe_update(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_swipe_update(w, seat, data, event),
        }
    }
    fn gesture_swipe_end(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureSwipeEndEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_swipe_end(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_swipe_end(w, seat, data, event),
        }
    }
    fn gesture_pinch_begin(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GesturePinchBeginEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_pinch_begin(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_pinch_begin(w, seat, data, event),
        }
    }
    fn gesture_pinch_update(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GesturePinchUpdateEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_pinch_update(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_pinch_update(w, seat, data, event),
        }
    }
    fn gesture_pinch_end(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GesturePinchEndEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_pinch_end(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_pinch_end(w, seat, data, event),
        }
    }
    fn gesture_hold_begin(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureHoldBeginEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_hold_begin(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_hold_begin(w, seat, data, event),
        }
    }
    fn gesture_hold_end(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &GestureHoldEndEvent,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => PointerTarget::gesture_hold_end(w, seat, data, event),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => PointerTarget::gesture_hold_end(w, seat, data, event),
        }
    }
}

impl<BackendData: Backend> KeyboardTarget<ScreenComposer<BackendData>> for KeyboardFocusTarget<BackendData> {
    fn enter(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        match self {
            KeyboardFocusTarget::Window(w) => match w.underlying_surface() {
                WindowSurface::Wayland(w) => KeyboardTarget::enter(w.wl_surface(), seat, data, keys, serial),
                #[cfg(feature = "xwayland")]
                WindowSurface::X11(s) => KeyboardTarget::enter(s, seat, data, keys, serial),
            },
            KeyboardFocusTarget::LayerSurface(l) => {
                KeyboardTarget::enter(l.wl_surface(), seat, data, keys, serial)
            }
            KeyboardFocusTarget::Popup(p) => KeyboardTarget::enter(p.wl_surface(), seat, data, keys, serial),
            KeyboardFocusTarget::View(d) => KeyboardTarget::enter(d, seat, data, keys, serial),
        }
    }
    fn leave(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        serial: Serial,
    ) {
        match self {
        KeyboardFocusTarget::Window(w) => match w.underlying_surface() {
            WindowSurface::Wayland(w) => KeyboardTarget::leave(w.wl_surface(), seat, data, serial),
            #[cfg(feature = "xwayland")]
            WindowSurface::X11(s) => KeyboardTarget::leave(s, seat, data, serial),
        },
        KeyboardFocusTarget::LayerSurface(l) => KeyboardTarget::leave(l.wl_surface(), seat, data, serial),
        KeyboardFocusTarget::Popup(p) => KeyboardTarget::leave(p.wl_surface(), seat, data, serial),
        KeyboardFocusTarget::View(d) => KeyboardTarget::leave(d, seat, data, serial),
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
            KeyboardFocusTarget::Window(w) => match w.underlying_surface() {
                WindowSurface::Wayland(w) => {
                    KeyboardTarget::key(w.wl_surface(), seat, data, key, state, serial, time)
                }
                #[cfg(feature = "xwayland")]
                WindowSurface::X11(s) => KeyboardTarget::key(s, seat, data, key, state, serial, time),
            },
            KeyboardFocusTarget::LayerSurface(l) => {
                KeyboardTarget::key(l.wl_surface(), seat, data, key, state, serial, time)
            }
            KeyboardFocusTarget::Popup(p) => {
                KeyboardTarget::key(p.wl_surface(), seat, data, key, state, serial, time)
            }
            KeyboardFocusTarget::View(d) => KeyboardTarget::key(d, seat, data, key, state, serial, time),
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
            KeyboardFocusTarget::Window(w) => match w.underlying_surface() {
                WindowSurface::Wayland(w) => {
                    KeyboardTarget::modifiers(w.wl_surface(), seat, data, modifiers, serial)
                }
                #[cfg(feature = "xwayland")]
                WindowSurface::X11(s) => KeyboardTarget::modifiers(s, seat, data, modifiers, serial),
            },
            KeyboardFocusTarget::LayerSurface(l) => {
                KeyboardTarget::modifiers(l.wl_surface(), seat, data, modifiers, serial)
            }
            KeyboardFocusTarget::Popup(p) => {
                KeyboardTarget::modifiers(p.wl_surface(), seat, data, modifiers, serial)
            }
            KeyboardFocusTarget::View(d) => KeyboardTarget::modifiers(d, seat, data, modifiers, serial),
        }
    }
}

impl<BackendData: Backend> TouchTarget<ScreenComposer<BackendData>> for PointerFocusTarget {
    fn down(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &smithay::input::touch::DownEvent,
        seq: Serial,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::down(w, seat, data, event, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::down(w, seat, data, event, seq),
        }
    }

    fn up(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &smithay::input::touch::UpEvent,
        seq: Serial,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::up(w, seat, data, event, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::up(w, seat, data, event, seq),
        }
    }

    fn motion(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &smithay::input::touch::MotionEvent,
        seq: Serial,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::motion(w, seat, data, event, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::motion(w, seat, data, event, seq),
        }
    }

    fn frame(&self, seat: &Seat<ScreenComposer<BackendData>>, data: &mut ScreenComposer<BackendData>, seq: Serial) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::frame(w, seat, data, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::frame(w, seat, data, seq),
        }
    }

    fn cancel(&self, seat: &Seat<ScreenComposer<BackendData>>, data: &mut ScreenComposer<BackendData>, seq: Serial) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::cancel(w, seat, data, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::cancel(w, seat, data, seq),
        }
    }

    fn shape(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &smithay::input::touch::ShapeEvent,
        seq: Serial,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::shape(w, seat, data, event, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::shape(w, seat, data, event, seq),
        }
    }

    fn orientation(
        &self,
        seat: &Seat<ScreenComposer<BackendData>>,
        data: &mut ScreenComposer<BackendData>,
        event: &smithay::input::touch::OrientationEvent,
        seq: Serial,
    ) {
        match self {
            PointerFocusTarget::WlSurface(w) => TouchTarget::orientation(w, seat, data, event, seq),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => TouchTarget::orientation(w, seat, data, event, seq),
        }
    }
}

impl WaylandFocus for PointerFocusTarget {
    #[inline]
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            PointerFocusTarget::WlSurface(w) => w.wl_surface(),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => w.wl_surface().map(Cow::Owned),
        }
    }
    #[inline]
    fn same_client_as(&self, object_id: &ObjectId) -> bool {
        match self {
            PointerFocusTarget::WlSurface(w) => w.same_client_as(object_id),
            #[cfg(feature = "xwayland")]
            PointerFocusTarget::X11Surface(w) => w.same_client_as(object_id),
        }
    }
}

impl<BackendData: Backend> WaylandFocus for KeyboardFocusTarget<BackendData> {
    #[inline]
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            KeyboardFocusTarget::Window(w) => w.wl_surface(),
            KeyboardFocusTarget::LayerSurface(l) => Some(Cow::Borrowed(l.wl_surface())),
            KeyboardFocusTarget::Popup(p) => Some(Cow::Borrowed(p.wl_surface())),
            KeyboardFocusTarget::View(d) => None,
        }
    }
}

impl From<WindowElement> for PointerFocusTarget {
    fn from(w: WindowElement) -> Self {
        match w.underlying_surface() {
            WindowSurface::Wayland(w) => PointerFocusTarget::from(w.wl_surface()),
            #[cfg(feature = "xwayland")]
            WindowSurface::X11(s) => PointerFocusTarget::X11Surface(s.clone()),
        }
    }
}

impl<BackendData: Backend> From<WindowElement> for KeyboardFocusTarget<BackendData> {
    fn from(w: WindowElement) -> Self {
        KeyboardFocusTarget::Window(w)
    }
}

impl From<WlSurface> for PointerFocusTarget {
    #[inline]
    fn from(value: WlSurface) -> Self {
        PointerFocusTarget::WlSurface(value)
    }
}

impl From<&WlSurface> for PointerFocusTarget {
    #[inline]
    fn from(value: &WlSurface) -> Self {
        PointerFocusTarget::from(value.clone())
    }
}
impl From<LayerSurface> for PointerFocusTarget {
    fn from(l: LayerSurface) -> Self {
        PointerFocusTarget::from(l.wl_surface())
    }
}

impl<BackendData: Backend> From<LayerSurface> for KeyboardFocusTarget<BackendData> {
    fn from(w: LayerSurface) -> Self {
        KeyboardFocusTarget::LayerSurface(w)
    }
}
impl From<PopupKind> for PointerFocusTarget {
    fn from(p: PopupKind) -> Self {
        PointerFocusTarget::from(p.wl_surface())
    }
}
impl<BackendData: Backend> From<PopupKind> for KeyboardFocusTarget<BackendData> {
    fn from(p: PopupKind) -> Self {
        KeyboardFocusTarget::Popup(p)
    }
}

impl<BackendData: Backend>  From<KeyboardFocusTarget<BackendData>> for PointerFocusTarget {
    fn from(k: KeyboardFocusTarget<BackendData>) -> Self {
        PointerFocusTarget::from(&*k.wl_surface().unwrap())
    }
}
// impl<S:Hash + Clone + 'static> From<layers::prelude::View<S>> for PointerFocusTarget
// where Arc<RwLock<S>>: Send + Sync {
//     fn from(value: layers::prelude::View<S>) -> Self {
//         let view = value.clone();
//         let d = InteractiveView { view: Box::new(view) };
//         PointerFocusTarget::View(d)
//     }
// }

// impl From<WindowSelectorView> for PointerFocusTarget {
//     fn from(value: WindowSelectorView) -> Self {
//         let view = value.clone();
//         let d = InteractiveView { view: Box::new(view) };
//         PointerFocusTarget::View(d)
//     }
// }

// impl From<AppSwitcherView> for PointerFocusTarget {
//     fn from(value: AppSwitcherView) -> Self {
//         let view = value.clone();
//         let d = InteractiveView { view: Box::new(view) };
//         PointerFocusTarget::View(d)
//     }
// }