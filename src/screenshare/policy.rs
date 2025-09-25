#![allow(dead_code)]

use std::collections::HashSet;

use crate::screenshare::frame_tap::OutputId;

/// Identifier for a window/toplevel that could be recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToplevelId(pub u64);

/// Target of an active screencast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastTarget {
    Output(OutputId),
    Window(ToplevelId),
    None,
}

impl Default for CastTarget {
    fn default() -> Self {
        CastTarget::None
    }
}

/// Runtime policy describing which windows are blocked and the current cast target.
#[derive(Default, Debug)]
pub struct ScreencastPolicy {
    blocked: HashSet<ToplevelId>,
    target: CastTarget,
}

impl ScreencastPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_surface_screencast_blocked(&mut self, surface: ToplevelId, blocked: bool) {
        if blocked {
            self.blocked.insert(surface);
        } else {
            self.blocked.remove(&surface);
        }
    }

    pub fn is_surface_blocked(&self, surface: &ToplevelId) -> bool {
        self.blocked.contains(surface)
    }

    pub fn cast_target(&self) -> &CastTarget {
        &self.target
    }

    pub fn set_cast_target(&mut self, target: CastTarget) {
        self.target = target;
    }
}
