# Project Structure

## Overview

Otto is a Wayland compositor built with Smithay. The codebase follows a modular architecture separating backends, rendering, shell handling, and UI components.

## Directory Layout

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

## Key Modules

- **Backends**: `udev.rs` (DRM/GBM), `winit.rs`, `x11.rs` — platform-specific display and input
- **Rendering**: `render.rs`, `skia_renderer.rs` — frame composition and Skia-based drawing
- **State**: Protocol handlers, seat management, data device, and global compositor state
- **Shell**: XDG toplevel/popup handling, layer shell, server-side decorations
- **Workspaces**: Multi-workspace support, window views, dock, app switcher
- **Screenshare**: PipeWire-based screen sharing with D-Bus API for portal integration

## Build & Run

```sh
cargo build
./target/release/otto --winit   # Winit backend (development)
./target/release/otto --tty-udev # Native DRM/GBM (production)
```

## Feature Flags

Check `Cargo.toml` for available feature flags that enable optional functionality like different backends or experimental features.
