# Plan: Compositor-Side Screenshare Integration

**Status as of 2025-12-08:**
  - Phase 1 (D-Bus service infrastructure): ✅ Complete
  - Phase 3 (Damage tracking): ✅ Complete
  - Phase 2 (PipeWire integration): ⏳ In Progress
  - **NEW: Command handler wiring**: ✅ Complete
    - Session state management in compositor (`ScreencastSession`, `ActiveStream`)
    - `CreateSession`, `StartRecording`, `StopRecording`, `DestroySession` handlers implemented
    - Frame tap registration/unregistration working
  - **NEW: Frame capture**: ✅ Complete (winit, udev)
    - RGBA frame capture integrated into render loops
    - `notify_rgba_with_damage()` called with captured frames
    - Damage regions passed through
  - D-Bus API now matches portal expectations and is fully dynamic (sessions/streams registered at runtime)
  - All code compiles and passes tests
  - Handover: Next agent should focus on **real PipeWire stream implementation** (currently placeholder)

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

5. **Create ScreencastSessionTap** (`src/screenshare/session_tap.rs`) ✅ (skeleton)
   - Implements `FrameTap` trait
   - Filters frames by `OutputId`
   - Frame delivery to PipeWire: ⏳ To be completed

6. **PipeWire stream setup** (`src/screenshare/pipewire_stream.rs`) ⏳
   - PipeWireStream struct and config exist (skeleton)
   - **TODO:** Implement real PipeWire stream creation, buffer negotiation, and fd export
   - **TODO:** Wire up frame delivery from ScreencastSessionTap to PipeWireStream

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
    - **TODO:** Implement SPA_META_REGION support for damage rectangles

## Handover Notes (for next agent)

### What's Now Working
- **Command handler wiring complete:**
  - `ScreencastSession` and `ActiveStream` structs in `src/screenshare/mod.rs`
  - `screenshare_sessions: HashMap` in compositor state
  - `CreateSession` creates session in compositor state
  - `StartRecording` creates `ScreencastSessionTap` + `PipeWireStream`, registers tap
  - `StopRecording` unregisters tap, removes stream
  - `DestroySession` cleans up all streams in session
- **Frame capture working:**
  - winit: RGBA capture inside render closure
  - udev: RGBA capture via `capture_rgba_frame()` after `render_frame()`
  - Frames flow through `ScreencastSessionTap` → mpsc channel → `PipeWireStream`
- **D-Bus API correct and dynamic:**
  - Sessions and streams registered at runtime
  - All method signatures match portal client

### PipeWire Integration (Main Remaining Task)
The `PipeWireStream` in `src/screenshare/pipewire_stream.rs` is still a skeleton:
1. **`start()`** - Currently returns random placeholder node_id. Need real PipeWire init:
   - Create `pw_context`, `pw_core`, `pw_stream`
   - Negotiate video format (RGBA/BGRA, dimensions, framerate)
   - Get actual node_id from stream
2. **`pump_loop()`** - Receives frames but doesn't send them anywhere:
   - `handle_rgba_frame()` needs to copy data to PipeWire SHM buffer and queue
   - `handle_dmabuf_frame()` needs DMA-BUF import (if supported)
3. **`GetPipeWireFd`** - Returns error. Need to export PipeWire socket FD
4. **SPA_META_REGION** - Damage hints not implemented

### Testing
After PipeWire implementation:
- Start compositor
- Use `dbus-send` or portal to create session and start recording
- Verify PipeWire node appears in `pw-dump`
- Test with GNOME Screen Recorder or OBS

### Key Files
- `src/screenshare/mod.rs` - Session state + command handlers
- `src/screenshare/pipewire_stream.rs` - **Main focus for PipeWire work**
- `src/screenshare/session_tap.rs` - Frame tap → channel
- `components/xdg-desktop-portal-sc/src/screencomposer_client/screencast.rs` - Client expectations

---
