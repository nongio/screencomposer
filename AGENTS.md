## What ScreenComposer is
- A Wayland compositor built with Smithay.
- Runs with one of the supported backends: udev (DRM/GBM), winit, or X11.
- Provides a Wayland socket for clients to connect and standard protocols for windowing, input, and output management.

## Running the compositor
- Select a backend via one of: `--tty-udev`, `--winit`, or `--x11`.
- On startup, a Wayland socket is created automatically. The socket name is logged; set `WAYLAND_DISPLAY` to that value for child processes that act as clients.
- Logging: controlled by `RUST_LOG` (tracing + env-filter). Example: `RUST_LOG=info`.
- Optional profiling (built by default): Puffin HTTP server starts on the default port when the profiling feature is active; connect with a Puffin viewer to inspect frame timelines.

## Protocols available to clients
See also: [docs/wayland.md](./docs/wayland.md) for a complete list and code references.
ScreenComposer exposes a broad set of Wayland globals via Smithay. Key ones agents typically use:
- Core: `wl_compositor`, `wl_shm`, `wl_seat`, `wl_data_device_manager`
- Shells and windowing: `xdg_wm_base` (XDG shell), `wlr_layer_shell_v1` (layer shell)
- Output and presentation: `wl_output`, `xdg_output`, `wp_presentation`
- Rendering helpers: `zwp_linux_dmabuf_v1`, `wp_viewporter`
- Input and UX: pointer gestures, relative pointer, keyboard shortcuts inhibit, text input manager, input method manager
- Selection/clipboard: primary selection, data control (wlr-data-control)
- XDG foreign: to identify and interact with toplevels across clients

Note: Protocol availability can depend on build features and backend capabilities.

## Typical agent patterns
- Launch the compositor, parse logs to obtain the Wayland socket name, and set `WAYLAND_DISPLAY` for subsequent client processes.
- Drive scenarios by launching regular Wayland clients that use standard protocols (XDG shell for windows, layer shell for panels/overlays).
- Use frame callbacks (`wl_surface.frame`) to synchronize on rendering, and `wp_presentation` feedback to correlate frames and timestamps.
- Observe output changes through `wl_output`/`xdg_output` events to know sizes, transforms, and scales.
- Manage clipboard via `wl_data_device_manager` and data-control/primary-selection as needed.

## Input and focus
- Input comes from the active backend (libinput via udev, or the windowing backend in winit/X11 mode).
- Programmatic input injection is not provided at this time; agents should prefer driving behavior through client-side protocol requests (e.g., resizing via XDG requests) instead of simulating hardware input.

## Keyboard shortcuts
- `Alt+W`: Close every window owned by the focused application.
- `Alt+Shift+W`: Close only the currently focused window.

## Rendering behavior (for synchronization)
- Composition uses damage tracking; only damaged regions are repainted.
- Output presentation events (`wp_presentation`) allow agents to detect when a frame was displayed and retrieve precise timestamps.
- To avoid race conditions, wait for either a `wl_callback` frame done or a presentation feedback event after requesting a visible change.

### Skia-based layered rendering
- Rendering is performed via a Skia wrapper (`SkiaRenderer`) on top of Smithay’s GLES renderer.
- The scene is composed using a layers engine (see `src/render_elements/*` and `workspaces`), where windows and UI elements are layered and ordered explicitly.
- Overlays (e.g., status indicators) are implemented as layers; agents should account for their z-order when validating pixels.
- Fractional scaling and transforms can affect raster output; prefer semantic checks (e.g., sizes/positions from Wayland events) and presentation feedback over strict pixel equality.

## Observability
- Tracing logs (stdout/stderr) with `RUST_LOG` control help agents detect lifecycle milestones (socket name, backend selection, output add/remove).
- Optional Puffin profiling server can be enabled at build time; connect a viewer to inspect frame timings and renderer scopes.

## Testing hooks
- The repository contains WLCS-related sources under `wlcs/` and `wlcs_screencomposer/` for interoperability testing. Agents integrating WLCS can reuse these components to validate protocol behavior and compositor conformance.

## Limitations (current)
- No built-in programmatic input injection.
- No built-in screen capture API.
- Headless mode is not provided; run with one of the existing backends.

## Tips for reliable automation
- Ensure the compositor is fully initialized (socket created, at least one output announced) before launching clients; parse logs or wait for `wl_registry` globals.
- Use deterministic client assets (fonts, images) to reduce rendering variance across environments.
- Prefer protocol-driven layout (XDG configure/ack) over timing-based sleeps.
