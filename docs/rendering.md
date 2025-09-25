# Rendering pipeline overview

This document explains at a high level how ScreenComposer renders frames and where integration points live.

- Backend: udev/DRM+GBM+EGL (primary), plus optional winit/x11 paths.
- Renderer: Smithay GlesRenderer wrapped by Skia (`src/skia_renderer.rs`) to draw, import dmabufs, and manage textures/surfaces; integrated with the `lay-rs` engine.
- Composition: `src/render.rs` produces `OutputRenderElements` for each output. The scene graph and layout are managed by the `lay-rs` engine (not a custom layered renderer).
- Damage tracking: `smithay::backend::renderer::damage::OutputDamageTracker` is used per output to render only damaged regions.
- DRM flow (udev): `src/udev.rs` manages connectors, CRTCs, planes, swapchains, and frame submission. It calls `render_output` with the current elements and queues the resulting buffer.

Integration points for screenshare:
- After `damage_tracker.render_output(...)` returns (in `udev.rs` Surface path), the composed frame is available before queuing. This is where a FrameTap can read/export pixels (dmabuf preferred, SHM fallback) for screencopy or streaming.
- Screencopy protocol can source frames from the same point, respecting damage and cursor flags.

Notes:
- Output transforms and fractional scaling are already handled in `render.rs` and element building.
- For deterministic screenshots, add a headless backend and fixed clock advancing to render a specific frame consistently.

## See also
- Project structure: [docs/project-structure.md](./project-structure.md)
- Configuration guide: [docs/configuration.md](./configuration.md)
- Screensharing overview: [docs/screensharing.md](./screensharing.md)
