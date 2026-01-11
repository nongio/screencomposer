# Scene Element Damage Clipping Optimization

## What Was Implemented

Enabled damage-based clipping for the scene element rendering to avoid rendering pixels outside damaged regions.

## The Problem

Before this optimization, the scene element (lay-rs scene graph) was rendering the entire scene tree even when only small portions changed:

```rust
// Old code - always renders full tree
render_node_tree(root_id, arena, renderable_arena, canvas, 1.0);
```

## The Solution

Use Skia's Region clipping to restrict rendering to only damaged areas:

```rust
// New code - clips to damaged regions
let mut clip_region = lay_rs::skia::Region::new();
for damage_rect in damage.iter() {
    clip_region.op_rect(&irect, RegionOp::Union);
}
canvas.clip_region(&clip_region, ClipOp::Intersect);
render_node_tree(root_id, arena, renderable_arena, canvas, 1.0);
```

## How It Works

### Before (No Clipping)
```
Damage: [Small rect at cursor]
Rendering: Entire scene tree traversed and drawn
GPU Work: Full screen rasterization
```

### After (With Clipping)
```
Damage: [Small rect at cursor]
Rendering: Scene tree traversed, but Skia clips output
GPU Work: Only damaged region rasterized
```

## Implementation Details

**File**: `src/render_elements/scene_element.rs`

### Key Changes

1. **Empty damage check**
   - If no damage, render full scene (unchanged)
   
2. **Build clip region**
   - Union all damage rects into a Skia Region
   - Handles complex multi-rect damage efficiently

3. **Apply clip and render**
   - Save canvas state
   - Apply region clip with Intersect operation
   - Render scene tree (Skia clips automatically)
   - Restore canvas state

### Code Location

```rust
// Line ~251 in scene_element.rs
if damage.is_empty() {
    // No damage, render full scene
    render_node_tree(root_id, arena, renderable_arena, canvas, 1.0);
} else {
    // Build clip region from damage rects
    let mut clip_region = lay_rs::skia::Region::new();
    for d in damage.iter() {
        let irect = lay_rs::skia::IRect::from_xywh(...);
        clip_region.op_rect(&irect, RegionOp::Union);
    }
    
    // Render with clipping
    canvas.save();
    canvas.clip_region(&clip_region, ClipOp::Intersect);
    render_node_tree(root_id, arena, renderable_arena, canvas, 1.0);
    canvas.restore();
}
```

## Expected Performance Impact

### Cursor Movement (1% damage)
- **Before**: Traverse full tree, render ~2M pixels
- **After**: Traverse full tree, render ~20K pixels
- **GPU savings**: ~99%

### Window Drag (10% damage)
- **Before**: Traverse full tree, render ~2M pixels
- **After**: Traverse full tree, render ~200K pixels
- **GPU savings**: ~90%

### Important Notes

1. **CPU traversal still happens**
   - The scene tree is still fully traversed
   - CPU time not reduced significantly
   - GPU rasterization is what's optimized

2. **Skia handles clipping**
   - Automatic discard of pixels outside clip
   - No additional code in render_node_tree needed
   - Works with all Skia drawing operations

3. **Region vs per-rect clipping**
   - Using Region is more efficient than multiple save/restore/clip cycles
   - Skia optimizes complex clip regions internally

## Comparison with Skia Renderer Optimization

This complements the skia_renderer.rs optimizations:

| Component | Optimization | Benefit |
|-----------|-------------|---------|
| **Scene Element** | Clip scene rendering to damage | Reduces GPU work for scene |
| **Skia Renderer** | Per-rect clear() and render_texture_from_to() | Reduces GPU work for textures |

Both together provide comprehensive partial rendering.

## Testing

### Before Optimization
```bash
./scripts/measure_baseline.sh winit
# Note render times
```

### After Optimization
```bash
cargo build --release
./scripts/measure_baseline.sh winit
# Compare render times - should be faster
```

### Expected Results

**Cursor-only movement:**
- 50-70% faster rendering
- Lower CPU usage (less GPU wait time)

**Active window manipulation:**
- 30-50% faster rendering
- More consistent frame times

## Potential Further Optimizations

### 1. Scene Tree Culling (Future)
Currently: Tree is fully traversed even with clipping

Improvement: Skip subtrees entirely outside damaged regions
```rust
if node_bounds.intersects(damage_region) {
    render_node(...);
}
```

### 2. Dirty Region Tracking in lay-rs (Future)
Have lay-rs track which nodes changed, only traverse those subtrees

### 3. Occlusion Culling (Future)
Skip rendering nodes fully occluded by opaque nodes above them

## Risks and Mitigations

| Risk | Mitigation | Status |
|------|------------|--------|
| Clip state corruption | Always use save()/restore() | ✅ Implemented |
| Empty region edge case | Check is_empty() before clipping | ✅ Handled |
| Performance regression | Feature flag possible | ✅ Direct edit (can revert) |

## Debugging

If visual artifacts appear:

1. **Check damage tracking**
   - Ensure damage rects are correct
   - Look for off-by-one errors

2. **Visual debug mode**
   - Uncomment debug code at line ~278 in scene_element.rs
   - Will draw red outline around clip region

3. **Disable optimization**
   - Comment out the clip_region code
   - Render full scene to verify artifacts disappear

## Rollback

To disable this optimization:

```rust
// In scene_element.rs, replace the optimized code with:
if let Some(root_id) = root_id {
    render_node_tree(root_id, arena, renderable_arena, canvas, 1.0);
}
```

## Summary

✅ **Scene element now respects damage clipping**  
✅ **Uses efficient Skia Region for multi-rect damage**  
✅ **Complements skia_renderer optimizations**  
✅ **Expected 30-70% GPU savings**  
✅ **Ready to measure with baseline scripts**

This is a foundational optimization that makes partial rendering effective for the scene graph layer.
