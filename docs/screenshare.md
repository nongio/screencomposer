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
│  │  │ CreateSession   │◄───│──│ RGBA Frame Capture             │  │   │    │
│  │  │ StartRecording  │    │  │ (capture_rgba_frame)           │  │   │    │
│  │  │ StopRecording   │    │  └────────────┬───────────────────┘  │   │    │
│  │  │ DestroySession  │    │               │                      │   │    │
│  │  └────────┬────────┘    └───────────────│──────────────────────┘   │    │
│  │           │                             │                          │    │
│  │           ▼ calloop channel             ▼                          │    │
│  │  ┌─────────────────────────────────────────────────────────────┐   │    │
│  │  │                    FrameTapManager                          │   │    │
│  │  │  - Manages registered frame taps                            │   │    │
│  │  │  - Distributes frames to interested taps                    │   │    │
│  │  │  - Tracks damage regions                                    │   │    │
│  │  └────────────────────────┬────────────────────────────────────┘   │    │
│  │                           │                                        │    │
│  │                           ▼                                        │    │
│  │  ┌─────────────────────────────────────────────────────────────┐   │    │
│  │  │              ScreencastSessionTap                           │   │    │
│  │  │  - Filters frames by output                                 │   │    │
│  │  │  - Sends frames via mpsc channel                            │   │    │
│  │  └────────────────────────┬────────────────────────────────────┘   │    │
│  │                           │                                        │    │
│  │                           ▼ tokio mpsc channel                     │    │
│  │  ┌─────────────────────────────────────────────────────────────┐   │    │
│  │  │               PipeWireStream (dedicated thread)             │   │    │
│  │  │  - MainLoop, Context, Core, Stream                          │   │    │
│  │  │  - SHM buffer management                                    │   │    │
│  │  │  - Video format negotiation                                 │   │    │
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

### 2. Frame Tap Manager (`src/screenshare/frame_tap.rs`)

The `FrameTapManager` receives frames from the compositor's render loop and distributes
them to registered taps. It supports:

- **RGBA frames**: CPU-accessible pixel data
- **DMA-BUF frames**: GPU buffer handles (zero-copy path)
- **Damage tracking**: Only notifies taps when content changes

**Key types:**

```rust
pub trait FrameTap: Send + Sync {
    fn on_frame_rgba(&self, output: &OutputId, frame: RgbaFrame, meta: &FrameMeta);
    fn on_frame_dmabuf(&self, output: &OutputId, dmabuf: &Dmabuf, meta: &FrameMeta);
    fn wants_output(&self, output: &OutputId) -> bool;
}

pub struct FrameMeta {
    pub size: (u32, u32),
    pub stride: u32,
    pub fourcc: u32,
    pub time_ns: u64,
    pub modifier: Option<u64>,
    pub has_damage: bool,
    pub damage: Option<Vec<Rectangle<i32, Physical>>>,
}
```

### 3. Session Tap (`src/screenshare/session_tap.rs`)

`ScreencastSessionTap` implements `FrameTap` and acts as the bridge between the
frame tap manager and PipeWire:

- Filters frames by target output
- Converts frame metadata to a thread-safe snapshot
- Sends frames via tokio mpsc channel to the PipeWire thread

### 4. PipeWire Stream (`src/screenshare/pipewire_stream.rs`)

The `PipeWireStream` manages the actual PipeWire video stream:

**Features:**
- Runs on a dedicated thread with its own PipeWire main loop
- Creates `MainLoopBox`, `ContextBox`, and `StreamBox`
- Negotiates video format (BGRA/RGBA/BGRx/RGBx)
- Uses SHM buffers with automatic allocation
- Proper cleanup on stop/drop

**Configuration:**

```rust
pub struct StreamConfig {
    pub width: u32,           // Stream width
    pub height: u32,          // Stream height
    pub framerate_num: u32,   // Framerate numerator (e.g., 60)
    pub framerate_denom: u32, // Framerate denominator (e.g., 1)
    pub format: u32,          // Pixel format (FourCC)
    pub prefer_dmabuf: bool,  // DMA-BUF preference
}
```

### 5. Command Handler (`src/screenshare/mod.rs`)

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

## Frame Capture

Frame capture is integrated into the render loops:

### Winit Backend (`src/winit.rs`)

```rust
// After render_frame(), capture RGBA data
let captured_frame = crate::screenshare::capture_rgba_frame(renderer, size);

// After buffer swap, notify frame taps
if let Some(frame) = captured_frame {
    state.frame_tap_manager.notify_rgba_with_damage(
        &output,
        frame,
        Fourcc::Abgr8888,
        time,
        None,
        damage.as_deref(),
    );
}
```

### Udev Backend (`src/udev.rs`)

Similar pattern, with `capture_rgba_frame()` called after `render_frame()`.

## Usage

### Starting the Compositor

```bash
cargo run -- --winit
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

1. Open OBS Studio and add a "Screen Capture (PipeWire)" source
2. Use GNOME's built-in screen recorder
3. Share screen in Firefox or Chromium

## File Overview

| File | Purpose |
|------|---------|
| `src/screenshare/mod.rs` | Module root, session state, command handlers |
| `src/screenshare/dbus_service.rs` | D-Bus interface implementation |
| `src/screenshare/frame_tap.rs` | FrameTap trait, FrameTapManager, capture utilities |
| `src/screenshare/session_tap.rs` | ScreencastSessionTap implementation |
| `src/screenshare/pipewire_stream.rs` | PipeWire stream management |
| `src/winit.rs` | Frame capture integration (winit backend) |
| `src/udev.rs` | Frame capture integration (udev backend) |

## Future Enhancements

- **SPA_META_REGION**: Efficient damage hints to PipeWire consumers
- **DMA-BUF support**: Zero-copy GPU buffer sharing
- **Window capture**: Capture individual windows instead of full outputs
- **Cursor metadata**: Separate cursor position/image stream
