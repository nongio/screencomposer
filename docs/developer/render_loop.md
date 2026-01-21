## Render Loop Architecture

Otto keeps each backend’s render loop intentionally quiet until there is real work to do. This document collects the key pieces that govern when the compositor wakes up, when it performs a render, and how clients are signalled afterwards.

### Latency (why this design)

The render loop is designed around a simple goal: keep *input → visible pixels* latency low without burning CPU/GPU when nothing changes.

- **Why “quiet until needed” helps**: if there is no scene damage, no cursor/DnD surface activity, and no forced redraw, rendering again would just re-draw identical pixels. Skipping those frames reduces power and CPU usage.
- **Why timing matters**: many clients only repaint after they receive a frame callback (`wl_callback`) or presentation feedback. If the compositor always repaints immediately after a VBlank and only then sends frame callbacks, clients may miss the *next* VBlank and you can end up with roughly “two-frame” latency (client paints for the following VBlank, then the compositor presents a frame later).
- **What we do on DRM (udev)**: after a page flip completes, we can schedule the next repaint with a small delay. This gives clients time to render a fresh buffer before the compositor repaints, so that new content can land on the next VBlank more often.
- **What we do on windowed backends (winit/x11)**: there is no real KMS VBlank to target. Instead we bias for responsiveness by using short dispatch timeouts (e.g. 1 ms) when we expect follow-up work, and a longer timeout when idle.

### Shared Building Blocks

**Calloop dispatch.** `Otto` owns a `calloop::LoopHandle`. Whenever surface state changes (e.g. a surface commit in `src/shell/mod.rs`), we call `Otto::schedule_event_loop_dispatch()`, which sends a message on a calloop channel source. This wakes the loop even if the backend is currently dispatching with a long timeout.

**Frame bookkeeping.** After a successful render, we call `post_repaint`/`take_presentation_feedback` to issue `wl_callback` frame-done events and `wp_presentation` feedback. Clients driven by either mechanism receive precise timestamps and can schedule their next buffers.

**Damage tracking.** Each backend feeds `render_output` with a damage tracker. We short-circuit the heavy drawing path when the scene has no new damage, when no redraw was requested, and when auxiliary surfaces (cursor, DnD icon) don’t need to be repainted.

**Drag-and-drop and cursor surfaces.** Surface-backed cursors or DnD icons bypass the “no damage” fast path so interactive UI stays responsive even when the main scene is static.

### Damage tracking (performance and pitfalls)

Damage tracking is the main reason Otto can stay responsive without re-rendering full frames all the time.

**Why it matters for performance.** Rendering a full output every frame is expensive: it burns CPU/GPU and power even when nothing changed. By tracking which regions are “dirty”, we can redraw only the parts that actually changed and keep idle outputs truly idle.

**Why it’s tricky.** Correct damage tracking is easy to get subtly wrong:

- Damage is expressed in output space, but surfaces can move, scale, and be transformed.
- Some things “change” without the main scene changing (cursor surfaces, DnD icons, popups), and they still need to trigger redraw.
- Buffer age and partial updates mean you must be careful about when you can reuse old pixels versus forcing a full redraw.

There is also an additional layer of complexity with **translucent UI layers** (for example the dock or the app switcher). These elements don’t just “cover” what’s behind them: they blend with it. That means changes in the underlying scene may require re-rendering the overlay region (because the blended result changes), and changes/animations in the overlay may require re-rendering parts of the background underneath. In Otto this is handled via **backdrop regions** coming from the `lay-rs` scene: when a translucent layer is present, the effective damage often needs to include both the element’s own bounds and the affected backdrop area behind it.

**How Otto does it.** Otto combines two layers of damage tracking:

- Smithay’s `OutputDamageTracker` is the outer mechanism that decides which parts of an output need repainting and orchestrates partial redraws.
- Otto’s retained scene graph (`lay-rs`) keeps track of what changed in the UI. The `SceneElement` (the render element that represents “all our graphics”) reports damage based on the scene’s dirty regions, so the renderer can update only what changed.

### Winit Backend (`src/winit.rs`)

1. **Event intake** – `winit.dispatch_new_events` pumps windowing/input events and updates workspace geometry on resizes. Gestures and pointer moves flow through the shared `Otto` state.
2. **Scene update pass** – We call `state.scene_element.update()` once per loop to refresh layered UI animations. The return value (`scene_has_damage`) is part of the render decision.
3. **Render decision** – We render iff any of the following is true:

   - The scene reported damage.
   - A forced redraw is pending (`full_redraw > 0`).
   - The pointer is backed by a Wayland surface.
   - There is an active drag-and-drop icon.
4. **Submitting frames** – When `render_output` produces damage, we submit via the winit window’s swapchain. Success toggles `needs_redraw_soon`, which shortens the next wait timeout to 1 ms to catch follow-up work quickly.
5. **Idle waiting** – After each loop iteration we call `event_loop.dispatch(wait_timeout, &mut state)`.

   - The timeout is 1 ms when we expect imminent follow-up work: `needs_redraw_soon`, active pointer surfaces / DnD (`pointer_active`), or the scene just reported damage.
   - Otherwise we wait ~16 ms (roughly 60 Hz).
6. **Housekeeping** – On every successful dispatch we refresh workspace layouts, clean up popups, and flush Wayland clients.

### Latency considerations
- The buffer age queried from the backend lets us use partial damage when available. If a full redraw was requested (`full_redraw > 0`), we reset the buffer-age path to avoid stale contents.
- Presentation feedback timestamps are derived from the shared `Clock<Monotonic>` so they line up with input event timestamps.

### X11 Backend (`src/x11.rs`)

1. **Backend events** – A Smithay-provided X11 source pushes resize, present-complete, refresh, and input events into the calloop. Resizes rebuild the `Output` mode, reflow workspaces, and mark `render = true`.
2. **Render gating** – The main loop checks `state.backend_data.render`. If no work is pending we skip directly to the calloop dispatch with a 16 ms timeout.
3. **Rendering** – When rendering, we bind the X11 surface buffer, gather render elements (currently the scene plus optional FPS overlay), and call `render_output`. Successful submits toggle `render` depending on whether submission succeeded.

   - Note: cursor surface rendering in the X11 backend is currently TODO (the code has placeholders but does not yet add cursor render elements).
4. **Feedback and cleanup** – Similar to the winit path, we push frame callbacks, presentation feedback, and manage RenderDoc captures when the debug feature is active.
5. **Loop dispatch** – We always dispatch calloop with a 16 ms timeout, relying on backend event sources (and the compositor wakeup channel) to wake sooner if needed.

### Udev (DRM/GBM) Backend (`src/udev.rs`)

1. **Device events** – The DRM backend registers session pause/resume hooks and per-output frame timers. On VT resume we reset buffer state and schedule an idle render to guarantee the outputs redraw.
2. **Render scheduling** – After a page flip completes (`frame_finish`), we decide whether to queue the next repaint timer. Successful submits compute a repaint delay aimed at sharing the refresh interval between client rendering and compositor rendering. For now we schedule an immediate timer (the infrastructure supports delayed timers for finer latency tuning).
3. **Repaint timers** – When the timer fires we call `render(node, Some(crtc))`, which re-renders either a specific CRTC or all if none is specified.
4. **Presentation integration** – Using metadata from the DRM page-flip event, we fill `wp_presentation` feedback with hardware clock bits when available. Temporary DRM errors either pause scheduling (device inactive) or trigger retries depending on the error class.

<!-- 
### Early imports
- `Otto::backend_data.early_import(surface)` runs on every surface commit before rendering. Backends can use this hook to import client buffers into GPU memory early, avoiding stalls during `render_output`.

### Event Loop Wakeups on Surface Commit

- The compositor’s surface commit handler calls `Otto::schedule_event_loop_dispatch()`. This sends on a calloop channel source, ensuring that any backend waiting with a long timeout still wakes promptly to process new buffers. This is especially useful for DRM where presentation-driven timers might otherwise delay reacting to surface updates.

### Instrumentation and Profiling

- **Tracing (`RUST_LOG`)** – Use `RUST_LOG=trace` or similar to capture backend-specific diagnostic logs, including timer scheduling and swapchain outcomes.
- **Puffin profiling** – When the `profile-with-puffin` feature is active, each loop iteration registers a new Puffin frame so you can inspect render timings in a Puffin viewer.
- **RenderDoc integration** – With the `debug` feature on, the compositor brackets each submitted frame with RenderDoc capture start/end calls, discarding frames with no damage to keep captures focused.
- **Perf counters** – The `perf-counters` feature enables additional frame statistics logging without impacting release builds. -->
