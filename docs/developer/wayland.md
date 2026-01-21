## Wayland protocols

Otto follows Smithay’s “one big compositor state” architecture: most Wayland protocol state and handlers hang off a single struct, `Otto<BackendData>`. If you’re new to the codebase, learning where that struct is initialized and how Smithay routes protocol requests into it will make everything else easier to navigate.

### The big state variable: `Otto<BackendData>`

`Otto<BackendData>` lives in `src/state/mod.rs` and contains:

- High-level compositor state (workspaces, popups, input state, the scene graph, etc.)
- Smithay protocol state objects (e.g. `CompositorState`, `XdgShellState`, `WlrLayerShellState`, `PresentationState`, `ShmState`, …)
- Backend-specific data (`BackendData`) for rendering + outputs

When you see code like `self.xdg_shell_state` or `self.shm_state`, that is one of these Smithay protocol state objects stored directly inside `Otto`.

### Where it gets initialized

Most Wayland globals/state objects are created in `Otto::init(...)` in `src/state/mod.rs`:

- The Wayland socket/client dispatch source is installed into calloop.
- Smithay protocol states are constructed (e.g. `CompositorState::new`, `XdgShellState::new`, `PresentationState::new`, etc.).
- Capability-gated globals are conditionally created based on backend capabilities.

Backends may also create backend-specific globals in their entrypoints (for example `zwp_linux_dmabuf_v1` is created per backend).

### How Smithay “delegation” works here

Most Smithay protocols are wired via `delegate_*` macros. The pattern is:

1. Otto stores a Smithay protocol state object (e.g. `xdg_shell_state: XdgShellState`).
2. Otto implements the corresponding Smithay `*Handler` trait (e.g. `XdgShellHandler for Otto<BackendData>`).
3. A `delegate_*` macro is invoked for `Otto<BackendData>` (often in `src/state/mod.rs`, sometimes in a dedicated handler module under `src/state/`).

The `delegate_*` macro generates the dispatch glue so incoming Wayland requests for that global get forwarded into your `*Handler` trait impl.

### “I’m looking for protocol X”: a practical search recipe

When you need to find where a protocol is implemented, this workflow is usually fastest:

1. Search for the delegate macro:
   - Example patterns: `delegate_xdg_shell!`, `delegate_layer_shell!`, `delegate_presentation!`, `delegate_data_device!`, …
2. Find the handler trait impl for `Otto<...>`:
   - Example patterns: `impl<BackendData: Backend> XdgShellHandler for Otto<BackendData>`
3. Find where the protocol state is constructed:
   - Usually in `Otto::init(...)` in `src/state/mod.rs`.
   - Backend-specific globals (notably dmabuf) are in `src/udev.rs`, `src/winit.rs`, `src/x11.rs`.

In Otto specifically, protocol *handlers* are split roughly like this:

- `src/state/*.rs`: “core” protocol handlers and delegate glue (seat, selection, input-method, fractional-scale, foreign toplevel, etc.)
- `src/shell/*.rs`: XDG shell, layer-shell, and surface commit plumbing
- `src/{udev,winit,x11}.rs`: backend-specific globals and backend-specific handler impls

### Common protocol entrypoints (where to start looking)

This is a non-exhaustive map of common protocols to the place you’ll usually land first:

- `wl_compositor` / surface commits
  - Handler: `CompositorHandler for Otto<BackendData>` in `src/shell/mod.rs`
  - State creation + delegation: `src/state/mod.rs` (`CompositorState::new`, `delegate_compositor!`)

- `xdg_wm_base` (XDG shell)
  - Handler: `XdgShellHandler for Otto<BackendData>` in `src/shell/xdg.rs`
  - Delegation: `src/state/mod.rs` (`delegate_xdg_shell!`)

- `wlr_layer_shell_v1`
  - Handler: `WlrLayerShellHandler for Otto<BackendData>` in `src/shell/mod.rs`
  - Delegation: `src/state/mod.rs` (`delegate_layer_shell!`)
  - Otto custom protocol lives separately under `src/sc_layer_shell/`.

- `wl_seat` (input)
  - Handler/delegation: `src/state/seat_handler.rs` + `delegate_seat!`
  - Seat creation and wiring happen during initialization in `src/state/mod.rs`.

- Clipboard/selection
  - `wl_data_device_manager`: `src/state/data_device_handler.rs` (+ `delegate_data_device!`)
  - primary selection + data control: `src/state/selection_handler.rs` (+ `delegate_primary_selection!`, `delegate_data_control!`)

- Presentation timing
  - State creation + delegation: `src/state/mod.rs` (`PresentationState::new`, `delegate_presentation!`)
  - Presentation feedback is emitted after rendering (see `post_repaint` / `take_presentation_feedback` in `src/state/mod.rs`, and backend render loops).

- `zwp_linux_dmabuf_v1`
  - Implemented per backend: see `impl DmabufHandler for Otto<...>` in `src/udev.rs`, `src/winit.rs`, and `src/x11.rs`.
