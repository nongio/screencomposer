## Rendering pipeline overview

This document explains (at a high level) how Otto renders frames, starting from the underlying architecture and ending with the practical “where do I hook in?” points.

If you want to follow the code while reading:
- Element building / composition: `src/render.rs`
- Skia wrapper over Smithay GL renderer: `src/skia_renderer.rs`
- DRM/udev backend and frame submission: `src/udev.rs`
- winit backend (dev path): `src/winit.rs`

If you are new to this codebase, a good reading order is:
1. Skim “Rendering architecture” for the mental model.
2. Read “Frame flow” to understand the per-frame execution.
3. Jump to “Screenshare integration points” if you are working on capture/streaming.

### Rendering architecture (layered mental model)

Think of Otto rendering as a few layers stacked on top of each other:

1. **Smithay provides the backend plumbing**

   - Smithay owns the low-level rendering machinery (a `GlesRenderer`) and the output infrastructure.
   - Conceptually, Smithay renders “into a buffer” that will later be presented on screen.

2. **The backend decides what “buffer” means**

   - On **winit**, the “output” is a window inside another compositor; the buffer is presented to that host window.
   - On **udev/DRM**, the “output” is a real connector/CRTC/plane pipeline; buffers are submitted to KMS.

3. **Otto wraps Smithay GL with Skia**

   - Otto’s renderer uses Smithay’s `GlesRenderer` internally.
   - It wraps the current EGL framebuffer (from the GL context) into a Skia surface.
   - Otto then uses a Skia canvas to draw.

4. **Otto draws mostly via a retained scene graph (`lay-rs`)**

   - The UI/scene is built as a retained scene graph managed by `lay-rs`.
   - In rendering terms, one of the output render elements is a `SceneElement`, which encapsulates “all our graphics”.

### Key pieces (what lives where)

- **Backend**: udev/DRM+GBM+EGL (primary), plus optional winit/x11 paths.
- **Buffers & presentation**: handled by Smithay + the backend (winit presents into a host window; udev submits to DRM/KMS).
- **Renderer**: Smithay `GlesRenderer` wrapped by Skia (`src/skia_renderer.rs`). This draws, imports dmabufs, and manages textures/surfaces.
- **Elements**: `src/render.rs` produces `OutputRenderElements` per output; it includes a `SceneElement` (from `src/render_elements/scene_element.rs`) that renders the `lay-rs` scene.
- **Damage tracking**: `smithay::backend::renderer::damage::OutputDamageTracker` is used per output so only damaged regions are rendered.

### Backends in practice (winit vs udev)

- **winit backend**

  - Best for development.
  - Output is a regular window; there is no hardware cursor plane.
  - The cursor is rendered as part of normal composition.
  - Does not offer all the functionalities of the udev/DRM path (no real outputs, no DMAbuffer, no screensharing).
  - no touch gestures support.

- **udev/DRM backend**

  - Production/bare-metal path.
  - Smithay/DRM manages connectors/CRTCs/planes, swapchains, and submission.
  - The cursor is handled through Smithay so it can be optimized (including being placed on a dedicated DRM plane when available).

### Frame flow (common path)

At a high level, rendering a frame looks like:

1. Build the list of renderable elements for each output (`OutputRenderElements` in `src/render.rs`).
   - This includes the `SceneElement` which renders the `lay-rs` scene graph.
2. Hand those elements to Smithay’s `OutputDamageTracker`.
3. Smithay drives the render pass for damaged regions.
   - Otto’s `SkiaRenderer` uses Smithay’s GL renderer under the hood.
   - The current framebuffer is wrapped into a Skia surface, and Otto renders the scene (via `SceneElement`) into the Skia canvas.
4. The backend presents the resulting buffer.

### Screenshare integration points

**Screenshar is available only in udev backend**

After a successful normal render, the udev render loop blits a compositor Output (Screen) buffer into PipeWire-provided buffers.

   - This happens in `src/udev.rs` in `render_surface(...)`: when `outcome.rendered && !screenshare_sessions.is_empty()`, Otto dequeues an available PipeWire buffer and calls `crate::screenshare::fullscreen_to_dmabuf(...)`, then triggers `PipeWireStream` to queue the buffer.
   - Damage is forwarded when possible; the first frame (or buffer changes) forces a full-frame blit.