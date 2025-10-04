# Render Loop Architecture

ScreenComposer keeps each backend’s render loop intentionally quiet until there is real work to do. This document collects the key pieces that govern when the compositor wakes up, when it performs a render, and how clients are signalled afterwards.

## Shared Building Blocks

- **Calloop dispatch** – `ScreenComposer` owns a `calloop::LoopHandle`. Whenever surface state changes (e.g. via `CompositorHandler::commit`) we queue an idle source so that the next event-loop iteration is guaranteed to run, even if the backend didn’t schedule a timer itself.
- **Frame bookkeeping** – After a successful render, we call `post_repaint`/`take_presentation_feedback` to issue `wl_callback` frame-done events and `wp_presentation` feedback. Clients driven by either mechanism receive precise timestamps and can schedule their next buffers.
- **Damage tracking** – Each backend feeds `render_output` with a damage tracker. We short-circuit the heavy drawing path when the scene has no new damage, when no redraw was requested, and when auxiliary surfaces (cursor, DnD icon) don’t need to be repainted.
- **Drag-and-drop and cursor surfaces** – Surface-backed cursors or DnD icons bypass the “no damage” fast path so interactive UI stays responsive even when the main scene is static.

## Winit Backend (`src/winit.rs`)

1. **Event intake** – `winit.dispatch_new_events` pumps windowing/input events and updates workspace geometry on resizes. Gestures and pointer moves flow through the shared `ScreenComposer` state.
2. **Scene update pass** – We call `state.scene_element.update()` once per loop to refresh layered UI animations. The return value (`scene_has_damage`) is part of the render decision.
3. **Render decision** – We render iff any of the following is true:
	- The scene reported damage.
	- A forced redraw is pending (`full_redraw > 0`).
	- The pointer is backed by a Wayland surface.
	- There is an active drag-and-drop icon.
4. **Submitting frames** – When `render_output` produces damage, we submit via the winit window’s swapchain. Success toggles `needs_redraw_soon`, which shortens the next wait timeout to 1 ms to catch follow-up work quickly.
5. **Idle waiting** – After each loop iteration we call `event_loop.dispatch(wait_timeout, &mut state)` where the timeout is 1 ms when `needs_redraw_soon` is true, otherwise ~16 ms (roughly 60 Hz). This keeps CPU usage low while still reacting quickly to active rendering.
6. **Housekeeping** – On every successful dispatch we refresh workspace layouts, clean up popups, and flush Wayland clients.

### Latency considerations
- The buffer age queried from the backend lets us use partial damage when available. If a full redraw was requested (`full_redraw > 0`), we reset the buffer-age path to avoid stale contents.
- Presentation feedback timestamps are derived from the shared `Clock<Monotonic>` so they line up with input event timestamps.

## X11 Backend (`src/x11.rs`)

1. **Backend events** – A Smithay-provided X11 source pushes resize, present-complete, refresh, and input events into the calloop. Resizes rebuild the `Output` mode, reflow workspaces, and mark `render = true`.
2. **Render gating** – The main loop checks `state.backend_data.render`. If no work is pending we skip directly to the calloop dispatch with a 16 ms timeout.
3. **Rendering** – When rendering, we bind the GBM buffer, gather render elements (scene, cursor, optional FPS overlay), and call `render_output`. Successful submits toggle `render` depending on whether the swapchain accepted the frame.
4. **Feedback and cleanup** – Similar to the winit path, we push frame callbacks, presentation feedback, and manage RenderDoc captures when the debug feature is active.
5. **Loop dispatch** – We always dispatch calloop with a 16 ms timeout, relying on the compositor’s idle sources to wake sooner if needed.

## Udev (DRM/GBM) Backend (`src/udev.rs`)

1. **Device events** – The DRM backend registers session pause/resume hooks and per-output frame timers. On VT resume we reset buffer state and schedule an idle render to guarantee the outputs redraw.
2. **Render scheduling** – After a buffer submit, `schedule_render` decides whether to queue the next repaint. Successful submits compute a repaint delay aimed at sharing the refresh interval between client rendering and compositor rendering. For now we schedule an immediate timer (the infrastructure supports delayed timers for finer latency tuning).
3. **Repaint timers** – When the timer fires we call `render(node, Some(crtc))`, which re-renders either a specific CRTC or all if none is specified.
4. **Presentation integration** – Using metadata from the DRM page-flip event, we fill `wp_presentation` feedback with hardware clock bits when available. Temporary DRM errors either pause scheduling (device inactive) or trigger retries depending on the error class.

### Early imports
- `ScreenComposer::backend_data.early_import(surface)` runs on every surface commit before rendering. Backends can use this hook to import client buffers into GPU memory early, avoiding stalls during `render_output`.

## Event Loop Wakeups on Surface Commit

- The compositor’s `commit` handler now calls `ScreenComposer::schedule_event_loop_dispatch()`. This schedules a zero-op idle task on the calloop, ensuring that any backend waiting with a long timeout will still wake promptly to process new buffers. This is especially useful for DRM where presentation-driven timers might otherwise delay reacting to surface updates.

## Instrumentation and Profiling

- **Tracing (`RUST_LOG`)** – Use `RUST_LOG=trace` or similar to capture backend-specific diagnostic logs, including timer scheduling and swapchain outcomes.
- **Puffin profiling** – When the `profile-with-puffin` feature is active, each loop iteration registers a new Puffin frame so you can inspect render timings in a Puffin viewer.
- **RenderDoc integration** – With the `debug` feature on, the compositor brackets each submitted frame with RenderDoc capture start/end calls, discarding frames with no damage to keep captures focused.
- **Perf counters** – The `perf-counters` feature enables additional frame statistics logging without impacting release builds.
