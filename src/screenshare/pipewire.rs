#![allow(dead_code)]

use crate::screenshare::frame_tap::OutputId;
use thiserror::Error;

#[derive(Debug)]
pub struct PipeWirePublisher {
    name: String,
    resolution: (u32, u32),
    fps: (u32, u32),
}

impl PipeWirePublisher {
    pub fn new(name: &str, w: u32, h: u32, fps: (u32, u32)) -> Result<Self, PipeWireError> {
        Ok(Self {
            name: name.to_string(),
            resolution: (w, h),
            fps,
        })
    }

    pub fn start_with_output(&mut self, _out: OutputId) -> Result<(), PipeWireError> {
        Err(PipeWireError::Unimplemented)
    }

    pub fn stop(self) -> Result<(), PipeWireError> {
        Err(PipeWireError::Unimplemented)
    }
}

#[derive(Debug, Error)]
pub enum PipeWireError {
    #[error("PipeWire streaming is not yet implemented")]
    Unimplemented,
}
