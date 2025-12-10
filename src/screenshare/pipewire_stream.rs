//! PipeWire stream management for screencast.
//!
//! This module handles the PipeWire side of screen casting:
//! - Creating and configuring PipeWire streams
//! - Buffer management (SHM for RGBA frames)
//! - Feeding frames from the compositor to PipeWire
//!
//! ## Buffer Types
//!
//! - **SHM**: CPU memory buffers for RGBA frame data. The compositor renders
//!   to an RGBA buffer which is then copied to PipeWire's SHM buffers.
//!
//! ## Architecture
//!
//! The PipeWire stream runs on a dedicated thread with its own main loop.
//! Frames are sent from the compositor thread via an mpsc channel.
//! The stream thread receives frames and queues them to PipeWire.

use std::cell::RefCell;
use std::mem::size_of;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use std::thread;

use pipewire as pw;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::video::VideoFormat;
use pw::spa::param::ParamType;
use pw::spa::pod::{ChoiceValue, Pod, Property, Value};
use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Direction, Fraction, Rectangle, SpaTypes};
use pw::spa::sys::{
    spa_buffer_find_meta_data, spa_meta_header, spa_meta_region, spa_point, spa_rectangle,
    SPA_META_Header, SPA_META_VideoDamage, SPA_PARAM_BUFFERS_align, SPA_PARAM_BUFFERS_blocks,
    SPA_PARAM_BUFFERS_buffers, SPA_PARAM_BUFFERS_dataType, SPA_PARAM_BUFFERS_size,
    SPA_PARAM_BUFFERS_stride, SPA_PARAM_META_size, SPA_PARAM_META_type,
};
use pw::spa::buffer::DataType;
use pw::stream::{StreamFlags, StreamState};
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
            prefer_dmabuf: false, // Use SHM for now (simpler)
        }
    }
}

/// Shared state between the PipeWire thread and the main compositor.
struct SharedState {
    /// The PipeWire node ID (set when stream is paused/ready).
    node_id: AtomicU32,
    /// Whether the stream is currently active.
    active: AtomicBool,
    /// Signal to stop the PipeWire thread.
    should_stop: AtomicBool,
}

/// Internal state for the PipeWire stream thread.
struct PipeWireThreadState {
    /// Negotiated video format.
    format: pw::spa::param::video::VideoInfoRaw,
    /// Current frame data waiting to be sent.
    pending_frame: Option<(Arc<[u8]>, FrameMetaSnapshot)>,
    /// Frame sequence counter for PTS calculation.
    sequence: u64,
}

/// A PipeWire stream for screen casting.
///
/// This manages the PipeWire stream lifecycle and buffer handling.
/// The stream runs on a dedicated thread.
pub struct PipeWireStream {
    /// Shared state with the PipeWire thread.
    shared: Arc<SharedState>,
    /// Stream configuration.
    config: StreamConfig,
    /// Receiver for frames from the compositor.
    frame_receiver: Option<mpsc::Receiver<FrameData>>,
    /// Handle to the PipeWire thread.
    thread_handle: Option<thread::JoinHandle<()>>,
    /// The PipeWire main loop FD (for sharing with portal clients).
    pipewire_fd: Option<OwnedFd>,
}

impl PipeWireStream {
    /// Create a new PipeWire stream.
    ///
    /// Returns the stream and a sender for frames.
    pub fn new(config: StreamConfig) -> (Self, mpsc::Sender<FrameData>) {
        // Create a bounded channel for frames (buffer up to 3 frames)
        let (sender, receiver) = mpsc::channel(3);

        let shared = Arc::new(SharedState {
            node_id: AtomicU32::new(0),
            active: AtomicBool::new(false),
            should_stop: AtomicBool::new(false),
        });

        let stream = Self {
            shared,
            config,
            frame_receiver: Some(receiver),
            thread_handle: None,
            pipewire_fd: None,
        };

        (stream, sender)
    }

    /// Start the PipeWire stream synchronously.
    ///
    /// This spawns a dedicated thread that runs the PipeWire main loop.
    /// Blocks until the stream is ready and returns the PipeWire node ID.
    ///
    /// Use this from synchronous contexts (like the compositor's calloop handler).
    pub fn start_sync(&mut self) -> Result<u32, PipeWireError> {
        if self.shared.active.load(Ordering::SeqCst) {
            return Err(PipeWireError::AlreadyActive);
        }

        let receiver = self
            .frame_receiver
            .take()
            .ok_or_else(|| PipeWireError::InitFailed("Frame receiver already taken".to_string()))?;

        let config = self.config.clone();
        let shared = self.shared.clone();

        // Use std::sync channel for synchronous waiting
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        // Spawn the PipeWire thread
        let handle = thread::spawn(move || {
            if let Err(e) = run_pipewire_thread_sync(config, shared.clone(), receiver, ready_tx) {
                tracing::error!("PipeWire thread error: {}", e);
            }
            shared.active.store(false, Ordering::SeqCst);
        });

        self.thread_handle = Some(handle);

        // Wait for the stream to be ready (with timeout)
        let result = ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| {
                PipeWireError::InitFailed(format!("PipeWire thread failed to initialize: {}", e))
            })??;

        let (node_id, fd) = result;

        self.shared.node_id.store(node_id, Ordering::SeqCst);
        self.shared.active.store(true, Ordering::SeqCst);
        self.pipewire_fd = Some(fd);

        tracing::info!(
            "PipeWire stream started: {}x{} @ {}/{}fps, node_id={}",
            self.config.width,
            self.config.height,
            self.config.framerate_num,
            self.config.framerate_denom,
            node_id
        );

        Ok(node_id)
    }

    /// Start the PipeWire stream asynchronously.
    ///
    /// This spawns a dedicated thread that runs the PipeWire main loop.
    /// Returns the PipeWire node ID.
    #[allow(dead_code)]
    pub async fn start(&mut self) -> Result<u32, PipeWireError> {
        if self.shared.active.load(Ordering::SeqCst) {
            return Err(PipeWireError::AlreadyActive);
        }

        let receiver = self
            .frame_receiver
            .take()
            .ok_or_else(|| PipeWireError::InitFailed("Frame receiver already taken".to_string()))?;

        let config = self.config.clone();
        let shared = self.shared.clone();

        // Channel to receive the node_id and FD from the thread once it's ready
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

        // Spawn the PipeWire thread
        let handle = thread::spawn(move || {
            if let Err(e) = run_pipewire_thread(config, shared.clone(), receiver, ready_tx) {
                tracing::error!("PipeWire thread error: {}", e);
            }
            shared.active.store(false, Ordering::SeqCst);
        });

        self.thread_handle = Some(handle);

        // Wait for the stream to be ready
        let (node_id, fd) = ready_rx.await.map_err(|_| {
            PipeWireError::InitFailed("PipeWire thread failed to initialize".to_string())
        })??;

        self.shared.node_id.store(node_id, Ordering::SeqCst);
        self.shared.active.store(true, Ordering::SeqCst);
        self.pipewire_fd = Some(fd);

        tracing::info!(
            "PipeWire stream started: {}x{} @ {}/{}fps, node_id={}",
            self.config.width,
            self.config.height,
            self.config.framerate_num,
            self.config.framerate_denom,
            node_id
        );

        Ok(node_id)
    }

    /// Stop the PipeWire stream.
    pub async fn stop(&mut self) -> Result<(), PipeWireError> {
        if !self.shared.active.load(Ordering::SeqCst) {
            return Err(PipeWireError::NotActive);
        }

        tracing::info!(
            "Stopping PipeWire stream (node {})",
            self.shared.node_id.load(Ordering::SeqCst)
        );

        // Signal the thread to stop
        self.shared.should_stop.store(true, Ordering::SeqCst);

        // Wait for the thread to finish
        if let Some(handle) = self.thread_handle.take() {
            handle.join().map_err(|_| {
                PipeWireError::StreamError("Failed to join PipeWire thread".to_string())
            })?;
        }

        self.shared.active.store(false, Ordering::SeqCst);
        self.pipewire_fd = None;

        Ok(())
    }

    /// Get the PipeWire node ID.
    pub fn node_id(&self) -> u32 {
        self.shared.node_id.load(Ordering::SeqCst)
    }

    /// Get the stream configuration.
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Check if the stream is active.
    pub fn is_active(&self) -> bool {
        self.shared.active.load(Ordering::SeqCst)
    }

    /// Get the PipeWire file descriptor for sharing with portal clients.
    ///
    /// Returns None if the stream is not active.
    pub fn pipewire_fd(&self) -> Option<RawFd> {
        self.pipewire_fd.as_ref().map(|fd| fd.as_raw_fd())
    }

    /// Take ownership of the frame receiver for the stream pump loop.
    pub fn take_frame_receiver(&mut self) -> Option<mpsc::Receiver<FrameData>> {
        self.frame_receiver.take()
    }

    /// Run the stream pump loop (legacy API, now handled internally).
    pub async fn pump_loop(
        mut receiver: mpsc::Receiver<FrameData>,
        node_id: u32,
    ) -> Result<(), PipeWireError> {
        tracing::debug!("Starting legacy pump loop for node {}", node_id);

        // Just drain the receiver - actual PipeWire handling is in the thread
        while let Some(frame) = receiver.recv().await {
            match frame {
                FrameData::DmaBuf { meta, .. } => {
                    tracing::trace!(
                        "Legacy pump: dmabuf frame {}x{}, damage={}",
                        meta.size.0,
                        meta.size.1,
                        meta.has_damage
                    );
                }
                FrameData::Rgba { data, meta } => {
                    tracing::trace!(
                        "Legacy pump: RGBA frame {}x{}, {} bytes, damage={}",
                        meta.size.0,
                        meta.size.1,
                        data.len(),
                        meta.has_damage
                    );
                }
            }
        }

        tracing::debug!("Legacy pump loop ended for node {}", node_id);
        Ok(())
    }
}

impl Drop for PipeWireStream {
    fn drop(&mut self) {
        self.shared.should_stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Run the PipeWire main loop on a dedicated thread.
fn run_pipewire_thread(
    config: StreamConfig,
    shared: Arc<SharedState>,
    mut frame_receiver: mpsc::Receiver<FrameData>,
    ready_tx: tokio::sync::oneshot::Sender<Result<(u32, OwnedFd), PipeWireError>>,
) -> Result<(), PipeWireError> {
    // Initialize PipeWire
    pw::init();

    // Create main loop (using MainLoopBox for owned version)
    let mainloop = pw::main_loop::MainLoopBox::new(None)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to create main loop: {}", e)))?;

    // Get the loop FD for sharing
    let loop_fd = mainloop.loop_().fd();
    let owned_fd = loop_fd.try_clone_to_owned().map_err(|e| {
        PipeWireError::InitFailed(format!("Failed to clone loop FD: {}", e))
    })?;

    // Create context (using ContextBox for owned version)
    let context = pw::context::ContextBox::new(mainloop.loop_(), None)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to create context: {}", e)))?;

    // Connect to PipeWire
    let core = context
        .connect(None)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to connect to PipeWire: {}", e)))?;

    // Create stream
    let stream = pw::stream::StreamBox::new(
        &core,
        "screen-composer-screencast",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
            *pw::keys::MEDIA_CLASS => "Video/Source",
        },
    )
    .map_err(|e| PipeWireError::InitFailed(format!("Failed to create stream: {}", e)))?;

    // State for callbacks
    let thread_state = Rc::new(RefCell::new(PipeWireThreadState {
        format: Default::default(),
        pending_frame: None,
        sequence: 0,
    }));

    // Track if we've sent the ready signal
    let ready_sent = Rc::new(RefCell::new(false));
    let ready_tx = Rc::new(RefCell::new(Some(ready_tx)));
    let owned_fd = Rc::new(RefCell::new(Some(owned_fd)));

    // Clone for callbacks
    let shared_cb = shared.clone();
    let ready_tx_cb = ready_tx.clone();
    let owned_fd_cb = owned_fd.clone();
    let ready_sent_cb = ready_sent.clone();

    // Set up stream listener
    let _listener = stream
        .add_local_listener_with_user_data(thread_state.clone())
        .state_changed(move |stream, _state, old, new| {
            tracing::debug!("PipeWire stream state: {:?} -> {:?}", old, new);

            match new {
                StreamState::Paused => {
                    // Stream is ready, get node ID
                    let node_id = stream.node_id();
                    tracing::info!("PipeWire stream paused, node_id={}", node_id);

                    // Send ready signal (only once)
                    if !*ready_sent_cb.borrow() {
                        *ready_sent_cb.borrow_mut() = true;
                        if let Some(tx) = ready_tx_cb.borrow_mut().take() {
                            if let Some(fd) = owned_fd_cb.borrow_mut().take() {
                                let _ = tx.send(Ok((node_id, fd)));
                            }
                        }
                    }
                }
                StreamState::Streaming => {
                    tracing::info!("PipeWire stream now streaming");
                }
                StreamState::Error(ref err) => {
                    tracing::error!("PipeWire stream error: {}", err);
                    shared_cb.should_stop.store(true, Ordering::SeqCst);

                    // Send error if not ready yet
                    if !*ready_sent_cb.borrow() {
                        *ready_sent_cb.borrow_mut() = true;
                        if let Some(tx) = ready_tx_cb.borrow_mut().take() {
                            let _ = tx.send(Err(PipeWireError::StreamError(err.clone())));
                        }
                    }
                }
                _ => {}
            }
        })
        .param_changed(|_stream, state, id, param| {
            let Some(param) = param else {
                return;
            };

            if id != ParamType::Format.as_raw() {
                return;
            }

            // Parse the format
            let (media_type, media_subtype) =
                match pw::spa::param::format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to parse format: {:?}", e);
                        return;
                    }
                };

            if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                return;
            }

            // Parse video info
            let mut state = state.borrow_mut();
            if let Err(e) = state.format.parse(param) {
                tracing::warn!("Failed to parse video format: {:?}", e);
                return;
            }

            tracing::info!(
                "PipeWire format negotiated: {:?} {}x{} @ {}/{}",
                state.format.format(),
                state.format.size().width,
                state.format.size().height,
                state.format.framerate().num,
                state.format.framerate().denom
            );

            // Ask PipeWire to allocate buffers/meta for this format
            let size = state.format.size();
            let stride = (size.width * 4) as i32;
            let frame_bytes = (size.width * size.height * 4) as i32;

            let buffers_obj = pw::spa::pod::object!(
                SpaTypes::ObjectParamBuffers,
                ParamType::Buffers,
                Property::new(
                    SPA_PARAM_BUFFERS_buffers,
                    Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Range {
                            default: 8,
                            min: 2,
                            max: 16
                        }
                    )))
                ),
                Property::new(SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
                Property::new(SPA_PARAM_BUFFERS_size, Value::Int(frame_bytes)),
                Property::new(SPA_PARAM_BUFFERS_stride, Value::Int(stride)),
                Property::new(
                    SPA_PARAM_BUFFERS_dataType,
                    Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Flags {
                            default: 1 << DataType::MemPtr.as_raw(),
                            flags: vec![
                                1 << DataType::MemPtr.as_raw(),
                                1 << DataType::MemFd.as_raw(),
                            ],
                        },
                    )))
                ),
            );

            let meta_header = pw::spa::pod::object!(
                SpaTypes::ObjectParamMeta,
                ParamType::Meta,
                Property::new(
                    SPA_PARAM_META_type,
                    Value::Id(pw::spa::utils::Id(SPA_META_Header as u32)),
                ),
                Property::new(
                    SPA_PARAM_META_size,
                    Value::Int(size_of::<spa_meta_header>() as i32)
                ),
            );

            let meta_damage = pw::spa::pod::object!(
                SpaTypes::ObjectParamMeta,
                ParamType::Meta,
                Property::new(
                    SPA_PARAM_META_type,
                    Value::Id(pw::spa::utils::Id(SPA_META_VideoDamage as u32))
                ),
                Property::new(
                    SPA_PARAM_META_size,
                    Value::Int((size_of::<spa_meta_region>() * 2) as i32)
                ),
            );

            let mut b1 = Vec::new();
            let mut b2 = Vec::new();
            let mut b3 = Vec::new();
            let buffers_pod = pw::spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &pw::spa::pod::Value::Object(buffers_obj),
            )
            .map(|v| v.0.into_inner())
            .ok();
            let header_pod = pw::spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &pw::spa::pod::Value::Object(meta_header),
            )
            .map(|v| v.0.into_inner())
            .ok();
            let damage_pod = pw::spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &pw::spa::pod::Value::Object(meta_damage),
            )
            .map(|v| v.0.into_inner())
            .ok();

            let mut param_refs: Vec<&Pod> = Vec::new();
            if let Some(bytes) = buffers_pod.as_ref() {
                if let Some(pod) = Pod::from_bytes(bytes) {
                    b1 = bytes.clone();
                    param_refs.push(pod);
                }
            }
            if let Some(bytes) = header_pod.as_ref() {
                if let Some(pod) = Pod::from_bytes(bytes) {
                    b2 = bytes.clone();
                    param_refs.push(pod);
                }
            }
            if let Some(bytes) = damage_pod.as_ref() {
                if let Some(pod) = Pod::from_bytes(bytes) {
                    b3 = bytes.clone();
                    param_refs.push(pod);
                }
            }

            if let Err(e) = _stream.update_params(&mut param_refs) {
                tracing::warn!("Failed to update buffers/meta params: {e}");
            }
        })
        .process(|stream, state| {
            // Dequeue a buffer from PipeWire (raw) so we can explicitly queue it back
            let buffer_ptr = unsafe { stream.dequeue_raw_buffer() };
            if buffer_ptr.is_null() {
                return;
            }

            let mut state = state.borrow_mut();

            // Get pending frame data
            let Some((frame_data, meta)) = state.pending_frame.take() else {
                unsafe { pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr) };
                return;
            };

            unsafe {
                let spa_buf = (*buffer_ptr).buffer;
                if spa_buf.is_null() {
                    pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
                    return;
                }

                let datas = (*spa_buf).datas;
                if datas.is_null() || (*spa_buf).n_datas == 0 {
                    pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
                    return;
                }

                let data = datas;
                let chunk = (*data).chunk;
                let data_ptr = (*data).data as *mut u8;
                let maxsize = (*data).maxsize as usize;

                if data_ptr.is_null() || chunk.is_null() || maxsize == 0 {
                    pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
                    return;
                }

                let stride_bytes = meta.stride.max(meta.size.0 * 4);
                let expected_size = (stride_bytes * meta.size.1) as usize;
                let copy_len = frame_data.len().min(maxsize).min(expected_size);
                std::ptr::copy_nonoverlapping(frame_data.as_ptr(), data_ptr, copy_len);

                (*chunk).size = copy_len as u32;
                (*chunk).offset = 0;
                (*chunk).stride = stride_bytes as i32;
                (*chunk).flags = 0;

                state.sequence += 1;

                // Write SPA meta header if present
                let header_ptr = spa_buffer_find_meta_data(
                    spa_buf,
                    SPA_META_Header,
                    size_of::<spa_meta_header>(),
                ) as *mut spa_meta_header;
                if !header_ptr.is_null() {
                    (*header_ptr).flags = 0;
                    (*header_ptr).offset = 0;
                    (*header_ptr).pts = meta.time_ns as i64;
                    (*header_ptr).dts_offset = 0;
                    (*header_ptr).seq = state.sequence;
                }

                // Write full-frame damage meta if available
                let damage_ptr = spa_buffer_find_meta_data(
                    spa_buf,
                    SPA_META_VideoDamage,
                    size_of::<spa_meta_region>(),
                ) as *mut spa_meta_region;
                if !damage_ptr.is_null() {
                    // First entry: full frame
                    (*damage_ptr).region.position = spa_point { x: 0, y: 0 };
                    (*damage_ptr).region.size = spa_rectangle {
                        width: meta.size.0,
                        height: meta.size.1,
                    };
                    // Second entry: terminator (invalid region)
                    let term = damage_ptr.add(1);
                    (*term).region.position = spa_point { x: 0, y: 0 };
                    (*term).region.size = spa_rectangle { width: 0, height: 0 };
                }

                tracing::trace!(
                    "Queued frame {} to PipeWire: {}x{} ({} bytes)",
                    state.sequence,
                    meta.size.0,
                    meta.size.1,
                    copy_len
                );

                // Explicitly queue the buffer back to PipeWire
                pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
            }
        })
        .register()
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to register listener: {}", e)))?;

    // Build format/buffer/meta parameters
    let param_bytes = build_stream_params(&config)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to build params: {}", e)))?;
    let mut param_refs: Vec<&Pod> = param_bytes
        .iter()
        .filter_map(|b| Pod::from_bytes(b))
        .collect();

    // Connect the stream as a source (output)
    stream
        .connect(
            Direction::Output,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::ALLOC_BUFFERS,
            &mut param_refs,
        )
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to connect stream: {}", e)))?;

    tracing::info!("PipeWire stream connected");

    // Run the main loop, checking for frames and stop signal
    let loop_ref = mainloop.loop_();
    loop {
        // Check if we should stop
        if shared.should_stop.load(Ordering::SeqCst) {
            break;
        }

        // Poll and drain available frames (non-blocking), keep the latest
        loop {
            match frame_receiver.try_recv() {
                Ok(frame) => {
                    let (data, meta) = match frame {
                        FrameData::Rgba { data, meta } => (data, meta),
                        FrameData::DmaBuf { .. } => {
                            // For now, skip DMA-BUF frames
                            tracing::trace!("Skipping DMA-BUF frame (not implemented)");
                            continue;
                        }
                    };

                    thread_state.borrow_mut().pending_frame = Some((data, meta));
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, stop
                    tracing::debug!("Frame channel disconnected, stopping PipeWire thread");
                    return Ok(());
                }
            }
        }

        // Iterate the PipeWire loop with a short timeout
        loop_ref.iterate(std::time::Duration::from_millis(10));
    }

    // Disconnect and cleanup
    let _ = stream.disconnect();
    tracing::info!("PipeWire stream disconnected");

    Ok(())
}

/// Run the PipeWire main loop on a dedicated thread (synchronous version).
///
/// Uses std::sync::mpsc for the ready signal instead of tokio channels.
fn run_pipewire_thread_sync(
    config: StreamConfig,
    shared: Arc<SharedState>,
    mut frame_receiver: mpsc::Receiver<FrameData>,
    ready_tx: std::sync::mpsc::Sender<Result<(u32, OwnedFd), PipeWireError>>,
) -> Result<(), PipeWireError> {
    // Initialize PipeWire
    pw::init();

    // Create main loop (using MainLoopBox for owned version)
    let mainloop = pw::main_loop::MainLoopBox::new(None)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to create main loop: {}", e)))?;

    // Get the loop FD for sharing
    let loop_fd = mainloop.loop_().fd();
    let owned_fd = loop_fd.try_clone_to_owned().map_err(|e| {
        PipeWireError::InitFailed(format!("Failed to clone loop FD: {}", e))
    })?;

    // Create context (using ContextBox for owned version)
    let context = pw::context::ContextBox::new(mainloop.loop_(), None)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to create context: {}", e)))?;

    // Connect to PipeWire
    let core = context
        .connect(None)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to connect to PipeWire: {}", e)))?;

    // Create stream
    let stream = pw::stream::StreamBox::new(
        &core,
        "screen-composer-screencast",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
            *pw::keys::MEDIA_CLASS => "Video/Source",
        },
    )
    .map_err(|e| PipeWireError::InitFailed(format!("Failed to create stream: {}", e)))?;

    // State for callbacks
    let thread_state = Rc::new(RefCell::new(PipeWireThreadState {
        format: Default::default(),
        pending_frame: None,
        sequence: 0,
    }));

    // Track if we've sent the ready signal
    let ready_sent = Rc::new(RefCell::new(false));
    let ready_tx = Rc::new(RefCell::new(Some(ready_tx)));
    let owned_fd = Rc::new(RefCell::new(Some(owned_fd)));

    // Clone for callbacks
    let shared_cb = shared.clone();
    let ready_tx_cb = ready_tx.clone();
    let owned_fd_cb = owned_fd.clone();
    let ready_sent_cb = ready_sent.clone();

    // Set up stream listener
    let _listener = stream
        .add_local_listener_with_user_data(thread_state.clone())
        .state_changed(move |stream, _state, old, new| {
            tracing::debug!("PipeWire stream state: {:?} -> {:?}", old, new);

            match new {
                StreamState::Paused => {
                    // Stream is ready, get node ID
                    let node_id = stream.node_id();
                    tracing::info!("PipeWire stream paused, node_id={}", node_id);

                    // Send ready signal (only once)
                    if !*ready_sent_cb.borrow() {
                        *ready_sent_cb.borrow_mut() = true;
                        if let Some(tx) = ready_tx_cb.borrow_mut().take() {
                            if let Some(fd) = owned_fd_cb.borrow_mut().take() {
                                let _ = tx.send(Ok((node_id, fd)));
                            }
                        }
                    }
                }
                StreamState::Streaming => {
                    tracing::info!("PipeWire stream now streaming");
                }
                StreamState::Error(ref err) => {
                    tracing::error!("PipeWire stream error: {}", err);
                    shared_cb.should_stop.store(true, Ordering::SeqCst);

                    // Send error if not ready yet
                    if !*ready_sent_cb.borrow() {
                        *ready_sent_cb.borrow_mut() = true;
                        if let Some(tx) = ready_tx_cb.borrow_mut().take() {
                            let _ = tx.send(Err(PipeWireError::StreamError(err.clone())));
                        }
                    }
                }
                _ => {}
            }
        })
        .param_changed(|_stream, state, id, param| {
            let Some(param) = param else {
                return;
            };

            if id != ParamType::Format.as_raw() {
                return;
            }

            // Parse the format
            let (media_type, media_subtype) =
                match pw::spa::param::format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to parse format: {:?}", e);
                        return;
                    }
                };

            if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                return;
            }

            // Parse video info
            let mut state = state.borrow_mut();
            if let Err(e) = state.format.parse(param) {
                tracing::warn!("Failed to parse video format: {:?}", e);
                return;
            }

            tracing::info!(
                "PipeWire format negotiated: {:?} {}x{} @ {}/{}",
                state.format.format(),
                state.format.size().width,
                state.format.size().height,
                state.format.framerate().num,
                state.format.framerate().denom
            );
        })
        .process(|stream, state| {
            // Dequeue a buffer from PipeWire (raw) so we can explicitly queue it back
            let buffer_ptr = unsafe { stream.dequeue_raw_buffer() };
            if buffer_ptr.is_null() {
                return;
            }

            let mut state = state.borrow_mut();

            // Get pending frame data
            let Some((frame_data, meta)) = state.pending_frame.take() else {
                return;
            };

            unsafe {
                let spa_buf = (*buffer_ptr).buffer;
                if spa_buf.is_null() {
                    pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
                    return;
                }

                let datas = (*spa_buf).datas;
                if datas.is_null() || (*spa_buf).n_datas == 0 {
                    pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
                    return;
                }

                let data = datas;
                let chunk = (*data).chunk;
                let data_ptr = (*data).data as *mut u8;
                let maxsize = (*data).maxsize as usize;

                if data_ptr.is_null() || chunk.is_null() || maxsize == 0 {
                    pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
                    return;
                }

                let stride_bytes = meta.stride.max(meta.size.0 * 4);
                let expected_size = (stride_bytes * meta.size.1) as usize;
                let copy_len = frame_data.len().min(maxsize).min(expected_size);
                std::ptr::copy_nonoverlapping(frame_data.as_ptr(), data_ptr, copy_len);

                (*chunk).size = copy_len as u32;
                (*chunk).offset = 0;
                (*chunk).stride = stride_bytes as i32;
                (*chunk).flags = 0;

                state.sequence += 1;

                // Write SPA meta header if present
                let header_ptr = spa_buffer_find_meta_data(
                    spa_buf,
                    SPA_META_Header,
                    size_of::<spa_meta_header>(),
                ) as *mut spa_meta_header;
                if !header_ptr.is_null() {
                    (*header_ptr).flags = 0;
                    (*header_ptr).offset = 0;
                    (*header_ptr).pts = meta.time_ns as i64;
                    (*header_ptr).dts_offset = 0;
                    (*header_ptr).seq = state.sequence;
                }

                // Write full-frame damage meta if available
                let damage_ptr = spa_buffer_find_meta_data(
                    spa_buf,
                    SPA_META_VideoDamage,
                    size_of::<spa_meta_region>(),
                ) as *mut spa_meta_region;
                if !damage_ptr.is_null() {
                    (*damage_ptr).region.position = spa_point { x: 0, y: 0 };
                    (*damage_ptr).region.size = spa_rectangle {
                        width: meta.size.0,
                        height: meta.size.1,
                    };
                    let term = damage_ptr.add(1);
                    (*term).region.position = spa_point { x: 0, y: 0 };
                    (*term).region.size = spa_rectangle { width: 0, height: 0 };
                }
                tracing::debug!(
                    "PipeWire process (sync): queued frame {} ({}x{}, {} bytes)",
                    state.sequence,
                    meta.size.0,
                    meta.size.1,
                    copy_len
                );

                // Explicitly queue the buffer back to PipeWire
                pw::sys::pw_stream_queue_buffer(stream.as_raw_ptr(), buffer_ptr);
            }
        })
        .register()
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to register listener: {}", e)))?;

    // Build format/buffer/meta parameters
    let param_bytes = build_stream_params(&config)
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to build params: {}", e)))?;
    let mut param_refs: Vec<&Pod> = param_bytes
        .iter()
        .filter_map(|b| Pod::from_bytes(b))
        .collect();

    // Connect the stream as a source (output)
    stream
        .connect(
            Direction::Output,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::ALLOC_BUFFERS,
            &mut param_refs,
        )
        .map_err(|e| PipeWireError::InitFailed(format!("Failed to connect stream: {}", e)))?;

    tracing::info!("PipeWire stream connected");

    // Run the main loop, checking for frames and stop signal
    let loop_ref = mainloop.loop_();
    loop {
        if shared.should_stop.load(Ordering::SeqCst) {
            break;
        }

        // Poll and drain available frames (non-blocking), keep the latest
        loop {
            match frame_receiver.try_recv() {
                Ok(frame) => {
                    let (data, meta) = match frame {
                        FrameData::Rgba { data, meta } => {
                            tracing::debug!(
                                "PipeWire thread (sync): received RGBA frame {}x{}, {} bytes",
                                meta.size.0,
                                meta.size.1,
                                data.len()
                            );
                            (data, meta)
                        }
                        FrameData::DmaBuf { .. } => {
                            tracing::trace!("Skipping DMA-BUF frame (not implemented)");
                            continue;
                        }
                    };
                    thread_state.borrow_mut().pending_frame = Some((data, meta));
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    tracing::debug!("Frame channel disconnected, stopping PipeWire thread");
                    return Ok(());
                }
            }
        }

        loop_ref.iterate(std::time::Duration::from_millis(10));
    }

    let _ = stream.disconnect();
    tracing::info!("PipeWire stream disconnected");

    Ok(())
}

/// Build video format parameters for the stream.
fn build_video_format_params(config: &StreamConfig) -> pw::spa::pod::Object {
    pw::spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        pw::spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pw::spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pw::spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            // Default format first, then alternatives
            VideoFormat::BGRA,
            VideoFormat::BGRA,
            VideoFormat::RGBA,
            VideoFormat::BGRx,
            VideoFormat::RGBx
        ),
        pw::spa::pod::property!(
            FormatProperties::VideoSize,
            Rectangle,
            Rectangle {
                width: config.width,
                height: config.height
            }
        ),
        pw::spa::pod::property!(
            FormatProperties::VideoFramerate,
            Fraction,
            Fraction {
                num: config.framerate_num,
                denom: config.framerate_denom
            }
        )
    )
}

fn build_buffer_params(config: &StreamConfig) -> pw::spa::pod::Object {
    let stride = (config.width * 4) as i32;
    let size = (config.height * config.width * 4) as i32;

    pw::spa::pod::object!(
        SpaTypes::ObjectParamBuffers,
        ParamType::Buffers,
        Property::new(
            SPA_PARAM_BUFFERS_buffers,
            Value::Choice(ChoiceValue::Int(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Range {
                    default: 8,
                    min: 2,
                    max: 16
                },
            ))),
        ),
        Property::new(SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
        Property::new(SPA_PARAM_BUFFERS_size, Value::Int(size)),
        Property::new(SPA_PARAM_BUFFERS_stride, Value::Int(stride)),
        Property::new(SPA_PARAM_BUFFERS_align, Value::Int(4)),
        Property::new(
            SPA_PARAM_BUFFERS_dataType,
            Value::Choice(ChoiceValue::Int(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Flags {
                    default: 1 << DataType::MemFd.as_raw(),
                    flags: vec![
                        1 << DataType::MemFd.as_raw(),
                        1 << DataType::MemPtr.as_raw(),
                    ],
                },
            ))),
        ),
    )
}

fn build_meta_params() -> [pw::spa::pod::Object; 2] {
    let header = pw::spa::pod::object!(
        SpaTypes::ObjectParamMeta,
        ParamType::Meta,
        Property::new(
            SPA_PARAM_META_type,
            Value::Id(pw::spa::utils::Id(SPA_META_Header as u32)),
        ),
        Property::new(
            SPA_PARAM_META_size,
            Value::Int(size_of::<spa_meta_header>() as i32),
        )
    );

    let damage = pw::spa::pod::object!(
        SpaTypes::ObjectParamMeta,
        ParamType::Meta,
        Property::new(
            SPA_PARAM_META_type,
            Value::Id(pw::spa::utils::Id(SPA_META_VideoDamage as u32)),
        ),
        Property::new(
            SPA_PARAM_META_size,
            Value::Int((2 * size_of::<spa_meta_region>()) as i32),
        )
    );

    [header, damage]
}

fn build_stream_params(config: &StreamConfig) -> Result<Vec<Vec<u8>>, String> {
    let format_params = build_video_format_params(config);
    let buffer_params = build_buffer_params(config);
    let meta_params = build_meta_params();

    let format_bytes: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(format_params),
    )
    .map_err(|e| format!("Failed to serialize format: {:?}", e))?
    .0
    .into_inner();

    let buffer_bytes: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(buffer_params),
    )
    .map_err(|e| format!("Failed to serialize buffers: {:?}", e))?
    .0
    .into_inner();

    let meta_header_bytes: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(meta_params[0].clone()),
    )
    .map_err(|e| format!("Failed to serialize meta header: {:?}", e))?
    .0
    .into_inner();

    let meta_damage_bytes: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(meta_params[1].clone()),
    )
    .map_err(|e| format!("Failed to serialize meta damage: {:?}", e))?
    .0
    .into_inner();

    Ok(vec![
        format_bytes,
        buffer_bytes,
        meta_header_bytes,
        meta_damage_bytes,
    ])
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
