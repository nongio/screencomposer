# Virtual Screen Design

## Overview

Virtual screens are headless outputs rendered entirely in memory without physical display hardware. They provide configurable displays that can be accessed remotely via PipeWire streaming, enabling use cases like:

- Remote screen viewing from other computers (view-only initially)
- Screen recording/broadcasting
- Virtual displays for testing
- Dedicated application workspaces accessible from other devices

**Initial Scope:** View-only streaming. Interactive input handling deferred to future phases.

## Architecture

### High-Level Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    ScreenComposer                           │
│                                                             │
│  ┌──────────────┐    ┌──────────┐    ┌─────────────────┐  │
│  │ Virtual      │───▶│  Skia    │───▶│ Memory Buffers  │  │
│  │ Output       │    │ Renderer │    │ (SHM/DMA-BUF)   │  │
│  │ (Smithay)    │    │ Pipeline │    │                 │  │
│  └──────────────┘    └──────────┘    └────────┬────────┘  │
│                                                │            │
│                                                ▼            │
│                                       ┌─────────────────┐  │
│                                       │ PipeWire Stream │  │
│                                       │  (Video Source) │  │
│                                       └────────┬────────┘  │
└────────────────────────────────────────────────┼───────────┘
                                                 │
                          ┌──────────────────────┴────────────────────┐
                          │                                           │
                          ▼                                           ▼
                  ┌───────────────┐                          ┌────────────────┐
                  │ RDP Server    │                          │ WebRTC Server  │
                  │ (e.g. FreeRDP)│                          │                │
                  └───────┬───────┘                          └────────┬───────┘
                          │                                           │
                          ▼                                           ▼
                  ┌───────────────┐                          ┌────────────────┐
                  │ Network       │                          │ Network        │
                  │ (TCP/IP)      │                          │ (WebSocket)    │
                  └───────┬───────┘                          └────────┬───────┘
                          │                                           │
                          ▼                                           ▼
                  ┌───────────────┐                          ┌────────────────┐
                  │ RDP Client    │                          │ Web Browser    │
                  │ (View-only)   │                          │ (View-only)    │
                  └───────────────┘                          └────────────────┘

                  (Input events handling deferred to future phases)
```

## Components

### 1. Virtual Output Manager (`src/virtual_output/mod.rs`)

**Responsibilities:**
- Create headless Smithay `Output` instances from config
- Manage virtual output lifecycle (enable/disable)
- Handle resolution/refresh rate changes
- Integrate with existing output management

**Key Types:**
```rust
pub struct VirtualOutput {
    output: Output,
    config: VirtualScreenConfig,
    buffer_pool: BufferPool,
    damage_tracker: OutputDamageTracker,
}

pub struct VirtualScreenConfig {
    name: String,
    width: u32,
    height: u32,
    refresh_rate: u32,
    scale: f64,
    enabled: bool,
    pipewire_node_name: String,
}
```

### 2. Rendering Pipeline Integration

**Extend existing render pipeline:**
- Virtual outputs use same `OutputRenderElements` as physical outputs
- Render to memory buffers (SHM or DMA-BUF)
- Use existing Skia renderer and damage tracking
- No scanout, only buffer composition

**Implementation:**
```rust
// In src/render.rs
pub fn render_virtual_output(
    renderer: &mut SkiaRenderer,
    output: &Output,
    elements: &[OutputRenderElements],
) -> Result<MemoryBuffer, RenderError>
```

### 3. PipeWire Stream Extension (`src/screenshare/virtual_stream.rs`)

**Extend existing screenshare infrastructure:**
- New stream type: continuous vs on-demand
- Virtual outputs always stream when enabled
- Reuse buffer management from `pipewire_stream.rs`
- Support multiple concurrent virtual screens

**Stream Management:**
```rust
pub struct VirtualScreenStream {
    stream: PipeWireStream,
    output: Output,
    running: bool,
}

impl VirtualScreenStream {
    pub fn new(config: &VirtualScreenConfig) -> Result<Self>;
    pub fn push_frame(&mut self, buffer: &MemoryBuffer);
    pub fn stop(&mut self);
}
```

### 4. Configuration (`sc_config.toml`)

```toml
# Virtual screen configuration
[[virtual_screens]]
name = "virtual-1"
enabled = true
width = 1920
height = 1080
refresh_rate = 60
scale = 1.0
pipewire_node_name = "ScreenComposer Virtual-1"

[[virtual_screens]]
name = "virtual-2"
enabled = false
width = 2560
height = 1440
refresh_rate = 30
scale = 1.5
pipewire_node_name = "ScreenComposer Virtual-2"
```

**Config Structure:**
```rust
// In src/config/mod.rs
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // ... existing fields
    pub virtual_screens: Vec<VirtualScreenConfig>,
}
```

## Remote Access Protocols

### Option 1: RDP Server (Recommended for Multi-Platform)

**Separate component:** `components/rdp-server/`

**Features:**
- Consume PipeWire video stream
- TLS encryption
- Authentication support
- Multi-client support (view-only)
- _(Input event forwarding deferred to Phase 4)_

**Stack:**
- `freerdp3` or custom RDP implementation
- PipeWire consumer

**Configuration:**
```toml
[rdp_server]
enabled = true
port = 3389
bind_address = "0.0.0.0"
tls_cert = "/path/to/cert.pem"
tls_key = "/path/to/key.pem"
allowed_users = ["user1", "user2"]

# Map virtual screens to RDP sessions
[[rdp_server.screens]]
virtual_screen = "virtual-1"
session_name = "primary"
```

### Option 2: WebRTC Server (Recommended for Browser Access)

**Separate component:** `components/webrtc-server/`

**Features:**
- Browser-based client (no install needed)
- Low latency with proper STUN/TURN
- WebSocket signaling
- View-only streaming
- _(Input forwarding deferred to Phase 4)_

**Stack:**
- `webrtc-rs` or `gstreamer-webrtc`
- HTTP/WebSocket server (e.g., `axum`)
- PipeWire consumer

**Configuration:**
```toml
[webrtc_server]
enabled = true
http_port = 8080
websocket_port = 8081
stun_servers = ["stun:stun.l.google.com:19302"]

[[webrtc_server.screens]]
virtual_screen = "virtual-1"
stream_id = "main-screen"
```

### Option 3: VNC Server (Alternative)

**If needed for legacy compatibility:**

**Component:** `components/vnc-server/`

**Stack:**
- `libvncserver` or Rust VNC implementation
- PipeWire consumer
- Input forwarding

## Input Event Handling (Future Enhancement)

**Note:** This section describes future interactive capabilities. Initial implementation is view-only.

For interactive remote access (Phase 4 and beyond):

### Input Injection via Wayland

**Custom Protocol:** `sc-remote-input-v1`
```xml
<protocol name="sc_remote_input">
  <interface name="sc_remote_input_v1" version="1">
    <request name="inject_pointer_motion">
      <arg name="output" type="object" interface="wl_output"/>
      <arg name="x" type="fixed"/>
      <arg name="y" type="fixed"/>
    </request>
    
    <request name="inject_pointer_button">
      <arg name="button" type="uint"/>
      <arg name="state" type="uint"/>
    </request>
    
    <request name="inject_key">
      <arg name="key" type="uint"/>
      <arg name="state" type="uint"/>
    </request>
  </interface>
</protocol>
```

**Alternative:** Use `wlr-virtual-pointer` and `wlr-virtual-keyboard` protocols (existing standard)

### Security Considerations

- Input injection requires authentication
- Limit input to specific virtual screen only
- Rate limiting to prevent DoS
- View-only mode by default

## Implementation Phases

### Phase 1: Virtual Output Foundation
**Goal:** Headless outputs working locally

- [ ] Add `virtual_screens` config parsing
- [ ] Implement `VirtualOutput` type
- [ ] Integrate with existing output management
- [ ] Render to memory buffers
- [ ] Verify with compositor state inspection

**Deliverable:** Virtual screens exist and render, visible in `wlr-randr`

### Phase 2: PipeWire Integration
**Goal:** Virtual screens streamable via PipeWire

- [ ] Extend screenshare system for continuous streams
- [ ] Implement `VirtualScreenStream`
- [ ] Auto-start streams when virtual screen enabled
- [ ] Test with `pw-play` or GStreamer pipeline

**Deliverable:** Virtual screen video accessible via PipeWire

### Phase 3: Remote Access Server (Choose One)
**Goal:** View-only remote streaming from another device

**Option A: RDP Server**
- [ ] Create `components/rdp-server` package
- [ ] PipeWire consumer implementation
- [ ] RDP protocol handling
- [ ] Authentication/TLS

**Option B: WebRTC Server**
- [ ] Create `components/webrtc-server` package
- [ ] HTTP/WebSocket server
- [ ] WebRTC peer connection handling
- [ ] Browser client UI

**Deliverable:** Remote client can view virtual screen (view-only)

### Phase 4: Input Injection (Optional - Future Enhancement)
**Goal:** Interactive remote access

**Note:** This phase is optional and can be deferred. Many use cases (monitoring, broadcasting, recording) work perfectly with view-only access.

- [ ] Implement custom input protocol or use `wlr-virtual-*`
- [ ] Input forwarding from remote server
- [ ] Per-output input focus
- [ ] Security/authentication

**Deliverable:** Full interactive remote desktop functionality

### Phase 5: Production Polish (For View-Only System)
**Goal:** Reliable, secure, performant streaming

- [ ] Performance optimization (rendering, encoding)
- [ ] Bandwidth adaptation
- [ ] Multi-client support
- [ ] Logging/metrics
- [ ] Documentation
- [ ] Example configurations

## Dependencies

### New Crates Needed

**Core:**
```toml
# For memory buffer management
shm = "0.2"

# Already have PipeWire support
# pipewire = "0.8"
```

**For RDP Server:**
```toml
# Option 1: Use FreeRDP library
freerdp-sys = "..."  # Custom bindings if needed

# Option 2: Pure Rust (if available)
rdp-rs = "..."  # Check if exists/mature enough
```

**For WebRTC Server:**
```toml
webrtc = "0.9"
axum = "0.7"  # HTTP/WebSocket server
tokio-tungstenite = "0.21"  # WebSocket
```

## Testing Strategy

### Unit Tests
- Virtual output configuration parsing
- Buffer management
- Stream lifecycle

### Integration Tests
- Virtual output creation/destruction
- Rendering to virtual outputs
- PipeWire stream continuity

### Manual Tests
- Connect with RDP/WebRTC client
- Input responsiveness
- Multi-client scenarios
- Resolution changes
- Performance under load

### Test Clients
Create simple test utilities:
- `test-virtual-screen.sh` — Create and verify virtual output
- `test-pipewire-stream.sh` — Verify PipeWire stream from virtual output
- Web client for WebRTC testing
- Example RDP client configuration

## Future Enhancements

- **Dynamic resolution:** Change resolution on-the-fly based on client
- **Multiple bitrates:** Adaptive streaming based on network conditions
- **Recording mode:** Direct recording to file without network streaming
- **Virtual screen mirroring:** Mirror existing physical output
- **Clipboard synchronization:** Remote clipboard access
- **Audio forwarding:** If virtual screen apps produce audio
- **H.264/H.265 encoding:** Hardware-accelerated encoding for efficiency

## References

- [PipeWire Video Support](https://docs.pipewire.org/)
- [Smithay Output Management](https://smithay.github.io/smithay/)
- [RDP Protocol Spec](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdp/)
- [WebRTC Specs](https://www.w3.org/TR/webrtc/)
- Existing: [docs/screenshare.md](./screenshare.md) — Current screenshare implementation
