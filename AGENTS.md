# ScreenComposer

A Wayland compositor built with Smithay. Runs with udev (DRM/GBM), winit, or X11 backends.

## Project Structure

```
src/                    # Main crate source
├── main.rs             # Entry point and backend selection
├── lib.rs              # Library exports
├── config/             # Configuration parsing and runtime settings
├── state/              # Compositor state and Wayland protocol handlers
├── shell/              # Window management (XDG, layer shell, X11)
├── render_elements/    # Render element types for the damage tracker
├── workspaces/         # Workspace management and UI components
├── theme/              # Theming and styling
├── utils/              # Shared utilities
└── sc_layer_shell/     # Custom layer shell protocol

docs/                   # Design documentation
assets/                 # Static assets (icons, images)
resources/              # Runtime resources (cursors, etc.)
sample-clients/         # Example Wayland client applications
```

### Key Modules

- **Backends**: `udev.rs` (DRM/GBM), `winit.rs`, `x11.rs` — platform-specific display and input
- **Rendering**: `render.rs`, `skia_renderer.rs` — frame composition and Skia-based drawing
- **State**: Protocol handlers, seat management, data device, and global compositor state
- **Shell**: XDG toplevel/popup handling, layer shell, server-side decorations
- **Workspaces**: Multi-workspace support, window views, dock, app switcher

## Documentation

See the `docs/` folder for detailed documentation:

- `configuration.md` - Configuration options and settings
- `rendering.md` - Rendering pipeline details
- `render_loop.md` - Main render loop explanation
- `wayland.md` - Wayland protocol implementation
- `layer-shell.md` - Layer shell protocol support
- `expose.md` - Expose mode design and flow
- `dock-design.md` - Dock component design
- `window-move.md` - Window movement handling
- `keyboard_mapping.md` - Keyboard mapping configuration
- `screenshare.md` - Screen sharing support

## Build & Run

```sh
cargo build --release
./target/release/screen-composer --winit   # Winit backend (development)
./target/release/screen-composer --x11     # X11 backend
./target/release/screen-composer --tty-udev # Native DRM/GBM (production)
```

Check `Cargo.toml` for available feature flags.

## Observability

- Use `RUST_LOG` environment variable to control tracing output
- Optional Puffin profiling server available at build time
