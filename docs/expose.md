# Expose mode

## What it is
Expose shows scaled previews of every visible window on the current workspace so they can be clicked or dragged. It is triggered by `Workspaces::expose_show_all` (keyboard toggle or gesture) which drives both the layout calculation and the transition animation.

## Lifecycle
- Enter/exit: `expose_show_all_workspace` computes gesture state, then calls `expose_show_all_layout` to build/update the layout bin and `expose_show_all_animate` to drive the animation and visibility.
- Updates: `expose_update_if_needed` recalculates when windows change (map/unmap/move/drag/drop) but only when expose is visible.
- Visibility: The expose layer and overlay layers are kept hidden unless animation or `show_all` is active to avoid unnecessary drawing.

## Window mirroring
- Each window is mirrored by a layer created in `WorkspaceView::map_window` (`window_selector_view.map_window` adds it to the expose container). The mirror follows the real window layer via `add_follower_node`, so content stays in sync.
- Mirrors are excluded from expose while a drag is in progress (`expose_dragging_window`) to avoid double-rendering the dragged item.
- When a window is minimized, it is excluded from the expose, its mirror is hidden (`minimize_window`), and restored on unminimize.

## Layout: natural flow
- `expose_show_all_layout` builds an input list of windows (skipping minimized and currently dragged windows) with their real geometry and title.
- `WindowSelectorView::update_windows` calls `natural_layout` (in `utils::natural_layout`) to pack the windows into the target rectangle (`LayoutRect`) using a flowing grid algorithm:
  - Windows keep aspect ratios; scaling is limited to 1.0 so previews never exceed real size.
  - Packing is deterministic: windows are sorted by protocol id before hashing, and a layout hash is cached to skip no-op recalculations.
  - Results are stored in `expose_bin` and mirrored into `WindowSelectorState.rects`, which drives both drawing and hit-testing.

## Animation and positioning
- `expose_show_all_animate` interpolates window layers from their on-screen bbox to the target rects in `expose_bin`, applying translation + scale; easing is Spring-based when `end_gesture` is true.
- The workspace selector, dock, and overlay opacity/positions are animated in tandem to slide the UI into place. When expose is open, the dock is hidden unless fullscreen requires otherwise.

## Drag and drop in expose
- Drag activation happens in `WindowSelectorView::try_activate_drag` after a small threshold; mirrors are moved to the drag overlay while keeping anchor/scale consistent.
- Drop targets come from the workspace selector previews; intersection with a drop layer sets `current_drop_target`.
- On drop:
  - If a target workspace is selected, `move_window_to_workspace` is called with the window’s last known position; expose is refreshed to rebuild layout.
  - If no target, the dragged mirror is restored to its original parent and ordering (`restore_layer_order_from_state`), and expose is refreshed to realign.
- Logging: drop events log the window id and target workspace to help debugging.

## Common entry points
- Toggle expose: `expose_show_all(delta, end_gesture)`
- Force a relayout while in expose: `expose_update_if_needed` / `expose_update_if_needed_workspace`
- Show desktop (push windows away): `expose_show_desktop`

## Tips for agents
- Wait for expose to finish initializing (`show_all` true and `expose_bin` populated) before asserting layout.
- Use semantic data (rects from `WindowSelectorState` or `expose_bin`) rather than pixel checks; fractional scaling can shift raster output.
- During drags, the dragged window is intentionally absent from the grid; expect a temporary gap until drop completes or is cancelled.
