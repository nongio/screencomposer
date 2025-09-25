# Project structure

Top-level (selected):
- `src/` — main crate source
  - `config.rs` — runtime configuration handling
  - `render.rs` — build output render elements and call damage tracker
  - `skia_renderer.rs` — Skia wrapper over Smithay GLES renderer
  - `udev.rs` — DRM/GBM backend (primary production path)
  - `winit.rs`, `x11.rs` — alternative backends
  - `render_elements/` — element types used by the renderer
  - `shell/` — XDG/Layer shell handlers and window management glue
  - `state/` — Wayland globals, seats, data device, and compositor state
  - `sc_layer_shell/` — custom protocol example integration
  - `screenshare/` — (new, feature-gated) screenshare modules: frame_tap, policy, screenshot, optional pipewire
  - `protocols/` — (planned) screencopy module, generated protocol code, and XML
- `docs/` — documentation for configuration, design, and features
- `assets/`, `resources/` — images and UI resources

Features:
- `winit`, `x11`, `udev`, `egl` — backends/capabilities
- `screenshare` — enables screenshare modules (frame_tap/policy/screenshot)
- `screencopy` — enables zwlr_screencopy_v1 server (to be added)
- `pipewire` — enables PipeWire publisher (optional)
- `headless` — planned headless backend for CI screenshots

Build & run:
- Main binary: `screen-composer --winit|--x11|--tty-udev`
- Future: `sc` helper binary or subcommands for screenshots/streaming and policy toggles
