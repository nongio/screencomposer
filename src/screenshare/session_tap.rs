//! Screencast session tap implementation.
//!
//! This module provides `ScreencastSessionTap`, a `FrameTap` implementation that
//! filters frames by output and feeds them to a PipeWire stream for screen casting.

use std::sync::Arc;

use smithay::backend::allocator::dmabuf::Dmabuf;
use tokio::sync::mpsc;

use super::frame_tap::{FrameMeta, FrameTap, OutputId, RgbaFrame};

/// A frame tap that captures frames for a specific output and sends them to PipeWire.
pub struct ScreencastSessionTap {
    /// The session ID (D-Bus object path) this tap belongs to.
    session_id: String,
    /// The output ID to filter for.
    target_output: OutputId,
    /// Sender for frames to the PipeWire stream thread.
    frame_sender: mpsc::Sender<FrameData>,
    /// Whether this tap wants all frames regardless of damage.
    wants_all_frames: bool,
}

/// Frame data sent to the PipeWire stream.
#[derive(Debug)]
pub enum FrameData {
    /// DMA-BUF frame (zero-copy path).
    DmaBuf {
        dmabuf: Dmabuf,
        meta: FrameMetaSnapshot,
    },
    /// RGBA frame (CPU copy path).
    Rgba {
        data: Arc<[u8]>,
        meta: FrameMetaSnapshot,
    },
}

/// Snapshot of frame metadata that can be sent across threads.
#[derive(Debug, Clone)]
pub struct FrameMetaSnapshot {
    pub size: (u32, u32),
    pub stride: u32,
    pub fourcc: u32,
    pub time_ns: u64,
    pub modifier: Option<u64>,
    pub has_damage: bool,
    pub damage_rects: Option<Vec<DamageRect>>,
}

/// A damage rectangle.
#[derive(Debug, Clone, Copy)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ScreencastSessionTap {
    /// Create a new screencast session tap.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session D-Bus path this tap belongs to.
    /// * `target_output` - The output ID to capture frames from.
    /// * `frame_sender` - Channel sender for frames to the PipeWire thread.
    pub fn new(
        session_id: String,
        target_output: OutputId,
        frame_sender: mpsc::Sender<FrameData>,
    ) -> Self {
        Self {
            session_id,
            target_output,
            frame_sender,
            wants_all_frames: false,
        }
    }

    /// Set whether this tap wants all frames regardless of damage.
    pub fn set_wants_all_frames(&mut self, wants: bool) {
        self.wants_all_frames = wants;
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the target output ID.
    pub fn target_output(&self) -> &OutputId {
        &self.target_output
    }

    fn meta_to_snapshot(meta: &FrameMeta) -> FrameMetaSnapshot {
        FrameMetaSnapshot {
            size: meta.size,
            stride: meta.stride,
            fourcc: meta.fourcc as u32,
            time_ns: meta.time_ns,
            modifier: meta.modifier,
            has_damage: meta.has_damage,
            damage_rects: meta.damage.as_ref().map(|rects| {
                rects
                    .iter()
                    .map(|r| DamageRect {
                        x: r.loc.x,
                        y: r.loc.y,
                        width: r.size.w,
                        height: r.size.h,
                    })
                    .collect()
            }),
        }
    }
}

impl FrameTap for ScreencastSessionTap {
    fn wants_all_frames(&self) -> bool {
        self.wants_all_frames
    }

    fn on_frame_dmabuf(&self, out: &OutputId, dmabuf: &Dmabuf, meta: &FrameMeta) {
        // Filter by output
        if out != &self.target_output {
            return;
        }

        // Skip frames with no damage unless we want all frames
        if !meta.has_damage && !self.wants_all_frames {
            tracing::trace!(
                "Skipping dmabuf frame for session {}: no damage",
                self.session_id
            );
            return;
        }

        let snapshot = Self::meta_to_snapshot(meta);

        // Try to send the frame (non-blocking)
        match self.frame_sender.try_send(FrameData::DmaBuf {
            dmabuf: dmabuf.clone(),
            meta: snapshot,
        }) {
            Ok(()) => {
                tracing::trace!(
                    "Sent dmabuf frame to PipeWire for session {}",
                    self.session_id
                );
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    "Frame dropped for session {}: PipeWire channel full",
                    self.session_id
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(
                    "Frame dropped for session {}: PipeWire channel closed",
                    self.session_id
                );
            }
        }
    }

    fn on_frame_rgba(&self, out: &OutputId, frame: &RgbaFrame, meta: &FrameMeta) {
        // Filter by output
        if out != &self.target_output {
            return;
        }

        // Skip frames with no damage unless we want all frames
        if !meta.has_damage && !self.wants_all_frames {
            tracing::trace!(
                "Skipping RGBA frame for session {}: no damage",
                self.session_id
            );
            return;
        }

        let snapshot = Self::meta_to_snapshot(meta);

        // Clone the frame data (it's Arc'd so this is cheap)
        let data = Arc::from(frame.data());

        // Try to send the frame (non-blocking)
        match self.frame_sender.try_send(FrameData::Rgba {
            data,
            meta: snapshot,
        }) {
            Ok(()) => {
                tracing::trace!(
                    "Sent RGBA frame to PipeWire for session {}",
                    self.session_id
                );
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    "Frame dropped for session {}: PipeWire channel full",
                    self.session_id
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(
                    "Frame dropped for session {}: PipeWire channel closed",
                    self.session_id
                );
            }
        }
    }
}
