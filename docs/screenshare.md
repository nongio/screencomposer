# Screen Sharing in ScreenComposer

This document describes the screen sharing implementation in ScreenComposer, including
architecture, D-Bus API, and PipeWire integration.

## Overview

ScreenComposer implements screen sharing via the xdg-desktop-portal standard, allowing
applications like OBS, Chromium, Firefox, and GNOME tools to capture screen content
through PipeWire video streams.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Application (OBS, Chromium, Firefox, GNOME Screen Recorder)                │
│       │                                                                     │
│       ▼ Portal D-Bus API (org.freedesktop.portal.ScreenCast)                │
│                                                                             │
│  xdg-desktop-portal (system service)                                        │
│       │                                                                     │
│       ▼ Backend D-Bus API (org.freedesktop.impl.portal.ScreenCast)          │
│                                                                             │
│  xdg-desktop-portal-sc                                                      │
│  (components/xdg-desktop-portal-sc/)                                        │
│       │                                                                     │
│       ▼ Compositor D-Bus API (org.screencomposer.ScreenCast)                │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  ScreenComposer Compositor                                          │    │
│  │                                                                     │    │
│  │  ┌─────────────────┐    ┌──────────────────────────────────────┐   │    │
│  │  │ D-Bus Service   │    │ Render Loop (winit/udev)             │   │    │
│  │  │ (tokio thread)  │    │                                      │   │    │
│  │  │                 │    │  ┌────────────────────────────────┐  │   │    │
│  │  │ CreateSession   │◄───│──│ Direct GPU Blit                │  │   │    │
│  │  │ StartRecording  │    │  │ (Blit<Dmabuf> trait)           │  │   │    │
│  │  │ StopRecording   │    │  │ • glBlitFramebuffer            │  │   │    │
│  │  │ DestroySession  │    │  │ • Damage-aware regions         │  │   │    │
│  │  └────────┬────────┘    │  │ • GPU-only (no CPU copy)       │  │   │    │
│  │           │             │  └────────────┬───────────────────┘  │   │    │
│  │           ▼             └───────────────│──────────────────────┘   │    │
│  │  calloop channel                        ▼                          │    │
│  │  ┌─────────────────────────────────────────────────────────────┐   │    │
│  │  │  Session Management                                         │   │    │
│  │  │  • Active screenshare sessions                              │   │    │
│  │  │  • PipeWire buffer pool management                          │   │    │
│  │  │  • Per-output stream tracking                               │   │    │
│  │  └────────────────────────┬────────────────────────────────────┘   │    │
│  │                           │                                        │    │
│  │                           ▼                                        │    │
│  │  ┌─────────────────────────────────────────────────────────────┐   │    │
│  │  │               PipeWireStream (dedicated thread)             │   │    │
│  │  │  - MainLoop, Context, Core, Stream                          │   │    │
│  │  │  - DMA-BUF buffer management                                │   │    │
│  │  │  - Video format negotiation (BGRA preferred)                │   │    │
│  │  │  - VideoDamage metadata (SPA_META_VideoDamage)              │   │    │
│  │  └────────────────────────┬────────────────────────────────────┘   │    │
│  │                           │                                        │    │
│  └───────────────────────────│────────────────────────────────────────┘    │
│                              │                                             │
│                              ▼ PipeWire video stream                       │
│                                                                             │
│  Application receives video frames via PipeWire                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Components

### 1. D-Bus Service (`src/screenshare/dbus_service.rs`)

The compositor exposes a D-Bus service at `org.screencomposer.ScreenCast` that the
portal backend uses to control screen sharing sessions.

**Interfaces:**

| Interface | Path | Description |
|-----------|------|-------------|
| `org.screencomposer.ScreenCast` | `/org/screencomposer/ScreenCast` | Main service interface |
| `org.screencomposer.ScreenCast.Session` | `/org/screencomposer/ScreenCast/session/<id>` | Per-session control |
| `org.screencomposer.ScreenCast.Stream` | `/org/screencomposer/ScreenCast/stream/<id>` | Per-stream control |

**Methods:**

```
org.screencomposer.ScreenCast:
  CreateSession(properties: a{sv}) -> session_path: o
  ListOutputs() -> connectors: as

org.screencomposer.ScreenCast.Session:
  RecordMonitor(connector: s, properties: a{sv}) -> stream_path: o
  RecordWindow(properties: a{sv}) -> stream_path: o
  Start()
  Stop()
  OpenPipeWireRemote(options: a{sv}) -> fd: h

org.screencomposer.ScreenCast.Stream:
  Start()
  Stop()
  PipeWireNode() -> info: a{sv}
  Metadata() -> info: a{sv}
```

### 2. PipeWire Stream (`src/screenshare/pipewire_stream.rs`)

The `PipeWireStream` manages the actual PipeWire video stream:

**Features:**
- Runs on a dedicated thread with its own PipeWire main loop
- Creates `MainLoopBox`, `ContextBox`, and `StreamBox`
- Negotiates video format (BGRA preferred for GPU compatibility)
- Uses DMA-BUF buffers for zero-copy GPU rendering
- Single buffer mode (min=1, max=1) for optimal damage tracking
- Advertises VideoDamage metadata (SPA_META_VideoDamage) for client-side optimizations
- Proper cleanup on stop/drop

**Configuration:**

```rust
pub struct StreamConfig {
    pub width: u32,              // Stream width
    pub height: u32,             // Stream height
    pub framerate_num: u32,      // Framerate numerator (e.g., 60)
    pub framerate_denom: u32,    // Framerate denominator (e.g., 1)
    pub gbm_device: Option<Arc<GbmDevice<DrmDeviceFd>>>,
    pub capabilities: BackendCapabilities,
}
```

### 3. Command Handler (`src/screenshare/mod.rs`)

The command handler bridges async D-Bus operations with the sync compositor loop:

```rust
pub enum CompositorCommand {
    CreateSession { session_id, response_tx },
    StartRecording { session_id, output_connector, stream_id, response_tx },
    StopRecording { session_id, stream_id, response_tx },
    DestroySession { session_id, response_tx },
    GetPipeWireFd { session_id, response_tx },
}
```

Commands are sent via a calloop channel from the tokio D-Bus thread to the
compositor's main loop.

## Frame Delivery

Screen sharing uses a **direct GPU blit** approach for maximum performance:

### Udev Backend (`src/udev.rs`)

After successful rendering, frames are delivered directly to PipeWire buffers:

```rust
// After render_surface() succeeds
if outcome.rendered && !self.screenshare_sessions.is_empty() {
    for (_session_id, session) in &self.screenshare_sessions {
        for (connector, stream) in &session.streams {
            if connector == &output.name() {
                let buffer_pool = stream.pipewire_stream.buffer_pool();
                let mut pool = buffer_pool.lock().unwrap();
                
                if let Some(available) = pool.available.pop_front() {
                    // Direct GPU blit with damage awareness
                    crate::screenshare::fullscreen_to_dmabuf(
                        &mut renderer,
                        available.dmabuf,
                        size,
                        outcome.damage.as_deref(),  // Only blit damaged regions
                    )?;
                    
                    pool.to_queue.insert(available.fd, available.pw_buffer);
                    drop(pool);
                    stream.pipewire_stream.trigger_frame();
                }
            }
        }
    }
}
```

**Key Features:**
- **GPU-only path**: No CPU memcpy, direct FBO→dmabuf blit via `glBlitFramebuffer`
- **Damage-aware**: Only blits changed regions (or full frame if buffer changed)
- **Zero-copy**: Compositor renders once, PipeWire consumes GPU buffer directly
- **Synchronous**: Blit happens on main thread immediately after render

### Winit Backend (`src/winit.rs`)

Similar direct blit pattern using RGBA capture for development/testing.

## Usage

### Session Setup and Prerequisites

Screen sharing requires proper D-Bus session setup and PipeWire services. The compositor must share the same D-Bus session with applications.

**Required Services:**
- PipeWire (`pipewire.service`)
- PipeWire PulseAudio (`pipewire-pulse.service`)  
- WirePlumber (`wireplumber.service`)
- KDE Wallet or GNOME Keyring (for password management)

### Starting the Compositor

**Production (TTY/bare metal):**

Use the provided `start_session.sh` script for proper environment setup:

```bash
./scripts/start_session.sh
```

This script automatically:
1. Creates or reuses a D-Bus session
2. Saves D-Bus info to `$XDG_RUNTIME_DIR/dbus-session` for other terminals
3. Starts/verifies PipeWire services via systemctl
4. Launches the xdg-desktop-portal-screencomposer backend
5. Starts the compositor with correct environment variables

**Development (windowed mode):**

```bash
cargo run --release -- --winit
```

Note: Windowed mode may have different D-Bus session requirements.

### Running Applications with Screen Sharing

To use Chrome, Firefox, OBS, etc. with screen sharing support:

**From another terminal on the same TTY:**

```bash
# Connect to the compositor session
source ./scripts/connect-to-session.sh

# Now run applications
google-chrome        # Chrome/Chromium
firefox             # Firefox
obs                 # OBS Studio
```

**Manual connection:**

```bash
# Load D-Bus session environment
source $XDG_RUNTIME_DIR/dbus-session

# Set Wayland display
export WAYLAND_DISPLAY=wayland-0
export XDG_SESSION_TYPE=wayland

# Run application
google-chrome
```

### Testing with D-Bus

```bash
# Create a session
dbus-send --session --print-reply \
  --dest=org.screencomposer.ScreenCast \
  /org/screencomposer/ScreenCast \
  org.screencomposer.ScreenCast.CreateSession \
  dict:string:variant:

# List available outputs
dbus-send --session --print-reply \
  --dest=org.screencomposer.ScreenCast \
  /org/screencomposer/ScreenCast \
  org.screencomposer.ScreenCast.ListOutputs
```

### Verifying PipeWire Stream

```bash
# Check if PipeWire node appears
pw-dump | grep screen-composer
```

### Testing with Applications

After setting up the portal backend (see `docs/xdg-desktop-portal.md`):

1. Start the compositor: `./scripts/start_session.sh`
2. In another terminal: `source ./scripts/connect-to-session.sh`
3. Launch an application and test screen sharing:
   - **Chrome/Chromium**: Visit a meeting site, click share screen
   - **OBS Studio**: Add a "Screen Capture (PipeWire)" source
   - **Firefox**: Start screen sharing in a web conference
   - **GNOME Screen Recorder**: Use built-in screen recorder

Expected performance: 60 FPS at full resolution (e.g., 2880x1920).

### Troubleshooting

**Screen sharing dialog shows no outputs:**
- Ensure the app is running in the same D-Bus session as the compositor
- Check `$DBUS_SESSION_BUS_ADDRESS` is set: `echo $DBUS_SESSION_BUS_ADDRESS`
- Verify portal is registered: `busctl --user list | grep screencomposer`
- Try: `source ./scripts/connect-to-session.sh` before launching the app

**Video freezes after a few seconds:**
- Verify PipeWire is running: `pgrep -x pipewire`
- Check compositor logs: `tail -f screencomposer.log`
- Ensure systemd user services are enabled:
  ```bash
  systemctl --user enable --now pipewire.service pipewire-pulse.service wireplumber.service
  ```
- Restart PipeWire: `systemctl --user restart pipewire.service`

**D-Bus connection errors:**
- The D-Bus session file is created when compositor starts
- If running from a different TTY, source the session file first
- Ensure `$XDG_RUNTIME_DIR/dbus-session` exists and is readable
- Check permissions: `ls -la $XDG_RUNTIME_DIR/dbus-session`

**Portal backend not found:**
- Verify portal is built: `ls target/release/xdg-desktop-portal-screencomposer`
- Check portal logs: `tail -f components/xdg-desktop-portal-sc/portal.log`
- Rebuild if needed: `cargo build -p xdg-desktop-portal-screencomposer --release`

## File Overview

| File | Purpose |
|------|---------|
| `src/screenshare/mod.rs` | Module root, session state, command handlers, direct blit utility |
| `src/screenshare/dbus_service.rs` | D-Bus interface implementation |
| `src/screenshare/pipewire_stream.rs` | PipeWire stream management, buffer pool, format negotiation |
| `src/screenshare/frame_tap.rs` | Frame capture utilities (legacy, not used for delivery) |
| `src/screenshare/session_tap.rs` | Session tap implementation (legacy, not used for delivery) |
| `src/skia_renderer.rs` | Blit<Dmabuf> trait implementation for direct GPU blitting |
| `src/udev.rs` | Direct blit integration (udev backend) |
| `src/winit.rs` | RGBA capture integration (winit backend) |

## Future Enhancements

- **Window capture**: Capture individual windows instead of full outputs
- **Cursor metadata**: Separate cursor position/image stream
- **Multiple buffer modes**: Experiment with multi-buffering for specific use cases
- **Configurable framerate cap**: Allow per-client or global FPS limit configuration

## Known Issues and Fixes

### Framerate Compatibility (January 2026)

**Issue**: When the compositor runs on high-refresh-rate displays (e.g., 120Hz), screensharing 
would fail with Chrome/WebRTC clients showing "no more input formats" error.

**Root Cause**: Commit 5ea901 changed the code to use the actual display refresh rate instead 
of hardcoding 60Hz. Chrome and most WebRTC implementations don't support PipeWire streams 
above 60fps, causing format negotiation to fail.

**Fix**: The screenshare framerate is now capped at 60fps regardless of the display's actual 
refresh rate:

```rust
let framerate_num = (refresh_rate / 1000).min(60); // Cap at 60fps for compatibility
```

This maintains compatibility with Chrome, Firefox, OBS, and other screenshare clients while 
still allowing the display to run at higher refresh rates (120Hz, 144Hz, etc.) for normal 
compositor operation.

**Future**: The FPS cap should be made configurable (e.g., `config.screenshare.max_fps`) to 
allow power users to experiment with higher framerates for specific clients that support them.

## Implementation Details

### GPU-Only Rendering Path (December 2024)

The screenshare implementation uses a pure GPU rendering path with no CPU roundtrips:

**DMA-BUF Direct Blitting**:
- Implements `Blit<Dmabuf>` trait for `SkiaRenderer`
- Direct GPU framebuffer blitting using `glBlitFramebuffer`
- No CPU memory access or pixel readback
- Achieves 60 FPS at 2880x1920 resolution

**Damage-Aware Optimization**:
- Tracks damaged regions from compositor render loop
- Only blits changed areas when rendering to same buffer
- Forces full blit on buffer changes
- Single buffer mode (min=1, max=1) for optimal damage tracking

**Implementation Files**:
- `src/skia_renderer.rs`: Blit<Dmabuf> trait, lazy SkiaTextureMapping
- `src/screenshare/mod.rs`: blit_to_dmabuf_direct() helper
- `src/udev.rs`: Damage-aware blitting in render loop

**PipeWire Integration**:
- Buffer pool management with single buffer
- VideoDamage metadata support (SPA_META_VideoDamage)
- Up to 16 damage rectangles per frame advertised
- BGRA format negotiation with DRM modifiers

**Performance**:
- GPU-only blitting: no CPU bottleneck
- Damage tracking: reduced GPU work for partial updates
- Verified working at 2880x1920@60fps consistently
