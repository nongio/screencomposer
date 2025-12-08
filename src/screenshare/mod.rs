//! Screenshare infrastructure for the compositor.
//!
//! This module provides the compositor-side support for screen casting via the
//! xdg-desktop-portal protocol. It exposes a D-Bus service that the portal
//! backend (`xdg-desktop-portal-sc`) communicates with to:
//!
//! - Enumerate available outputs
//! - Create screencast sessions
//! - Start/stop recording
//! - Provide PipeWire file descriptors for video streams
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  org.screencomposer.ScreenCast (D-Bus)                      │
//! │       │                                                     │
//! │       ▼                                                     │
//! │  FrameTapManager ← receives frames from render loop         │
//! │       │                                                     │
//! │       ▼                                                     │
//! │  ScreencastSessionTap → PipeWire stream                     │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Threading Model
//!
//! The compositor uses calloop for its main event loop (synchronous, ~16ms
//! dispatch). The D-Bus service requires async (zbus/tokio). We bridge these:
//!
//! - D-Bus server runs on a dedicated tokio runtime thread
//! - Commands flow from D-Bus → compositor via `calloop::channel`
//! - Responses flow from compositor → D-Bus via `tokio::sync::mpsc`
//!

use std::collections::HashMap;
use std::sync::Arc;

mod dbus_service;
mod frame_tap;
mod pipewire_stream;
mod session_tap;

pub use dbus_service::run_dbus_service;
pub use frame_tap::{
    capture_rgba_frame, dmabuf_to_rgba, logging_tap, FrameMeta, FrameTap,
    FrameTapManager, FrameTapToken, OutputId, RgbaFrame,
};
pub use pipewire_stream::{PipeWireStream, StreamConfig};
pub use session_tap::ScreencastSessionTap;

use session_tap::FrameData;
use smithay::reexports::calloop::channel::{channel, Event as ChannelEvent, Sender as ChannelSender};
use zbus::zvariant::OwnedFd;

/// Active screencast session state (compositor side).
///
/// Tracks all active streams for a D-Bus session.
pub struct ScreencastSession {
    /// The D-Bus session path (e.g., "/org/screencomposer/ScreenCast/session/1").
    pub session_id: String,
    /// Active streams indexed by output connector name.
    pub streams: HashMap<String, ActiveStream>,
}

/// Active stream for one output.
///
/// Contains the frame tap, PipeWire stream, and channel for frame delivery.
pub struct ActiveStream {
    /// Output connector name (e.g., "HDMI-A-1").
    pub output_connector: String,
    /// Token for unregistering the tap from FrameTapManager.
    pub tap_token: FrameTapToken,
    /// PipeWire stream instance.
    pub pipewire_stream: PipeWireStream,
    /// Sender for frames to the PipeWire pump loop.
    pub frame_sender: tokio::sync::mpsc::Sender<FrameData>,
}

/// Commands sent from the D-Bus service to the compositor main loop.
#[derive(Debug)]
pub enum CompositorCommand {
    /// Create a new screencast session.
    CreateSession {
        session_id: String,
    },
    /// List available outputs for screen casting.
    ListOutputs {
        response_tx: tokio::sync::oneshot::Sender<Vec<OutputInfo>>,
    },
    /// Start recording on a specific output.
    StartRecording {
        session_id: String,
        output_connector: String,
    },
    /// Stop recording on a specific output.
    StopRecording {
        session_id: String,
        output_connector: String,
    },
    /// Get a PipeWire file descriptor for the session.
    GetPipeWireFd {
        session_id: String,
        response_tx: tokio::sync::oneshot::Sender<Result<OwnedFd, String>>,
    },
    /// Destroy a session.
    DestroySession {
        session_id: String,
    },
}

/// Information about an available output.
#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub connector: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
}

/// Information about an active stream.
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub node_id: u32,
    pub width: u32,
    pub height: u32,
}

/// Manager for the screenshare subsystem.
///
/// Owns the D-Bus service handle and the channel for communicating with it.
pub struct ScreenshareManager {
    /// Sender for commands from the D-Bus thread.
    pub command_sender: ChannelSender<CompositorCommand>,
    /// Handle to the tokio runtime thread (kept alive).
    _runtime_handle: std::thread::JoinHandle<()>,
}

impl ScreenshareManager {
    /// Start the screenshare D-Bus service.
    ///
    /// This spawns a dedicated tokio runtime thread that runs the zbus server.
    /// Returns a manager that can be stored in the compositor state.
    pub fn start<B: crate::state::Backend + 'static>(
        loop_handle: &smithay::reexports::calloop::LoopHandle<'static, crate::state::ScreenComposer<B>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (cmd_sender, cmd_receiver) = channel::<CompositorCommand>();

        // Register the calloop channel to receive commands
        loop_handle
            .insert_source(cmd_receiver, |event, _, state| {
                if let ChannelEvent::Msg(cmd) = event {
                    handle_screenshare_command(state, cmd);
                }
            })
            .map_err(|e| format!("Failed to insert screenshare channel: {}", e))?;

        // Spawn the D-Bus service on a dedicated tokio thread
        let cmd_sender_clone = cmd_sender.clone();
        let runtime_handle = std::thread::Builder::new()
            .name("screenshare-dbus".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create tokio runtime for screenshare");

                rt.block_on(async move {
                    if let Err(e) = dbus_service::run_dbus_service(cmd_sender_clone).await {
                        tracing::error!("Screenshare D-Bus service failed: {}", e);
                    }
                });
            })?;

        Ok(Self {
            command_sender: cmd_sender,
            _runtime_handle: runtime_handle,
        })
    }
}

/// Handle a command from the D-Bus service.
fn handle_screenshare_command<B: crate::state::Backend + 'static>(
    state: &mut crate::state::ScreenComposer<B>,
    cmd: CompositorCommand,
) {
    match cmd {
        CompositorCommand::CreateSession { session_id } => {
            tracing::info!("CreateSession: {}", session_id);

            // Create compositor-side session state
            state.screenshare_sessions.insert(
                session_id.clone(),
                ScreencastSession {
                    session_id,
                    streams: HashMap::new(),
                },
            );
        }
        CompositorCommand::ListOutputs { response_tx } => {
            let outputs: Vec<OutputInfo> = state
                .workspaces
                .outputs()
                .map(|output| {
                    let (width, height, refresh_rate) = output
                        .current_mode()
                        .map(|m| (m.size.w as u32, m.size.h as u32, m.refresh as u32))
                        .unwrap_or((0, 0, 0));
                    OutputInfo {
                        connector: output.name(),
                        name: output.name(),
                        width,
                        height,
                        refresh_rate,
                    }
                })
                .collect();
            let _ = response_tx.send(outputs);
        }
        CompositorCommand::StartRecording {
            session_id,
            output_connector,
        } => {
            tracing::info!(
                "StartRecording: session={}, output={}",
                session_id,
                output_connector
            );

            // Find the output by connector name
            let output = state
                .workspaces
                .outputs()
                .find(|o| o.name() == output_connector);

            let output = match output {
                Some(o) => o.clone(),
                None => {
                    tracing::error!("Output not found: {}", output_connector);
                    return;
                }
            };

            // Get the session
            let session = match state.screenshare_sessions.get_mut(&session_id) {
                Some(s) => s,
                None => {
                    tracing::error!("Session not found: {}", session_id);
                    return;
                }
            };

            // Check if already recording this output
            if session.streams.contains_key(&output_connector) {
                tracing::warn!("Already recording output: {}", output_connector);
                return;
            }

            // Get output dimensions for stream config
            let (width, height, refresh_rate) = output
                .current_mode()
                .map(|m| (m.size.w as u32, m.size.h as u32, m.refresh as u32))
                .unwrap_or((1920, 1080, 60000));

            // Create PipeWire stream
            let config = StreamConfig {
                width,
                height,
                framerate_num: refresh_rate / 1000, // Convert mHz to Hz
                framerate_denom: 1,
                ..Default::default()
            };
            let (pipewire_stream, frame_sender) = PipeWireStream::new(config);

            // Create output ID for the tap
            let output_id = OutputId::from_output(&output);

            // Create the session tap
            let tap = ScreencastSessionTap::new(
                session_id.clone(),
                output_id,
                frame_sender.clone(),
            );

            // Register the tap with the frame tap manager
            let tap_token = state.frame_tap_manager.register(Arc::new(tap));

            tracing::info!(
                "Registered frame tap for session={}, output={}, token={:?}",
                session_id,
                output_connector,
                tap_token
            );

            // Store the active stream
            session.streams.insert(
                output_connector.clone(),
                ActiveStream {
                    output_connector,
                    tap_token,
                    pipewire_stream,
                    frame_sender,
                },
            );
        }
        CompositorCommand::StopRecording {
            session_id,
            output_connector,
        } => {
            tracing::info!(
                "StopRecording: session={}, output={}",
                session_id,
                output_connector
            );

            // Get the session
            let session = match state.screenshare_sessions.get_mut(&session_id) {
                Some(s) => s,
                None => {
                    tracing::error!("Session not found: {}", session_id);
                    return;
                }
            };

            // Remove and stop the stream
            if let Some(stream) = session.streams.remove(&output_connector) {
                // Unregister the tap
                state.frame_tap_manager.unregister(stream.tap_token);
                tracing::info!(
                    "Unregistered frame tap for session={}, output={}",
                    session_id,
                    output_connector
                );
                // PipeWire stream will be dropped here
            } else {
                tracing::warn!(
                    "No active stream for output {} in session {}",
                    output_connector,
                    session_id
                );
            }
        }
        CompositorCommand::GetPipeWireFd {
            session_id,
            response_tx,
        } => {
            tracing::info!("GetPipeWireFd: session={}", session_id);
            // TODO: Return actual PipeWire FD once PipeWire integration is complete
            // For now, return an error indicating it's not yet implemented
            let _ = response_tx.send(Err("PipeWire integration not yet complete".into()));
        }
        CompositorCommand::DestroySession { session_id } => {
            tracing::info!("DestroySession: session={}", session_id);

            // Remove the session and clean up all streams
            if let Some(session) = state.screenshare_sessions.remove(&session_id) {
                for (connector, stream) in session.streams {
                    state.frame_tap_manager.unregister(stream.tap_token);
                    tracing::info!(
                        "Unregistered frame tap for destroyed session={}, output={}",
                        session_id,
                        connector
                    );
                }
            } else {
                tracing::warn!("Session not found for destruction: {}", session_id);
            }
        }
    }
}
