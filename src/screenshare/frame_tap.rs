//! Frame tap utilities for observing composed frames.
//!
//! This module provides a small, thread-safe observer mechanism ("frame taps")
//! that lets subsystems (e.g., screenshare, diagnostics, tests) receive a copy
//! of each presented frame for a given output. Two delivery paths are supported:
//! - Zero-copy path via DMA-BUF (`on_frame_dmabuf`) when consumers can work with
//!   GPU buffers directly.
//! - CPU-memory path via RGBA bytes (`on_frame_rgba`) when a host-readable copy
//!   is necessary (debug, testing, encoders without DMA-BUF support).
//!
//! ## GPU Synchronization
//!
//! The module now supports GPU synchronization via Smithay's `SyncPoint` API:
//! - `FrameMeta` includes an optional `sync: Option<SyncPoint>` field
//! - Sync-aware methods: `notify_dmabuf_with_sync()`, `notify_rgba_with_sync()`
//! - Compositor passes sync points from `render_frame()` results
//! - PipeWire consumers can delay buffer queueing until GPU rendering completes
//!

use std::{sync::Arc, time::Duration};

use smithay::{
    backend::{
        allocator::{
            dmabuf::{Dmabuf, DmabufMappingMode, DmabufSyncFlags},
            Buffer, Fourcc,
        },
        renderer::{sync::SyncPoint, ExportMem, Renderer},
    },
    output::Output,
    utils::{Physical, Rectangle},
};

/// Logical identifier of an output, used by taps to correlate frames to a sink.
///
/// TODO: The current implementation returns a placeholder string; it should be
/// wired to `Output` identity (e.g., name or persistent UUID) when available.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutputId(pub String);

impl OutputId {
    /// Build an `OutputId` from a Smithay `Output`.
    ///
    /// Currently returns a placeholder value; callers should not rely on its
    /// stability yet. See the `TODO` inside for future wiring.
    pub fn from_output(output: &Output) -> Self {
        Self(output.name())
    }
}

/// Minimal, transport-agnostic description of a frame.
///
/// - `size`: width/height in pixels.
/// - `stride`: bytes per row for plane 0.
/// - `fourcc`: pixel format (e.g., ARGB8888).
/// - `time_ns`: monotonic timestamp in nanoseconds.
/// - `modifier`: buffer modifier if known (e.g., tiling), `None` for linear/unknown.
/// - `sync`: GPU fence for synchronizing buffer access (optional).
/// - `damage`: list of damaged rectangles in physical coordinates.
/// - `has_damage`: quick flag indicating whether any damage exists.
#[derive(Clone, Debug)]
pub struct FrameMeta {
    pub size: (u32, u32),
    pub stride: u32,
    pub fourcc: Fourcc,
    pub time_ns: u64,
    pub modifier: Option<u64>,
    pub sync: Option<SyncPoint>,
    /// Damaged regions in physical coordinates. `None` means full damage.
    pub damage: Option<Vec<Rectangle<i32, Physical>>>,
    /// Quick check for whether any damage exists (false = skip frame).
    pub has_damage: bool,
}

impl Default for FrameMeta {
    fn default() -> Self {
        Self {
            size: (0, 0),
            stride: 0,
            fourcc: Fourcc::Argb8888,
            time_ns: 0,
            modifier: None,
            sync: None,
            damage: None,
            has_damage: false,
        }
    }
}
impl FrameMeta {
    /// Construct metadata from explicit parameters and a `Duration` timestamp.
    pub fn from_params(size: (u32, u32), stride: u32, fourcc: Fourcc, time: Duration) -> Self {
        let time_ns = time.as_nanos().min(u64::MAX as u128) as u64;
        Self {
            size,
            stride,
            fourcc,
            time_ns,
            modifier: None,
            sync: None,
            damage: None,
            has_damage: true, // Assume damage when not explicitly provided
        }
    }

    /// Construct metadata from explicit parameters with optional sync point.
    pub fn from_params_with_sync(
        size: (u32, u32),
        stride: u32,
        fourcc: Fourcc,
        time: Duration,
        sync: Option<SyncPoint>,
    ) -> Self {
        let time_ns = time.as_nanos().min(u64::MAX as u128) as u64;
        Self {
            size,
            stride,
            fourcc,
            time_ns,
            modifier: None,
            sync,
            damage: None,
            has_damage: true, // Assume damage when not explicitly provided
        }
    }

    /// Construct metadata with explicit damage information.
    pub fn from_params_with_damage(
        size: (u32, u32),
        stride: u32,
        fourcc: Fourcc,
        time: Duration,
        sync: Option<SyncPoint>,
        damage: Option<&[Rectangle<i32, Physical>]>,
    ) -> Self {
        let time_ns = time.as_nanos().min(u64::MAX as u128) as u64;
        let has_damage = damage.map_or(true, |d| !d.is_empty());
        Self {
            size,
            stride,
            fourcc,
            time_ns,
            modifier: None,
            sync,
            damage: damage.map(|d| d.to_vec()),
            has_damage,
        }
    }

    /// Construct metadata from a DMA-BUF handle. Uses plane 0 for stride and
    /// converts the hardware format code into Smithay's `Fourcc`.
    pub fn from_dmabuf(dmabuf: &Dmabuf, time: Duration) -> Self {
        let size = dmabuf.size();
        let width = size.w.max(0) as u32;
        let height = size.h.max(0) as u32;
        let stride = dmabuf.strides().next().unwrap_or(0);
        // Convert the dmabuf format code into smithay's Fourcc type
        let fourcc = Fourcc::from(dmabuf.format().code);
        let mut meta = Self::from_params((width, height), stride, fourcc, time);
        meta.modifier = Some(u64::from(dmabuf.format().modifier));
        meta
    }

    /// Construct metadata from a DMA-BUF handle with optional sync point.
    pub fn from_dmabuf_with_sync(dmabuf: &Dmabuf, time: Duration, sync: Option<SyncPoint>) -> Self {
        let mut meta = Self::from_dmabuf(dmabuf, time);
        meta.sync = sync;
        meta
    }

    /// Construct metadata from a DMA-BUF handle with sync point and damage.
    pub fn from_dmabuf_with_damage(
        dmabuf: &Dmabuf,
        time: Duration,
        sync: Option<SyncPoint>,
        damage: Option<&[Rectangle<i32, Physical>]>,
    ) -> Self {
        let mut meta = Self::from_dmabuf_with_sync(dmabuf, time, sync);
        meta.has_damage = damage.map_or(true, |d| !d.is_empty());
        meta.damage = damage.map(|d| d.to_vec());
        meta
    }

    /// Set damage information on existing metadata.
    pub fn with_damage(mut self, damage: Option<&[Rectangle<i32, Physical>]>) -> Self {
        self.has_damage = damage.map_or(true, |d| !d.is_empty());
        self.damage = damage.map(|d| d.to_vec());
        self
    }
}

/// Immutable, shareable CPU copy of frame bytes for 1 plane.
///
#[derive(Clone, Debug)]
pub struct RgbaFrame {
    data: Arc<[u8]>,
    size: (u32, u32),
    stride: u32,
}

impl RgbaFrame {
    /// Create a new CPU frame wrapper over owned bytes.
    pub fn new(data: Arc<[u8]>, size: (u32, u32), stride: u32) -> Self {
        Self { data, size, stride }
    }

    /// Raw plane-0 bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Width and height in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Bytes per row.
    pub fn stride(&self) -> u32 {
        self.stride
    }
}

/// Observer interface for frame taps.
///
/// Implementations should return quickly and offload heavy work to avoid
/// stalling the compositor thread. Both callbacks are optional and default to
/// no-ops.
pub trait FrameTap: Send + Sync {
    /// Called when a DMA-BUF frame is available.
    fn on_frame_dmabuf(&self, _out: &OutputId, _dmabuf: &Dmabuf, _meta: &FrameMeta) {}

    /// Called when an RGBA frame is available.
    fn on_frame_rgba(&self, _out: &OutputId, _frame: &RgbaFrame, _meta: &FrameMeta) {}

    /// Whether this tap wants all frames regardless of damage.
    ///
    /// By default, taps respect damage tracking and won't receive frames
    /// when `has_damage` is false. Override this to receive every frame
    /// (useful for diagnostics, testing, or consumers that need constant
    /// frame rate).
    fn wants_all_frames(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct FrameTapToken(u64);

/// Registrar that fan-outs presented frames to registered taps.
///
/// Registration returns a token that can be used to unregister later. All
/// notifications happen synchronously in registration order.
#[derive(Default)]
pub struct FrameTapManager {
    taps: Vec<(FrameTapToken, Arc<dyn FrameTap>)>,
    next_id: u64,
}

impl FrameTapToken {
    fn new(id: u64) -> Self {
        Self(id)
    }
}

impl FrameTapManager {
    /// Add a new tap and receive its token.
    pub fn register(&mut self, tap: Arc<dyn FrameTap>) -> FrameTapToken {
        let token = FrameTapToken::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.taps.push((token, tap));
        token
    }

    /// Remove a previously registered tap.
    pub fn unregister(&mut self, token: FrameTapToken) {
        self.taps.retain(|(entry_token, _)| *entry_token != token);
    }

    /// Fast check to avoid work when there are no observers.
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.taps.len()
    }

    /// Notify all taps with a DMA-BUF buffer and derived metadata.
    pub fn notify_dmabuf(&self, output: &Output, dmabuf: &Dmabuf, time: Duration) {
        self.notify_dmabuf_with_sync(output, dmabuf, time, None)
    }

    /// Notify all taps with a DMA-BUF buffer, metadata, and optional sync point.
    pub fn notify_dmabuf_with_sync(
        &self,
        output: &Output,
        dmabuf: &Dmabuf,
        time: Duration,
        sync: Option<SyncPoint>,
    ) {
        if self.taps.is_empty() {
            return;
        }
        tracing::debug!(
            "FrameTapManager: notifying {} taps with dmabuf ({}x{}, {} planes), sync={:?}",
            self.taps.len(),
            dmabuf.width(),
            dmabuf.height(),
            dmabuf.num_planes(),
            sync.as_ref().map(|_| "Some")
        );
        let meta = FrameMeta::from_dmabuf_with_sync(dmabuf, time, sync);
        let out_id = OutputId::from_output(output);
        for (_, tap) in &self.taps {
            tap.on_frame_dmabuf(&out_id, dmabuf, &meta);
        }
    }

    /// Notify all taps with a CPU copy and supplied pixel format.
    pub fn notify_rgba(&self, output: &Output, frame: RgbaFrame, fourcc: Fourcc, time: Duration) {
        self.notify_rgba_with_sync(output, frame, fourcc, time, None)
    }

    /// Notify all taps with a CPU copy, pixel format, and optional sync point.
    pub fn notify_rgba_with_sync(
        &self,
        output: &Output,
        frame: RgbaFrame,
        fourcc: Fourcc,
        time: Duration,
        sync: Option<SyncPoint>,
    ) {
        if self.taps.is_empty() {
            return;
        }
        tracing::debug!(
            "FrameTapManager: notifying {} taps with RGBA ({}x{}, stride={}), time={:?}, sync={:?}",
            self.taps.len(),
            frame.size().0,
            frame.size().1,
            frame.stride(),
            time,
            sync.as_ref().map(|_| "Some")
        );
        let meta =
            FrameMeta::from_params_with_sync(frame.size(), frame.stride(), fourcc, time, sync);
        let out_id = OutputId::from_output(output);
        for (_, tap) in &self.taps {
            tap.on_frame_rgba(&out_id, &frame, &meta);
        }
    }

    /// Check if any registered tap wants all frames regardless of damage.
    fn any_tap_wants_all_frames(&self) -> bool {
        self.taps.iter().any(|(_, tap)| tap.wants_all_frames())
    }

    /// Notify taps with a DMA-BUF buffer and damage information.
    ///
    /// This is the primary notification method for screen casting with damage
    /// tracking. Frames with no damage are skipped unless a tap explicitly
    /// requests all frames via `wants_all_frames()`.
    ///
    /// # Arguments
    ///
    /// * `output` - The output this frame was rendered for.
    /// * `dmabuf` - The DMA-BUF handle for the frame.
    /// * `time` - Presentation timestamp.
    /// * `sync` - Optional GPU sync point for synchronization.
    /// * `damage` - Damage rectangles in physical coordinates, or `None` for full damage.
    pub fn notify_dmabuf_with_damage(
        &self,
        output: &Output,
        dmabuf: &Dmabuf,
        time: Duration,
        sync: Option<SyncPoint>,
        damage: Option<&[Rectangle<i32, Physical>]>,
    ) {
        if self.taps.is_empty() {
            return;
        }

        // Check if there's actual damage
        let has_damage = damage.map_or(true, |d| !d.is_empty());

        // If no damage and no tap wants all frames, skip entirely
        if !has_damage && !self.any_tap_wants_all_frames() {
            tracing::trace!(
                "FrameTapManager: skipping frame for output {} (no damage)",
                output.name()
            );
            return;
        }

        tracing::debug!(
            "FrameTapManager: notifying {} taps with dmabuf ({}x{}, {} planes), damage={}, sync={:?}",
            self.taps.len(),
            dmabuf.width(),
            dmabuf.height(),
            dmabuf.num_planes(),
            has_damage,
            sync.as_ref().map(|_| "Some")
        );

        let meta = FrameMeta::from_dmabuf_with_damage(dmabuf, time, sync, damage);
        let out_id = OutputId::from_output(output);

        for (_, tap) in &self.taps {
            // Skip taps that don't want damaged frames and there's no damage
            if !has_damage && !tap.wants_all_frames() {
                continue;
            }
            tap.on_frame_dmabuf(&out_id, dmabuf, &meta);
        }
    }

    /// Notify taps with a CPU frame and damage information.
    ///
    /// Similar to `notify_dmabuf_with_damage` but for CPU-copied frames.
    pub fn notify_rgba_with_damage(
        &self,
        output: &Output,
        frame: RgbaFrame,
        fourcc: Fourcc,
        time: Duration,
        sync: Option<SyncPoint>,
        damage: Option<&[Rectangle<i32, Physical>]>,
    ) {
        if self.taps.is_empty() {
            return;
        }

        let has_damage = damage.map_or(true, |d| !d.is_empty());

        if !has_damage && !self.any_tap_wants_all_frames() {
            tracing::trace!(
                "FrameTapManager: skipping RGBA frame for output {} (no damage)",
                output.name()
            );
            return;
        }

        tracing::debug!(
            "FrameTapManager: notifying {} taps with RGBA ({}x{}, stride={}), damage={}, sync={:?}",
            self.taps.len(),
            frame.size().0,
            frame.size().1,
            frame.stride(),
            has_damage,
            sync.as_ref().map(|_| "Some")
        );

        let meta = FrameMeta::from_params_with_damage(
            frame.size(),
            frame.stride(),
            fourcc,
            time,
            sync,
            damage,
        );
        let out_id = OutputId::from_output(output);

        for (_, tap) in &self.taps {
            if !has_damage && !tap.wants_all_frames() {
                continue;
            }
            tap.on_frame_rgba(&out_id, &frame, &meta);
        }
    }
}

/// Capture the current renderer framebuffer into CPU memory.
///
/// Returns `None` when size is zero or on mapping failures. The pixel format is
/// ABGR8888 to match Smithay's convention for `copy_framebuffer`.
pub fn capture_rgba_frame<R>(renderer: &mut R, size: (u32, u32)) -> Option<RgbaFrame>
where
    R: Renderer + ExportMem,
    R::TextureMapping: 'static,
{
    let (width, height) = size;
    if width == 0 || height == 0 {
        return None;
    }

    let rect = smithay::utils::Rectangle::from_loc_and_size((0, 0), (width as i32, height as i32));
    let mapping = renderer.copy_framebuffer(rect, Fourcc::Abgr8888).ok()?;

    let data = renderer.map_texture(&mapping).ok()?;
    let owned: Arc<[u8]> = Arc::from(data.to_vec());
    let stride = width * 4;

    Some(RgbaFrame::new(owned, (width, height), stride))
}

/// Copy the contents of plane 0 from a DMA-BUF into CPU memory.
///
/// Synchronizes the read with explicit START/END fences, maps plane 0 in read
/// mode, and copies `stride * height` bytes into an owned buffer. This function
/// does not perform format conversion; consult `FrameMeta.fourcc` when
/// interpreting the bytes.
pub fn dmabuf_to_rgba(dmabuf: &Dmabuf) -> Option<RgbaFrame> {
    use std::slice;

    let bytes_per_row = dmabuf.strides().next()? as usize;
    let size = dmabuf.size();
    let width = size.w.max(0) as u32;
    let height = size.h.max(0) as u32;
    let total = bytes_per_row.checked_mul(height as usize)?;

    dmabuf
        .sync_plane(0, DmabufSyncFlags::START | DmabufSyncFlags::READ)
        .ok()?;
    let mapping = dmabuf.map_plane(0, DmabufMappingMode::READ).ok()?;

    // SAFETY: The pointer originates from the DMA-BUF mapping and is assumed to
    // be valid for at least `total` bytes. If a driver exposes a smaller mapping
    // than `stride * height`, this would be UB. Smithay's mapping abstraction is
    // expected to uphold this contract for plane 0.
    let slice = unsafe { slice::from_raw_parts(mapping.ptr() as *const u8, total) };
    let data = Arc::<[u8]>::from(slice.to_vec());

    if let Err(err) = dmabuf.sync_plane(0, DmabufSyncFlags::END | DmabufSyncFlags::READ) {
        tracing::warn!(?err, "failed to end dmabuf read sync");
    }

    Some(RgbaFrame::new(data, (width, height), bytes_per_row as u32))
}

/// A simple tap that logs metadata for each frame.
#[derive(Default)]
pub struct LoggingTap;

impl FrameTap for LoggingTap {
    fn on_frame_dmabuf(&self, out: &OutputId, _dmabuf: &Dmabuf, meta: &FrameMeta) {
        tracing::trace!(
            target: "screen_composer::frame_tap",
            output = %out.0,
            width = meta.size.0,
            height = meta.size.1,
            stride = meta.stride,
                fourcc = ?meta.fourcc,
            time_ns = meta.time_ns,
            kind = "dmabuf",
        );
    }

    fn on_frame_rgba(&self, out: &OutputId, frame: &RgbaFrame, meta: &FrameMeta) {
        tracing::trace!(
            target: "screen_composer::frame_tap",
            output = %out.0,
            width = frame.size().0,
            height = frame.size().1,
            stride = frame.stride(),
                fourcc = ?meta.fourcc,
            time_ns = meta.time_ns,
            kind = "rgba",
        );
    }
}

/// Whether the logging tap should be enabled based on environment.
///
/// Currently always returns false. Override by setting the environment
/// variable `SCREEN_COMPOSER_LOGGING_TAP=1`.
pub fn logging_tap_enabled() -> bool {
    std::env::var("SCREEN_COMPOSER_LOGGING_TAP")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Factory for the default `LoggingTap` implementation.
pub fn logging_tap() -> Arc<dyn FrameTap> {
    Arc::new(LoggingTap::default())
}
