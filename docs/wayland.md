# Wayland globals and where they are implemented

This page lists the Wayland globals exposed by ScreenComposer and points to the code that wires them up.

Most globals are initialized in `ScreenComposer::init` within `src/state/mod.rs`, and many of the event handlers are connected via `delegate_*` macros.

## Core and compositor
- wl_compositor — `CompositorState::new::<Self>(&dh)`; `delegate_compositor!`
- wl_shm — `ShmState::new::<Self>(&dh)`; `delegate_shm!`
- wl_data_device_manager — `DataDeviceState::new::<Self>(&dh)`
- wl_seat — `SeatState::new()` then `new_wl_seat(&dh, seat_name)`; pointer/keyboard added there

## Shells and windowing
- xdg_wm_base (XDG shell) — `XdgShellState::new::<Self>(&dh)`; `delegate_xdg_shell!`
- wlr_layer_shell_v1 — `WlrLayerShellState::new::<Self>(&dh)`; `delegate_layer_shell!`
  - Custom helper module: `src/sc_layer_shell/`

## Output management and presentation
- wl_output / xdg-output — `OutputManagerState::new_with_xdg_output::<Self>(&dh)`; `delegate_output!`
- wp_presentation — `PresentationState::new::<Self>(&dh, clock.id() as u32)`; `delegate_presentation!`

## Rendering helpers
- zwp_linux_dmabuf_v1 — created per backend
  - udev: see `src/udev.rs` (uses `DmabufState`, `DmabufGlobal`)
  - winit: see `src/winit.rs`
  - x11: see `src/x11.rs`
- wp_viewporter — `ViewporterState::new::<Self>(&dh)`; `delegate_viewporter!`

## Input and UX extras
- wp_pointer_gestures — `PointerGesturesState::new::<Self>(&dh)`; `delegate_pointer_gestures!` (gated by backend capability)
- wp_relative_pointer — `RelativePointerManagerState::new::<Self>(&dh)`; `delegate_relative_pointer!` (gated by backend capability)
- wp_pointer_constraints — `PointerConstraintsState::new::<Self>(&dh)`
- wp_keyboard_shortcuts_inhibit — `KeyboardShortcutsInhibitState::new::<Self>(&dh)`; `delegate_keyboard_shortcuts_inhibit!`
- text-input — `TextInputManagerState::new::<Self>(&dh)`
- input-method — `InputMethodManagerState::new::<Self, _>(&dh, |_client| true)`
- zwp_virtual_keyboard_v1 — `VirtualKeyboardManagerState::new::<Self, _>(&dh, |_client| true)` (currently open to all clients)

## Selection/clipboard
- primary-selection — `PrimarySelectionState::new::<Self>(&dh)`
- wlr-data-control — `DataControlState::new::<Self, _>(&dh, Some(&primary_selection_state), |_| true)`

## XDG foreign
- xdg-foreign — `XdgForeignState::new::<Self>(&dh)`; `delegate_xdg_foreign!`

## Notes
- Availability may differ by backend and enabled features (`udev`, `winit`, `x11`, etc.).
- DMABUF globals are created in the active backend startup path and provide format/feedback to clients; see `state/mod.rs` for `select_dmabuf_feedback` usage.

## Keyboard shortcuts
- Shortcut bindings are configured via the `[keyboard_shortcuts]` table in `sc_config.toml`. Each key is a modifier combo like `Logo+Shift+Return`.
- Actions accept simple strings for built-in behaviors (`Quit`, `ScaleUp`, `RunLayersDebug`), inline tables for indexed variants (`{ builtin = "Screen", index = 0 }`), or command definitions (`{ run = { cmd = "layers_debug" } }`).
- Use `{ open_default = "browser" }` (or `terminal`, `file_manager`, custom MIME IDs) to launch the system default from XDG `mimeapps.list`, with optional fallbacks.
- Supply every binding you care about; the compositor no longer seeds default shortcuts, so an empty map disables them entirely.
- Optional `modifier_remap` and `[key_remap]` settings let you swap modifiers (e.g. map `logo` to `ctrl`) or translate individual keysyms (`"BackSpace" = "Delete"`) before shortcuts run, and remapped keycodes are also forwarded to clients.
