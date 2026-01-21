## Project Structure

The project uses Rust and Cargo as its build system and package manager.
The codebase follows a modular architecture separating backends, rendering, shell handling, and UI components.

In the `src/` directory, there are the components for the main binary `otto` which implements the Wayland compositor.

Inside `components/`, there are standalone components like the `xdg-desktop-portal-otto` backend necessary for integration with the XDG Desktop Portal (for screensharing and screenshotting).

The `sample-clients/` directory contains example Wayland client applications useful for testing and development. Inside the `hello-design/` folder there is a work-in-progress Design Sytem leveraging wayland surfaces and the custom protocol offered by Otto.

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
├── sc_layer_shell/     # Custom layer shell protocol
└── screenshare/        # Screen sharing via PipeWire (see docs/screenshare.md)
components/             # Standalone components
└── xdg-desktop-portal-otto/  # XDG Desktop Portal backend
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
- **Screenshare**: PipeWire-based screen sharing with D-Bus API for portal integration

### Build & Run
Otto uses Cargo for building and managing dependencies.
To run Otto from within another graphical session (using Winit backend):
```sh
cargo run -- --winit   # Winit backend (development)
```

To run Otto as a standalone compositor (using DRM/GBM backend):
```sh
cargo run -- --tty-udev # Native DRM/GBM (production)
```

### Feature Flags

Otto uses Cargo features to enable/disable backends and developer tooling. The canonical list lives in the workspace `Cargo.toml` under `[features]`.

**Developer tooling**

- `debug`: enables debug-only functionality (includes RenderDoc integration).
- `debugger`: enables the lay-rs debugger hooks.
- `profile`: convenience feature that enables Puffin profiling (`profile-with-puffin` + `lay-rs/profile-with-puffin`).
- `profile-with-puffin`: enables Puffin profiling support.
- `profile-with-tracy`: enables Tracy profiling support.
- `profile-with-tracy-mem`: enables Tracy profiling support (memory).
- `perf-counters`: enables extra frame statistics logging.

**How to use features**

Enable extra features on top of defaults:

```sh
cargo run --features debugger -- --tty-udev
```

Build with a minimal set of features (useful for bisecting or reducing deps):

```sh
cargo run --no-default-features --features egl,winit -- --winit
```

Enable profiling (Puffin):

```sh
cargo run --features profile -- --winit
```