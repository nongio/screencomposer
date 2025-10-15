# Window Move Handling

This note outlines how ScreenComposer processes `xdg_toplevel.move` requests and updates window positions during drag operations.

- **Request entry point** — `ScreenComposer::move_request_xdg` (`src/shell/xdg.rs:658`) is invoked when a client issues `xdg_toplevel.move`. The compositor ensures the request originates from the grab that triggered the move (pointer or touch) and that the target surface is still mapped. Maximized surfaces are unmaximized prior to the drag, with their initial location reset to the pointer/touch position so the new window geometry tracks the cursor naturally.
- **Pointer-driven drags** — If the move was initiated via pointer, a `PointerMoveSurfaceGrab` (`src/shell/grabs.rs:37`) is installed on the seat’s pointer. While the grab is active, pointer focus is cleared from clients; cursor motions are fed into `PointerMoveSurfaceGrab::motion`, which computes the delta from the grab start and calls `workspaces.map_window` to reposition the window. Associated view layers are updated to keep compositor-side UI in sync.
- **Touch-driven drags** — For touch events the compositor assigns a `TouchMoveSurfaceGrab` (`src/shell/grabs.rs:198`). Touch move events adjust the window location via the same `workspaces.map_window` helper, using the touch slot that initiated the grab to maintain continuity until the finger lifts.

Both grab implementations release automatically when the initiating button is released or touch slot ends, restoring normal pointer/touch focus and completing the move.
