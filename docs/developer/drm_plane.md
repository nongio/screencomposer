## DRM Plane Selection in Otto

This document explains how Smithay's `DrmCompositor` selects render elements that can be assigned to hardware DRM planes for direct scanout, and how Otto configures this functionality.

## Overview

Hardware planes are dedicated display hardware layers that can composite surfaces without GPU involvement. Using planes efficiently reduces power consumption and improves performance by offloading composition work from the GPU to the display controller.

## Compositor Modes

Otto supports two compositor modes, configured via `compositor_mode` in the config:

### 1. Surface Mode (`compositor_mode == "surface"`)
- Uses `GbmBufferedSurface` with simple swapchain rendering
- All composition happens on the GPU
- Single primary plane used for scanout
- No plane optimization
- Simpler but less efficient

### 2. Compositor Mode (default)
- Uses Smithay's `DrmCompositor`
- Supports multiple hardware planes
- Automatic plane assignment and optimization
- Direct scanout for fullscreen windows
- Overlay plane usage when available

## Available Plane Types

From [src/udev.rs](../src/udev.rs):

- **Primary plane** (1): Main display surface, always present and required
- **Overlay planes** (0-N): Additional hardware compositing layers for window surfaces
- **Cursor plane** (0-1): Dedicated hardware cursor layer

The number of available planes is queried from the DRM driver at initialization:

```rust
let mut planes = surface.planes().clone();
println!("Available planes - Primary: 1, Overlay: {}, Cursor: {}", 
    planes.overlay.len(), 
    planes.cursor.len());
```

### Platform-Specific Limitations

**NVIDIA drivers**: Overlay planes are explicitly disabled due to driver compatibility issues:

```rust
if driver.name().contains("nvidia") || driver.description().contains("nvidia") {
    planes.overlay = vec![];
}
```

## DrmCompositor Initialization

The compositor is configured with available planes at connector initialization:

```rust
let mut compositor = DrmCompositor::new(
    &output,
    surface,
    Some(planes),      // Pass available planes to compositor
    allocator,
    device.gbm.clone(),
    color_formats,     // Supported color formats for rendering
    render_formats,    // Formats supported by render node
    device.drm.cursor_size(),
    Some(device.gbm.clone()),
)?;
```

## Plane Selection Process

The actual plane assignment happens inside Smithay's `DrmCompositor::render_frame()` method, which analyzes the render element tree and attempts intelligent plane assignment.

### 1. Direct Scanout (Primary Plane)

When a single fullscreen window covers the entire output, the compositor attempts **direct scanout**:

- The window's buffer is placed directly on the primary plane
- No GPU composition occurs
- Maximum efficiency for fullscreen applications (games, video players)

**Conditions for direct scanout**:
```rust
let allow_direct_scanout = 
    Config::with(|c| c.direct_scanout) && 
    pointer_in_output == false;

let fullscreen_window = if allow_direct_scanout {
    // Find single fullscreen window
    workspace.fullscreen_window()
} else {
    None
};
```

**Mode transitions**: When switching between direct scanout and normal composition:
```rust
let mode_changed = is_direct_scanout != surface.was_direct_scanout;
if mode_changed {
    surface.compositor.reset_buffers();  // Clear stale state
}
```

### 2. Overlay Plane Assignment

The compositor evaluates each render element for overlay plane compatibility based on:

#### Buffer Format Compatibility
Elements must provide dmabuf-backed textures in formats supported by both:
- The hardware plane (from `plane_info().formats`)
- The render node (for fallback composition)

```rust
let planes_formats = surface
    .plane_info()
    .formats
    .iter()
    .copied()
    .filter(|format| all_render_formats.contains(format))
    .collect::<FormatSet>();
```

#### Other Compatibility Factors
- **Transform compatibility**: Rotation, scaling, mirroring support
- **Z-order requirements**: Plane stacking order must match element order
- **Opaque regions**: Fully opaque rectangular regions are better candidates
- **Pixel format**: Must match plane's supported formats (XRGB8888, ARGB8888, etc.)

### 3. Cursor Plane

Hardware cursor plane is used when:
- A cursor plane is available
- The cursor image fits within `drm.cursor_size()` (typically 64×64 or 128×128)
- The cursor format is compatible (usually ARGB8888)

Pointer elements are rendered and can be assigned to the cursor plane:
```rust
workspace_render_elements.extend(pointer_element.render_elements(
    renderer,
    cursor_pos_scaled,
    cursor_rescale.into(),
    1.0,
));
```

### 4. Fallback Composition

Elements that cannot be assigned to planes are:
- Rendered to an offscreen buffer via GPU
- Composited together
- The final result placed on the primary plane

## Dmabuf Feedback for Scanout Optimization

Otto provides dmabuf feedback to Wayland clients to help them allocate buffers in scanout-compatible formats:

```rust
fn get_surface_dmabuf_feedback(
    primary_gpu: DrmNode,
    render_node: DrmNode,
    gpus: &mut GpuManager,
    composition: &SurfaceComposition,
) -> Option<DrmSurfaceDmabufFeedback> {
    // Returns format preferences with scanout tranche
    // Clients can allocate buffers that skip GPU composition
}
```

The feedback includes:
- **Render tranche**: Formats supported for GPU rendering (fallback)
- **Scanout tranche**: Formats supported for direct plane scanout (optimal)

## Render Element Requirements

For a render element to be eligible for plane assignment:

1. **Must provide dmabuf texture**: Software buffers cannot use planes
2. **Format compatibility**: Element's buffer format must be in plane's supported list
3. **Size constraints**: Must fit within plane limitations
4. **Transform support**: Plane must support any required rotations/transforms
5. **Proper alpha**: Plane may have alpha channel requirements

## Decision Flow

```
Render Elements
    ↓
DrmCompositor::render_frame()
    ↓
Is single fullscreen window?
    ├─ Yes → Try direct scanout on primary plane
    │         ├─ Success → Skip composition
    │         └─ Fail → Continue to element tree
    └─ No → Continue
    ↓
For each render element:
    ├─ Is cursor element + cursor plane available?
    │   ├─ Yes + compatible → Assign to cursor plane
    │   └─ No or incompatible → Continue
    ├─ Is compatible with overlay plane?
    │   ├─ Yes + overlay available → Assign to overlay plane
    │   └─ No or incompatible → Continue
    └─ Composite to primary plane buffer
    ↓
Submit atomic commit with plane assignments
```

## Performance Implications

**Benefits of plane usage**:
- Reduced GPU load (less composition work)
- Lower power consumption
- Potentially lower latency
- Better frame pacing for video playback

**Tradeoffs**:
- Plane format/size constraints may force fallback
- Atomic commit complexity
- Some drivers have buggy plane support (NVIDIA)

## Debugging Plane Usage

The compositor can be configured with debug flags:
```rust
compositor.set_debug_flags(debug_flags);
```

To see plane allocation information:
```bash
# Check available planes at startup
cargo run -- --tty-udev
# Output shows: "Available planes - Primary: 1, Overlay: X, Cursor: Y"
```

## Related Files

- [src/udev.rs](../src/udev.rs) - DRM backend and compositor configuration
- [src/render.rs](../src/render.rs) - Render element tree construction
- [docs/rendering.md](./rendering.md) - Overall rendering pipeline
- [docs/render_loop.md](./render_loop.md) - Frame rendering flow


### Smithay's Plane Assignment Process

When `DrmCompositor::render_frame()` is called, it walks all render elements and attempts plane assignment:

1. **Query underlying storage**: Calls `element.underlying_storage(renderer)` on each element
2. **Skip if no storage**: If returns `None`, element **must be GPU-composited** (no plane assignment possible)
3. **Export dmabuf**: If returns storage, calls `ExportBuffer::from_underlying_storage()`
4. **Create framebuffer**: Exports framebuffer via `framebuffer_exporter.add_framebuffer()`
5. **Test plane compatibility**: Calls `try_assign_plane()` to check format, transform, z-order
6. **Assign to plane**: If compatible, assigns to overlay/cursor/primary plane for direct scanout
7. **Fallback to GPU**: If incompatible, element is rendered via GPU to primary plane's swapchain

### Element Requirements for Plane Assignment

For a render element to be eligible for hardware plane assignment:

1. **Provide `underlying_storage()`**:
   ```rust
   fn underlying_storage(&self, renderer: &R) -> Option<UnderlyingStorage<'_>> {
       Some(UnderlyingStorage::from_dmabuf(&self.dmabuf))
   }
   ```

2. **Set `Kind::ScanoutCandidate`**:
   ```rust
   fn kind(&self) -> Kind {
       Kind::ScanoutCandidate
   }
   ```

3. **Dmabuf-backed texture**: Must render to a dmabuf buffer, not CPU memory or anonymous GPU texture

4. **Format compatibility**: Dmabuf format must be in plane's `formats` list (XRGB8888, ARGB8888, etc.)

5. **Transform compatibility**: Plane must support any required rotations/transforms

### Smithay Plane Assignment Tracing

Enable detailed logging to observe plane assignment decisions:
```bash
RUST_LOG=smithay::backend::drm::compositor=trace cargo run -- --tty-udev
```

Look for trace messages like:
- `assigned element ... to overlay plane ...` - Success
- `skipping element ... on overlay plane(s), element kind not scanout-candidate` - Wrong Kind
- `skipping direct scan-out ... format ... not supported` - Format mismatch
- `failed to claim plane` - Plane already in use

## References

- Smithay's `DrmCompositor` implementation handles the actual plane assignment algorithm
- The decision logic is internal to Smithay and not directly visible in Otto
- Otto's role is to configure the compositor and provide properly formatted render elements
- See `smithay/src/backend/drm/compositor/mod.rs` for full plane assignment implementation
