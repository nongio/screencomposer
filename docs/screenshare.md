Screensharing and screenshots: plan and phases

This document consolidates the screensharing feature plan. It’s the canonical reference for scope, design, and phased delivery. Issue pages in docs/issues/* link back here.

If you’re looking for the quick-start user doc, see docs/screensharing.md.

Contents at a glance
- Portal-first screencast path (via xdg-desktop-portal + PipeWire)
- Deterministic headless screenshots for CI
- Security and UX (recording indicator, sensitive windows)
- Deliverables and module layout (single-crate)
- Screencopy server details (zwlr_screencopy_v1)
- Policy and dynamic cast target
- Optional PipeWire publisher API
- CLI subcommands
- Acceptance tests and constraints

The full plan

Goal: Implement portal-first screen sharing (PipeWire) and UX parity with Niri: screencast monitors/windows via xdg-desktop-portal, support block sensitive windows, and dynamic cast target. Provide deterministic headless screenshots for CI.

Portal path (default for users)
Make ScreenComposer compatible with xdg-desktop-portal screencasting:
- Ensure we support zwlr_screencopy_v1 and linux-dmabuf (zero-copy when available).
- Ensure we expose window/monitor metadata sufficient for window and monitor selection.
- Add hooks to exclude specific surfaces from screencast ("sensitive windows").
- Add a control to switch the active cast target at runtime ("dynamic cast target").
- We do not implement our own portal backend now; we interoperate with existing backends (like xdg-desktop-portal-gnome or wlr).

Deterministic path (CI)
Add a headless output and a single-frame screenshot CLI for goldens:
- sc screen screenshot --out <png> [--output <name>] [--region x,y,w,h] [--frame N]
- This bypasses portals to keep CI reproducible.

Security
- Portal path: inherit portal prompts/indicators.
- Direct screencopy/screenshot path disabled by default; only enabled with --automation flag or config key. Add an on-screen "recording" dot overlay while active.

Deliverables / repo layout (single-crate)
Create or modify these files under the existing src/ tree. No workspace/crates split is required.
- src/screenshare/frame_tap.rs: A FrameTap that receives every composed frame (after render, before swap). Integrate this into the compositor’s render loop and virtual outputs (in src/udev.rs initially). Keep it behind the screenshare feature flag.
- src/screencopy/mod.rs (+ protocol.rs): Implement zwlr_screencopy_v1 minimal server side: capture_output, capture_output_region, frame acquire/commit lifecycle. Prefer dmabuf export; fallback to shm using a readback copy. Respect damage; throttle to chosen fps for screencast sessions. If Smithay provides a helper for screencopy at the pinned revision, prefer that. Otherwise, vendor the XML under src/screencopy/protocols/wlr-screencopy-unstable-v1.xml and generate code with wayland-scanner (mirroring src/sc_layer_shell/).
- src/screenshare/policy.rs: "Sensitive windows" policy + dynamic cast target. Maintain an allow/deny flag per toplevel (by app_id or surface_id). Public API: set_surface_screencast_blocked(surface, blocked) and set_cast_target(target: CastTarget::Output|Window). Screencopy/stream path must skip blocked surfaces when compositing to the screencast buffer.
- src/screenshare/pipewire.rs (optional): A PipeWirePublisher that can publish a video node from the FrameTap stream. Negotiate caps; use dmabuf planes when available; otherwise copy into SPA buffers. Map pts/time via spa_meta_header.

Simple API (feature = "pipewire")
pub struct PipeWirePublisher { /* ... */ }
impl PipeWirePublisher {
  pub fn new(name: &str, w: u32, h: u32, fps: (u32,u32)) -> anyhow::Result<Self>;
  pub fn start_with_output(&mut self, out: OutputId) -> anyhow::Result<()>;
  pub fn stop(self) -> anyhow::Result<()>;
}

Binary/CLI
New commands:
- sc screen stream --pipewire [--output NAME] [--window TITLE|APP_ID] [--fps 30] [--size 1920x1080]
- sc screen screenshot --out file.png [--output NAME] [--region x,y,w,h] [--frame N]
- sc screen block-window --window-id TLID --on/--off
- sc screen set-target --output NAME | --window TLID

Rust crates to use
- wayland-server, smithay (already in project)
- pipewire (+ spa) Rust bindings (optional)
- image (PNG write) for screenshots
- thiserror, anyhow, tracing for errors/logging

Key coding details
Screencopy server (Smithay)
- When client requests a frame: If dmabuf export supported by GPU path, advertise and fill planes. Else allocate shm pool, copy pixels from last composed FBO (use PBO if present) into SHM. Cursor inclusion optional (follow request flag). Ensure per-output capture and region capture both work.

"Sensitive windows"
- During the "cast composition," skip surfaces with blocked == true. Expose a runtime toggle (CLI + config). Add a visual indicator on blocked windows (e.g., hashed overlay) when a screencast session is active.

Dynamic cast target
- Store a current CastTarget; on change, rewire the screencopy source and PipeWire stream without tearing down the whole session when possible. If a selected window disappears, emit an event and fall back to "no source" until retargeted.

Headless deterministic screenshots
- Provide a surfaceless EGL or software path. For --frame N, advance a fixed simulation clock and render deterministically, then dump PNG.

Security & UX
- Portal path (default): rely on system xdg-desktop-portal for prompts & PipeWire session.
- Direct path: Hidden unless --automation is present or config enables. Show a red dot overlay while streaming or capturing. Log JSON audit lines to artifacts/agent/audit.jsonl.

Acceptance tests (must pass)
- Portal interop: On a desktop session with xdg-desktop-portal-(gnome|wlr) running: OBS / Firefox / Chromium should list ScreenComposer in "Share Screen/Window". Selecting a monitor or window captures via PipeWire; sharing works.
- Block sensitive window: Mark one window blocked; start a window cast on its parent workspace—blocked content must not appear in the stream.
- Dynamic target: Start a monitor cast, then sc screen set-target --window <TLID> and verify the portal client’s stream switches to that window without restarting the whole compositor.
- Headless CI: screencomposer --headless --virtual-output 1920x1080@60 --automation and sc screen screenshot --out frame.png --frame 200 produces a deterministic image that matches goldens.

Constraints
- Keep new code behind feature flags: features = ["screencopy", "pipewire", "screenshare", "headless"]. If dmabuf export is unavailable, automatically fall back to shm; never crash. Code must be well-commented and follow existing project style (rustfmt on).

Implementation notes for this repository
- Use src/screenshare/* for FrameTap, policy, screenshots, and optional PipeWire.
- Place the screencopy protocol alongside src/sc_layer_shell style (module with generated protocol code).
- Hook FrameTap in src/udev.rs after damage_tracker.render_output(...) and before queueing the frame.
- Add docs in docs/ to explain rendering flow and project structure.
- If an API detail is missing, use common Smithay/PipeWire patterns and keep interfaces simple and documented.
<!-- Main screensharing plan moved here from new_feature_screenshare.md -->
Goal: Implement portal-first screen sharing (PipeWire) and UX parity with Niri: screencast monitors/windows via xdg-desktop-portal, support block sensitive windows, and dynamic cast target. Provide deterministic headless screenshots for CI.

# High-level plan (follow exactly)

## Portal path (default for users)

Make ScreenComposer compatible with xdg-desktop-portal screencasting:

Ensure we support zwlr_screencopy_v1 and linux-dmabuf (zero-copy when available).

Ensure we expose window/monitor metadata sufficient for window and monitor selection.

Add hooks to exclude specific surfaces from screencast (“sensitive windows”).

Add a control to switch the active cast target at runtime (“dynamic cast target”).

We do not implement our own portal backend now; we interoperate with existing backends (like xdg-desktop-portal-gnome or wlr).

## Deterministic path (CI)

Add a headless output and a single-frame screenshot CLI for goldens:

sc screen screenshot --out <png> [--output <name>] [--region x,y,w,h] [--frame N]

This bypasses portals to keep CI reproducible.

## Security

Portal path: inherit portal prompts/indicators.

Direct screencopy/screenshot path disabled by default; only enabled with --automation flag or config key. Add an on-screen “recording” dot overlay while active.

Deliverables / repo layout (single-crate)

Create or modify these files under the existing `src/` tree. No workspace/crates split is required.

src/screenshare/frame_tap.rs
A FrameTap that receives every composed frame (after render, before swap):

```
pub struct FrameMeta { pub size: (u32,u32), pub stride: u32, pub fourcc: u32, pub time_ns: u64 }
pub trait FrameTap: Send + Sync {
    fn on_frame_rgba(&self, out: OutputId, buf: &MappedImage<'_>, meta: &FrameMeta);
    fn on_frame_dmabuf(&self, out: OutputId, dmabuf: &DmabufHandle<'_>, meta: &FrameMeta);
}
```

Integrate this into the compositor’s render loop and virtual outputs (in `src/udev.rs` initially). Keep it behind the `screenshare` feature flag.

src/screencopy/mod.rs (and `protocol.rs`)
Implement zwlr_screencopy_v1 minimal server side:

capture_output, capture_output_region, frame acquire/commit lifecycle.

Prefer dmabuf export; fallback to shm using a readback copy.

Respect damage; throttle to chosen fps for screencast sessions.

Note: if Smithay provides a helper for screencopy at the pinned revision, prefer that. Otherwise, vendor the XML under `src/screencopy/protocols/wlr-screencopy-unstable-v1.xml` and generate code with wayland-scanner (mirroring `src/sc_layer_shell/`).

src/screenshare/policy.rs
“Sensitive windows” policy + dynamic cast target:

Maintain an allow/deny flag per toplevel (by app_id or surface_id).

Public API:
```
    pub fn set_surface_screencast_blocked(surface: SurfaceId, blocked: bool);
pub fn set_cast_target(target: CastTarget); // enum { Output(OutputId), Window(ToplevelId) }
```

Screencopy/stream path must skip blocked surfaces when compositing to the screencast buffer.

src/screenshare/pipewire.rs (optional)
A PipeWirePublisher that can publish a video node from the FrameTap stream:

Create pw::MainLoop, Context, Core, Stream.

Negotiate caps: video/x-raw, format RGBA or BGRx, width/height, fps.

Use dmabuf planes when available; otherwise copy RGBA into SPA buffers.

Map pts/time via spa_meta_header.

## Simple API (feature = "pipewire"):
```
pub struct PipeWirePublisher { /* ... */ }
impl PipeWirePublisher {
    pub fn new(name: &str, w: u32, h: u32, fps: (u32,u32)) -> anyhow::Result<Self>;
    pub fn start_with_output(&mut self, out: OutputId) -> anyhow::Result<()>;
    pub fn stop(self) -> anyhow::Result<()>;
}
```
src/bin/sc.rs (recommended) or extend current binary with subcommands
## New commands:

sc screen stream --pipewire [--output NAME] [--window TITLE|APP_ID] [--fps 30] [--size 1920x1080]

sc screen screenshot --out file.png [--output NAME] [--region x,y,w,h] [--frame N]

sc screen block-window --window-id TLID --on/--off

sc screen set-target --output NAME | --window TLID

docs/screensharing.md
User doc: how to share screen via portals, how to block a window, how to switch cast target.

## Rust crates to use

wayland-server, smithay (already in project)

pipewire (+ spa) Rust bindings (optional)

image (PNG write) for screenshots

thiserror, anyhow, tracing for errors/logging

# Key coding details

## Screencopy server (Smithay)

When client requests a frame:

If dmabuf export supported by GPU path, advertise and fill planes.

Else allocate shm pool, copy pixels from last composed FBO (use PBO if present) into SHM.

Cursor inclusion optional (follow request flag).

Ensure per-output capture and region capture both work.

## “Sensitive windows”

During the “cast composition,” skip surfaces with blocked == true.

Expose a runtime toggle (CLI + config).

Add a visual indicator on blocked windows (e.g., hashed overlay) when a screencast session is active.

## Dynamic cast target

Store a current CastTarget; on change, rewire the screencopy source and PipeWire stream without tearing down the whole session when possible.

If a selected window disappears, emit an event and fall back to “no source” until retargeted.

## Headless deterministic screenshots

Provide a surfaceless EGL or software path.

For --frame N, advance a fixed simulation clock and render deterministically, then dump PNG.

## Security & UX

Portal path (default): rely on system xdg-desktop-portal for prompts & PipeWire session.

Direct path:

Hidden unless --automation is present or config enables.

Show a red dot overlay while streaming or capturing.

Log JSON audit lines to artifacts/agent/audit.jsonl.

# Example snippets to generate

PipeWire connect + stream: create pw::stream::Stream, set format, implement process callback, copy/attach the latest FrameTap buffer, set PTS, queue.

Screencopy handlers: implement frame acquire, copy/export into client buffer (dmabuf/shm), send ready/damage events correctly.

Blocked-window mask: during offscreen “screencast composition,” skip surfaces flagged blocked; if needed, draw a checkerboard over them.

# Acceptance tests (must pass)

Portal interop: On a desktop session with xdg-desktop-portal-(gnome|wlr) running:

OBS / Firefox / Chromium should list ScreenComposer in “Share Screen/Window”.

Selecting a monitor or window captures via PipeWire; sharing works.

Block sensitive window: Mark one window blocked; start a window cast on its parent workspace—blocked content must not appear in the stream.

Dynamic target: Start a monitor cast, then sc screen set-target --window <TLID> and verify the portal client’s stream switches to that window without restarting the whole compositor.

Headless CI: screencomposer --headless --virtual-output 1920x1080@60 --automation and
sc screen screenshot --out frame.png --frame 200 produces a deterministic image that matches goldens.

# Constraints

Keep new code behind feature flags:

features = ["screencopy", "pipewire", "screenshare", "headless"]

If dmabuf export is unavailable, automatically fall back to shm; never crash.

Code must be well-commented and follow existing project style (Rustfmt on).

Implementation notes for this repository:

- Use `src/screenshare/*` for FrameTap, policy, screenshots, and optional PipeWire.
- Place the screencopy protocol alongside `src/sc_layer_shell` style (module with generated protocol code).
- Hook FrameTap in `src/udev.rs` after `damage_tracker.render_output(...)` and before queueing the frame.
- Add docs in `docs/` to explain rendering flow and project structure.
- If an API detail is missing, use common Smithay/PipeWire patterns and keep interfaces simple and documented.