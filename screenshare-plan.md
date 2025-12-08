# Plan: Compositor-Side Screenshare Integration

**Status as of 2025-12-08:**
  - Phase 1 (D-Bus service infrastructure): ✅ Complete
  - Phase 3 (Damage tracking): ✅ Complete
  - Phase 2 (PipeWire integration): ✅ Complete
  - **Command handler wiring**: ✅ Complete
    - Session state management in compositor (`ScreencastSession`, `ActiveStream`)
    - `CreateSession`, `StartRecording`, `StopRecording`, `DestroySession` handlers implemented
    - Frame tap registration/unregistration working
  - **Frame capture**: ✅ Complete (winit, udev)
    - RGBA frame capture integrated into render loops
    - `notify_rgba_with_damage()` called with captured frames
    - Damage regions passed through
  - **PipeWire stream**: ✅ Complete
    - Real PipeWire initialization with MainLoopBox, ContextBox, StreamBox
    - Video format negotiation (BGRA/RGBA with dimensions and framerate)
    - SHM buffer handling for RGBA frames
    - Stream runs on dedicated thread with proper synchronization
    - Node ID and FD retrieval working
  - D-Bus API now matches portal expectations and is fully dynamic (sessions/streams registered at runtime)
  - All code compiles

---

Complete the screenshare pipeline by adding a D-Bus service to the compositor that the xdg-desktop-portal-sc talks to, wire up FrameTapManager for frame capture, and implement damage tracking to skip unchanged frames.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Application (OBS, GNOME, etc.)                                             │
│       │                                                                     │
│       ▼ (Portal D-Bus API)                                                  │
│  xdg-desktop-portal                                                         │
│       │                                                                     │
│       ▼ (Portal Backend API)                                                │
│  xdg-desktop-portal-sc  ◄── components/xdg-desktop-portal-sc/ (✅ done)     │
│       │                                                                     │
│       ▼ (Compositor D-Bus API)                                              │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │  org.screencomposer.ScreenCast  ◄── NEW D-Bus service       │            │
│  │       │                                                     │            │
│  │       ▼                                                     │            │
│  │  FrameTapManager  ◄── src/screenshare/frame_tap.rs (exists) │            │
│  │       │                                                     │            │
│  │       ▼                                                     │            │
│  │  PipeWire Stream  ◄── feeds frames to PipeWire              │            │
│  └─────────────────────────────────────────────────────────────┘            │
│       │                                                                     │
│       ▼ (PipeWire fd)                                                       │
│  Application receives video stream                                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Steps

### Phase 1: Compositor D-Bus Service

1. **Add zbus dependency to compositor** (`Cargo.toml`) ✅
   - `zbus = "4"` and `pipewire = "0.9"` added
   - Tokio features enabled

2. **Create D-Bus service module** (`src/screenshare/dbus_service.rs`) ✅
   - Implements `org.screencomposer.ScreenCast`, `Session`, and `Stream` interfaces
   - D-Bus API signatures now match portal/xdg-desktop-portal-sc expectations
   - Sessions and streams are registered dynamically at runtime

3. **Bridge async D-Bus ↔ sync compositor** (`src/screenshare/mod.rs`) ✅
   - D-Bus server runs on a tokio thread
   - Uses calloop channel for D-Bus→compositor commands
   - Commands: `CreateSession`, `StartRecording`, `StopRecording`, `GetPipeWireFd`, etc.

4. **Wire FrameTapManager into compositor state** (`src/state/mod.rs`) ✅
   - `frame_tap_manager: FrameTapManager` added to state
   - Initialized at startup

### Phase 2: PipeWire Stream Integration

5. **Create ScreencastSessionTap** (`src/screenshare/session_tap.rs`) ✅
   - Implements `FrameTap` trait
   - Filters frames by `OutputId`
   - Sends frames to PipeWire via mpsc channel

6. **PipeWire stream setup** (`src/screenshare/pipewire_stream.rs`) ✅
   - Real PipeWire implementation with MainLoopBox, ContextBox, StreamBox
   - Video format negotiation (BGRA/RGBA/BGRx/RGBx)
   - SHM buffer handling with automatic allocation (ALLOC_BUFFERS flag)
   - Dedicated thread with proper atomic synchronization
   - FD export for portal clients

### Phase 3: Damage Tracking

7. **Extend `FrameMeta` with damage fields** (`src/screenshare/frame_tap.rs`) ✅
   - `damage: Option<Vec<Rectangle<i32, Physical>>>` and `has_damage: bool` added

8. **Add damage-aware notification methods in `FrameTapManager`** (`src/screenshare/frame_tap.rs`) ✅
   - `notify_dmabuf_with_damage()` and `notify_rgba_with_damage()` implemented

9. **Hook into render loop** (`src/udev.rs`, `src/winit.rs`) ✅
   - Frame capture implemented in both winit and udev render loops
   - RGBA frames captured after render, before buffer swap
   - `notify_rgba_with_damage()` called with captured frames
   - x11 backend skipped (not required)

10. **Update PipeWire stream with damage hints** (`src/screenshare/pipewire_stream.rs`) ⏳
    - **TODO (optional enhancement):** Implement SPA_META_REGION support for damage rectangles

## Implementation Complete

### What's Working
- **Full screenshare pipeline:**
  - D-Bus service infrastructure (Phase 1)
  - PipeWire integration with real stream (Phase 2)
  - Damage tracking in frame metadata (Phase 3)
  - Command handlers wired to compositor state
  - Frame capture in winit and udev backends

### Architecture
```
                   Application (OBS, GNOME, etc.)
                           │
                           ▼
                   xdg-desktop-portal
                           │
                           ▼
                   xdg-desktop-portal-sc (portal backend)
                           │
                           ▼ D-Bus API
    ┌──────────────────────────────────────────────────────┐
    │            org.screencomposer.ScreenCast             │
    │                      │                               │
    │    ┌─────────────────┴─────────────────┐             │
    │    │      ScreencastSession            │             │
    │    │  ┌─────────────────────────────┐  │             │
    │    │  │    ScreencastSessionTap     │  │             │
    │    │  │  (filters by output, sends  │  │             │
    │    │  │   frames via mpsc channel)  │  │             │
    │    │  └──────────────┬──────────────┘  │             │
    │    │                 │                 │             │
    │    │  ┌──────────────▼──────────────┐  │             │
    │    │  │      PipeWireStream         │  │             │
    │    │  │  (dedicated thread with     │  │             │
    │    │  │   MainLoop, Context, Core,  │  │             │
    │    │  │   Stream + buffer handling) │  │             │
    │    │  └─────────────────────────────┘  │             │
    │    └───────────────────────────────────┘             │
    │                      │                               │
    │   FrameTapManager    │  (receives frames from       │
    │   (in compositor)    │   render loop)               │
    └──────────────────────│───────────────────────────────┘
                           │
                           ▼ PipeWire FD
                   Application receives video stream
```

### Testing
- Start compositor: `cargo run -- --winit`
- Use `pw-dump` to verify PipeWire node appears when stream starts
- Test with GNOME Screen Recorder or OBS

### Key Files
- `src/screenshare/mod.rs` - Session state + command handlers
- `src/screenshare/pipewire_stream.rs` - PipeWire stream implementation
- `src/screenshare/session_tap.rs` - Frame tap → channel
- `src/screenshare/frame_tap.rs` - FrameTapManager and damage tracking
- `src/winit.rs`, `src/udev.rs` - Frame capture integration

### Future Enhancements
- SPA_META_REGION for efficient damage hints to consumers
- DMA-BUF support for zero-copy frame sharing
- Window-level capture (currently only output/monitor capture)

---
