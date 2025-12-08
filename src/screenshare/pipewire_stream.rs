//! PipeWire stream management for screencast.
//!
//! This module handles the PipeWire side of screen casting:
//! - Creating and configuring PipeWire streams
//! - Buffer management (DMA-BUF and SHM)
//! - Feeding frames from the compositor to PipeWire
//!
//! ## Buffer Types
//!
//! - **DMA-BUF (preferred)**: Zero-copy GPU buffer sharing. The compositor's
//!   render buffer is directly shared with consumers like OBS.
//! - **SHM (fallback)**: CPU memory buffers for consumers that don't support
//!   DMA-BUF.
//!
//! ## Damage Hints
//!
//! Damage rectangles are communicated via `SPA_META_REGION` on each buffer,
//! allowing consumers to do partial updates.

use tokio::sync::mpsc;

use super::session_tap::{FrameData, FrameMetaSnapshot};

/// Configuration for a PipeWire stream.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Framerate numerator (e.g., 60 for 60fps).
    pub framerate_num: u32,
    /// Framerate denominator (usually 1).
    pub framerate_denom: u32,
    /// Pixel format (FourCC code).
    pub format: u32,
    /// Whether to prefer DMA-BUF over SHM.
    pub prefer_dmabuf: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate_num: 60,
            framerate_denom: 1,
            format: 0x34325241, // ARGB8888 as FourCC
            prefer_dmabuf: true,
        }
    }
}

/// A PipeWire stream for screen casting.
///
/// This manages the PipeWire stream lifecycle and buffer handling.
pub struct PipeWireStream {
    /// The PipeWire node ID for this stream.
    node_id: u32,
    /// Stream configuration.
    config: StreamConfig,
    /// Receiver for frames from the compositor.
    frame_receiver: Option<mpsc::Receiver<FrameData>>,
    /// Whether the stream is active.
    active: bool,
}

impl PipeWireStream {
    /// Create a new PipeWire stream.
    ///
    /// Returns the stream and a sender for frames.
    pub fn new(config: StreamConfig) -> (Self, mpsc::Sender<FrameData>) {
        // Create a bounded channel for frames (buffer up to 3 frames)
        let (sender, receiver) = mpsc::channel(3);

        let stream = Self {
            node_id: 0, // Will be set when stream is started
            config,
            frame_receiver: Some(receiver),
            active: false,
        };

        (stream, sender)
    }

    /// Start the PipeWire stream.
    ///
    /// This initializes the PipeWire connection and creates the stream.
    /// Returns the PipeWire node ID.
    pub async fn start(&mut self) -> Result<u32, PipeWireError> {
        if self.active {
            return Err(PipeWireError::AlreadyActive);
        }

        // TODO: Actually initialize PipeWire
        // For now, we'll use a placeholder implementation
        tracing::info!(
            "Starting PipeWire stream: {}x{} @ {}/{}fps",
            self.config.width,
            self.config.height,
            self.config.framerate_num,
            self.config.framerate_denom
        );

        // Placeholder node ID - in real implementation, this comes from PipeWire
        self.node_id = rand::random::<u32>() & 0xFFFF;
        self.active = true;

        Ok(self.node_id)
    }

    /// Stop the PipeWire stream.
    pub async fn stop(&mut self) -> Result<(), PipeWireError> {
        if !self.active {
            return Err(PipeWireError::NotActive);
        }

        tracing::info!("Stopping PipeWire stream (node {})", self.node_id);
        self.active = false;

        Ok(())
    }

    /// Get the PipeWire node ID.
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Get the stream configuration.
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Check if the stream is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Take ownership of the frame receiver for the stream pump loop.
    pub fn take_frame_receiver(&mut self) -> Option<mpsc::Receiver<FrameData>> {
        self.frame_receiver.take()
    }

    /// Run the stream pump loop.
    ///
    /// This should be run on a dedicated thread/task. It receives frames from
    /// the compositor and sends them to PipeWire.
    pub async fn pump_loop(
        mut receiver: mpsc::Receiver<FrameData>,
        node_id: u32,
    ) -> Result<(), PipeWireError> {
        tracing::debug!("Starting PipeWire pump loop for node {}", node_id);

        while let Some(frame) = receiver.recv().await {
            match frame {
                FrameData::DmaBuf { dmabuf, meta } => {
                    Self::handle_dmabuf_frame(&dmabuf, &meta, node_id).await?;
                }
                FrameData::Rgba { data, meta } => {
                    Self::handle_rgba_frame(&data, &meta, node_id).await?;
                }
            }
        }

        tracing::debug!("PipeWire pump loop ended for node {}", node_id);
        Ok(())
    }

    /// Handle a DMA-BUF frame.
    async fn handle_dmabuf_frame(
        dmabuf: &smithay::backend::allocator::dmabuf::Dmabuf,
        meta: &FrameMetaSnapshot,
        node_id: u32,
    ) -> Result<(), PipeWireError> {
        // TODO: Actually send to PipeWire
        // 1. Get a buffer from PipeWire's buffer pool
        // 2. Import the DMA-BUF into the PipeWire buffer
        // 3. Set damage region metadata (SPA_META_REGION)
        // 4. Queue the buffer

        tracing::trace!(
            "PipeWire node {}: dmabuf frame {}x{}, damage={}",
            node_id,
            meta.size.0,
            meta.size.1,
            meta.has_damage
        );

        if let Some(damage) = &meta.damage_rects {
            tracing::trace!(
                "  {} damage rects: {:?}",
                damage.len(),
                damage.first()
            );
        }

        Ok(())
    }

    /// Handle an RGBA frame.
    async fn handle_rgba_frame(
        data: &[u8],
        meta: &FrameMetaSnapshot,
        node_id: u32,
    ) -> Result<(), PipeWireError> {
        // TODO: Actually send to PipeWire
        // 1. Get a SHM buffer from PipeWire's buffer pool
        // 2. Copy the RGBA data into the buffer
        // 3. Set damage region metadata (SPA_META_REGION)
        // 4. Queue the buffer

        tracing::trace!(
            "PipeWire node {}: RGBA frame {}x{}, {} bytes, damage={}",
            node_id,
            meta.size.0,
            meta.size.1,
            data.len(),
            meta.has_damage
        );

        Ok(())
    }
}

/// Errors from PipeWire operations.
#[derive(Debug, thiserror::Error)]
pub enum PipeWireError {
    #[error("Stream is already active")]
    AlreadyActive,

    #[error("Stream is not active")]
    NotActive,

    #[error("PipeWire initialization failed: {0}")]
    InitFailed(String),

    #[error("Buffer allocation failed: {0}")]
    BufferError(String),

    #[error("Stream error: {0}")]
    StreamError(String),
}
