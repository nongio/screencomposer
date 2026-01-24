# WLR Layer Shell Implementation

This document describes the implementation of the `wlr_layer_shell_v1` protocol in Otto. It's intended as a reference for agents and developers working on layer-shell related features.

## Implementation Status

✅ **Implemented:**
- Global registration and protocol handler
- Surface lifecycle (create, configure, commit, destroy)
- Scene graph integration via lay_rs
- Rendering with buffer import
- Input routing (pointer and keyboard focus)
- Z-order stacking across all four layers
- Anchor and margin geometry computation
- Fullscreen/expose mode integration (overlay fade-out)
- Workspace scroll integration

⏳ **Pending:**
- Exclusive zone tracking and workspace geometry updates
- Per-output layer surface organization
- Output migration on disappearance

## Architecture Overview

### Key Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Otto                                │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │ layer_surfaces: HashMap<ObjectId, LayerShellSurface>            ││
│  └─────────────────────────────────────────────────────────────────┘│
│                              │                                       │
│                              ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │ Workspaces                                                       ││
│  │  ├── layer_shell_background: Layer (z=0)                        ││
│  │  │   └── [Background/Bottom layer surfaces]                     ││
│  │  ├── workspaces_layer: Layer (z=1)                              ││
│  │  │   └── [Workspace views with windows]                         ││
│  │  ├── layer_shell_overlay: Layer (z=2)                           ││
│  │  │   └── [Top/Overlay layer surfaces]                           ││
│  │  ├── dock: DockView                                              ││
│  │  ├── popup_overlay: Layer (z=highest)                           ││
│  │  └── expose_layer / workspace_selector                          ││
│  └─────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────┘
```

### Z-Order (Bottom to Top)

1. **layer_shell_background** - Client wallpapers, background layer surfaces
2. **workspaces_layer** - Workspace views containing windows
3. **layer_shell_overlay** - Panels, notifications (Top/Overlay wlr_layer)
4. **dock** - Compositor dock
5. **expose_layer** - Workspace selector, app switcher UI
6. **popup_overlay** - Popups (always highest)

## Code Locations

### Data Structures

| File | Component | Description |
|------|-----------|-------------|
| `src/shell/layer.rs` | `LayerShellSurface` | Wrapper struct bridging Smithay's `LayerSurface` with lay_rs scene graph |
| `src/state/mod.rs:142` | `layer_surfaces: HashMap` | Compositor-owned map of all layer surfaces by ObjectId |
| `src/workspaces/mod.rs:109-111` | `layer_shell_background`, `layer_shell_overlay` | Container layers in scene graph |

### Protocol Handlers

| File | Function | Description |
|------|----------|-------------|
| `src/shell/mod.rs` | `new_layer_surface()` | Creates LayerShellSurface, lay_rs layer, registers with Smithay |
| `src/shell/mod.rs` | `layer_destroyed()` | Cleans up lay_rs layer and HashMap entry |
| `src/shell/mod.rs` | `commit()` | Routes layer surface commits to rendering update |
| `src/state/mod.rs` | `update_layer_surface()` | Imports buffers, updates layer geometry and draw content |

### Rendering

| File | Function | Description |
|------|----------|-------------|
| `src/state/mod.rs` | `update_layer_surface()` | Collects render elements, imports buffers as Skia textures, updates lay_rs layer |
| `src/workspaces/mod.rs` | `create_layer_shell_layer()` | Creates child layer at correct z-order position |
| `src/workspaces/mod.rs` | `remove_layer_shell_layer()` | Removes layer from scene graph |

### Input Routing

| File | Lines | Description |
|------|-------|-------------|
| `src/input_handler.rs:388-402` | Keyboard focus | Checks overlay/top layer surfaces for keyboard focus |
| `src/input_handler.rs:443-465` | Keyboard focus | Checks bottom/background layers after windows |
| `src/input_handler.rs:515-530` | Pointer focus | Hit-tests layer surfaces in `surface_under()` |

### Fullscreen/Expose Integration

| File | Function | Description |
|------|----------|-------------|
| `src/workspaces/mod.rs:778-787` | `set_fullscreen_overlay_visibility()` | Animates overlay opacity on fullscreen enter/exit |
| `src/workspaces/mod.rs:1751-1754` | `scroll_to_workspace_index()` | Animates overlay opacity based on target workspace fullscreen state |
| `src/workspaces/mod.rs:703` | `expose_show_all_animate()` | Fades overlay during expose mode |
| `src/shell/xdg.rs:422-423` | Fullscreen request | Calls `set_fullscreen_overlay_visibility(true)` |
| `src/shell/xdg.rs:506-507` | Unfullscreen request | Calls `set_fullscreen_overlay_visibility(false)` |

## LayerShellSurface API

```rust
// src/shell/layer.rs
pub struct LayerShellSurface {
    layer_surface: LayerSurface,    // Smithay handle
    pub layer: Layer,               // lay_rs scene graph layer
    output: Output,                 // Bound output
    wlr_layer: WlrLayer,           // Background/Bottom/Top/Overlay
    namespace: String,              // Client-provided namespace
    last_configure_serial: AtomicU32,
    geometry: Rectangle<i32, Logical>,
}

impl LayerShellSurface {
    // Accessors
    pub fn id(&self) -> ObjectId;
    pub fn output(&self) -> &Output;
    pub fn wlr_layer(&self) -> WlrLayer;
    pub fn namespace(&self) -> &str;
    pub fn geometry(&self) -> Rectangle<i32, Logical>;
    
    // Cached state accessors (from Smithay)
    pub fn keyboard_interactivity(&self) -> KeyboardInteractivity;
    pub fn anchor(&self) -> Anchor;
    pub fn exclusive_zone(&self) -> ExclusiveZone;
    pub fn margin(&self) -> (i32, i32, i32, i32);  // (top, right, bottom, left)
    pub fn requested_size(&self) -> Size<i32, Logical>;
    
    // Geometry computation
    pub fn compute_geometry(&self, output_geometry: Rectangle) -> Rectangle;
    pub fn is_anchored_to_all_edges(&self) -> bool;
    
    // Focus
    pub fn can_receive_keyboard_focus(&self) -> bool;
}
```

## Surface Lifecycle

### Creation Flow

```
Client: get_layer_surface(surface, output, layer, namespace)
    │
    ▼
new_layer_surface() in src/shell/mod.rs
    │
    ├── Create LayerSurface wrapper (Smithay)
    ├── Call workspaces.create_layer_shell_layer(wlr_layer, namespace)
    │   └── Creates layers::Layer at correct z-order
    ├── Create LayerShellSurface struct
    ├── Store in layer_surfaces HashMap
    └── Register with Smithay's layer_map_for_output
```

### Commit Flow

```
Client: wl_surface.commit()
    │
    ▼
commit() in src/shell/mod.rs
    │
    ├── Check if surface is in layer_surfaces map
    └── Call update_layer_surface() in src/state/mod.rs
        │
        ├── Get output geometry
        ├── Compute layer geometry via compute_geometry()
        ├── Collect render elements via surface tree traversal
        ├── Import buffers as Skia textures
        ├── Update lay_rs layer position/size
        └── Set draw content with texture drawing
```

### Destroy Flow

```
Client disconnects or zwlr_layer_surface_v1.destroy()
    │
    ▼
layer_destroyed() in src/shell/mod.rs
    │
    ├── Remove from layer_surfaces HashMap
    ├── Call workspaces.remove_layer_shell_layer()
    │   └── Removes lay_rs layer from scene graph
    └── Unmap from Smithay's layer_map
```

## Z-Order Mapping

| WlrLayer | Container Layer | Z-Order | Visibility During Fullscreen |
|----------|----------------|---------|------------------------------|
| Background | `layer_shell_background` | 0 (lowest) | Visible |
| Bottom | `layer_shell_background` | 0 | Visible |
| Top | `layer_shell_overlay` | 2 | **Fades out** |
| Overlay | `layer_shell_overlay` | 2 | **Fades out** |

### Fullscreen Behavior

When a window enters fullscreen:
1. Layer shell overlay (Top/Overlay layers) fades to opacity 0.0
2. Background/Bottom layers remain visible
3. Dock hides (existing behavior)

When exiting fullscreen or scrolling to non-fullscreen workspace:
1. Layer shell overlay fades back to opacity 1.0
2. Dock shows (existing behavior)

## Anchor/Margin Geometry Computation

The `compute_geometry()` method calculates surface position based on:

```rust
// Horizontal positioning
if anchor_left && anchor_right {
    // Stretch horizontally, apply left/right margins
    width = output_width - margin_left - margin_right;
    x = margin_left;
} else if anchor_left {
    x = margin_left;
} else if anchor_right {
    x = output_width - width - margin_right;
} else {
    // Center horizontally
    x = (output_width - width) / 2;
}

// Vertical positioning (same pattern)
```

## Input Focus Priority

Pointer focus order in `surface_under()`:
1. Fullscreen window (if present)
2. **Overlay/Top layer surfaces** ← Layer shell
3. Dock
4. Regular windows
5. **Bottom/Background layer surfaces** ← Layer shell

Keyboard focus order:
1. **Overlay/Top layer surfaces** (if `keyboard_interactivity != None`)
2. Focused window
3. **Bottom/Background layer surfaces** (if `keyboard_interactivity != None`)

## Animation Integration

### Opacity Animations

Layer shell overlay participates in compositor animations:

```rust
// During expose mode entry (src/workspaces/mod.rs:703)
let layer_shell_overlay_opacity = 1.0.interpolate(&0.0, delta);
self.layer_shell_overlay.set_opacity(layer_shell_overlay_opacity, transition);

// During fullscreen transitions (src/workspaces/mod.rs:778-787)
pub fn set_fullscreen_overlay_visibility(&self, is_fullscreen: bool) {
    let target_opacity = if is_fullscreen { 0.0 } else { 1.0 };
    let transition = Some(Transition::ease_in_out_quad(1.4));
    self.layer_shell_overlay.set_opacity(target_opacity, transition);
}

// During workspace scroll (src/workspaces/mod.rs:1751-1754)
let target_opacity = if workspace.get_fullscreen_mode() { 0.0 } else { 1.0 };
self.layer_shell_overlay.set_opacity(target_opacity, Some(transition));
```

## Testing

### Manual Testing

Launch a layer-shell client and verify:
```bash
# Example: swaybg for background layer
WAYLAND_DISPLAY=<socket> swaybg -m fill -i /path/to/wallpaper.png

# Example: waybar for top layer
WAYLAND_DISPLAY=<socket> waybar

# Example: mako for overlay layer (notifications)
WAYLAND_DISPLAY=<socket> mako
```

### Verification Checklist

- [ ] Surface appears at correct z-order
- [ ] Anchors position surface correctly
- [ ] Margins are respected
- [ ] Pointer focus works on layer surfaces
- [ ] Keyboard focus works (if interactivity enabled)
- [ ] Surface fades during fullscreen/expose mode
- [ ] Surface restored on fullscreen exit
- [ ] Clean destruction without leaks

## Known Limitations

1. **No exclusive zone tracking** - Workspace geometry not updated for panel reservations
2. **Global containers** - `layer_shell_background/overlay` are global, not per-output
3. **No output migration** - Surfaces don't migrate when outputs disappear
4. **No per-surface animations** - All surfaces in a container animate together

## Future Work

### Phase 2: Exclusive Zones

Track per-output space reservations:
```rust
pub struct ExclusiveZones {
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
}
```

Update `Workspaces::get_logical_rect()` to subtract reserved zones from usable workspace area.

### Per-Output Organization

```rust
pub struct OutputLayers {
    background: Vec<LayerShellSurface>,
    bottom: Vec<LayerShellSurface>,
    top: Vec<LayerShellSurface>,
    overlay: Vec<LayerShellSurface>,
    exclusive_zones: ExclusiveZones,
    output: Output,
}
```

### Output Migration

When output disappears:
1. Find surfaces bound to that output
2. Migrate to fallback output (primary)
3. Reconfigure with new output geometry
4. Log warning for client awareness
