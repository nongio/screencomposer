#![allow(dead_code)]

use crate::screenshare::frame_tap::{FrameMeta, OutputId};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct ScreenshotRequest {
    pub output: Option<OutputId>,
    pub frame: Option<u32>,
}

#[derive(Debug, Error)]
#[error("headless screenshots are not yet implemented")]
pub struct HeadlessCaptureError;

pub fn capture_screenshot(
    _request: &ScreenshotRequest,
    _meta: &FrameMeta,
) -> Result<(), HeadlessCaptureError> {
    Err(HeadlessCaptureError)
}
