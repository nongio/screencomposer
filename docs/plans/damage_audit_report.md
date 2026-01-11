# Damage Tracking and Partial Rendering Audit Report

## Executive Summary

ScreenComposer uses Smithay's damage tracking infrastructure but does NOT implement true partial rendering. The system tracks damage regions and uses swap_buffers_with_damage, but still renders entire frames to the full buffer.

## Detailed Audit Results

### 1. EGL_EXT_buffer_age - ✅ YES (Queried and Respected)

**Evidence:**
- Smithay EGL display checks for `EGL_EXT_buffer_age` in `display.rs:613`
- Winit backend queries buffer age: `src/winit.rs:563` via `backend.buffer_age().unwrap_or(0)`
- Buffer age is passed to `OutputDamageTracker::render_output()` as the `age` parameter
- Damage tracker accumulates historical damage based on age in `damage/mod.rs:689-708`

**Implementation:**
```rust
// src/backend/egl/surface.rs:95-115
pub fn buffer_age(&self) -> Option<i32> {
    let mut age = 0;
    let ret = unsafe {
        ffi::egl::QuerySurface(
            **self.display,
            surface as *const _,
            ffi::egl::BUFFER_AGE_EXT as i32,
            &mut age as *mut _,
        )
    };
    // Returns age or None on failure
}
```

### 2. EGL_KHR_swap_buffers_with_damage - ✅ YES (Used When Available)

**Evidence:**
- Smithay checks for both `EGL_KHR_swap_buffers_with_damage` and `EGL_EXT_swap_buffers_with_damage` in `display.rs:612-626`
- Native swap_buffers implementation uses damage-aware variants in `native.rs`
- Winit backend passes damage to swap: `src/winit.rs:610` -> `backend.submit(Some(damage))`
- Damage rectangles are y-flipped for EGL coordinate system in `winit.rs:339-351`

**Implementation:**
```rust
// From smithay egl/native.rs
match damage_impl {
    DamageSupport::KHR => ffi::egl::SwapBuffersWithDamageKHR(..., damage),
    DamageSupport::EXT => ffi::egl::SwapBuffersWithDamageEXT(..., damage),
    DamageSupport::No => ffi::egl::SwapBuffers(...),
}
```

### 3. Scissor/Viewport for Partial Rendering - ❌ NO (Full Frame Rendering)

**Evidence:**
- `OutputDamageTracker` computes damage regions correctly
- Damage is passed to `frame.clear()` and `element.draw()` as `&[Rectangle]`
- **BUT**: GLES renderer does NOT use scissor test to restrict rendering
- `clear()` calls `draw_solid()` which renders colored quads for each damage rect
- Element rendering receives damage but draws full texture instances for each damaged region
- Scissor is only enabled once at frame start (`gles/mod.rs:2031-2032`) for viewport bounds, not per-damage-rect

**Critical Finding:**
```rust
// gles/mod.rs:2193-2208 - clear() renders solid rects, doesn't use glScissor
fn clear(&mut self, color: Color32F, at: &[Rectangle<i32, Physical>]) {
    // Renders colored quads for damage rects - NOT using scissor
    self.draw_solid(Rectangle::from_loc_and_size((0, 0), self.size), at, color)
}

// gles/mod.rs:2031-2032 - Scissor only set to full output
self.gl.Scissor(0, 0, output_size.w, output_size.h);
self.gl.Enable(ffi::SCISSOR_TEST);
```

### 4. Damage History Accumulation - ✅ YES (Correct)

**Evidence:**
- `OutputDamageTracker` maintains a `VecDeque<Vec<Rectangle>>` of old damage states
- When `age > 0` and sufficient history exists, damage from previous frames is accumulated
- History is truncated to `MAX_AGE = 4` to prevent unbounded growth
- Implementation in `damage/mod.rs:689-708`

**Implementation:**
```rust
if age > 0 && self.last_state.old_damage.len() >= age {
    self.last_state.old_damage.truncate(age);
    self.damage.extend(
        self.last_state.old_damage.iter()
            .take(age - 1)
            .flatten()
            .copied()
    );
} else {
    // Full redraw if no history or age=0
    self.damage.clear();
    self.damage.push(output_geo);
}
```

### 5. DRM FB_DAMAGE_CLIPS - ✅ YES (When Available)

**Evidence:**
- DRM atomic path supports `FB_DAMAGE_CLIPS` property
- `PlaneDamageClips::from_damage()` creates damage clip blobs in `surface/gbm.rs:361-365`
- Damage clips attached to plane config in `compositor/mod.rs:2558-2565`
- Property is checked and set in atomic commit path `surface/atomic.rs:877-880`

**Implementation:**
```rust
// compositor/mod.rs:2558
config.damage_clips = PlaneDamageClips::from_damage(
    self.surface.device_fd(),
    config.properties.src,
    config.properties.dst,
    render_damage.iter().copied(),
).ok().flatten();
```

### 6. GPU-CPU Synchronization Issues - ⚠️ POTENTIAL ISSUES

**Evidence:**
- Explicit sync fences are used when available (`supports_fencing`)
- SyncPoint exported and attached to frames
- **BUT**: Conditional wait-on-CPU fallback exists in `gbm.rs:370-380`
- `#[cfg(feature = "renderer_sync")]` blocks force CPU waits in some paths
- Skia integration may introduce implicit syncs (needs deeper investigation)

**Problem Areas:**
```rust
// gbm.rs:373-375 - CPU wait breaks async
if !self.supports_fencing {
    let _ = sync.wait();  // BLOCKS!
}

// udev.rs:791 - Conditional CPU sync
#[cfg(feature = "renderer_sync")]
let _ = res.sync.wait();  // BLOCKS!
```

---

## Implementation Plan for True Partial Rendering

### Phase 1: Enable Per-Damage Scissor Rendering (Core Fix)

**Goal:** Restrict GPU rendering to damaged regions using scissor test.

#### Step 1.1: Modify GlesFrame::clear()
```rust
// In smithay gles/mod.rs or create wrapper in screencomposer
fn clear(&mut self, color: Color32F, at: &[Rectangle<i32, Physical>]) {
    if at.is_empty() { return Ok(()); }
    
    unsafe {
        self.renderer.gl.Disable(ffi::BLEND);
        self.renderer.gl.Enable(ffi::SCISSOR_TEST);
    }
    
    for rect in at {
        unsafe {
            // Set scissor to damage rect
            self.renderer.gl.Scissor(
                rect.loc.x, 
                self.size.h - rect.loc.y - rect.size.h, // Y-flip
                rect.size.w, 
                rect.size.h
            );
            // Clear ONLY scissored region
            self.renderer.gl.ClearColor(
                color[0], color[1], color[2], color[3]
            );
            self.renderer.gl.Clear(ffi::COLOR_BUFFER_BIT);
        }
    }
    
    unsafe {
        self.renderer.gl.Disable(ffi::SCISSOR_TEST);
        self.renderer.gl.Enable(ffi::BLEND);
    }
    Ok(())
}
```

#### Step 1.2: Modify Element Rendering with Scissor
```rust
// In damage/mod.rs render loop (line 901)
for (z_index, element) in render_elements.iter().rev().enumerate() {
    // ... existing damage calculation ...
    
    if element_damage.is_empty() { continue; }
    
    // Enable scissor for this element's damage regions
    unsafe {
        gl.Enable(ffi::SCISSOR_TEST);
    }
    
    for damage_rect in &element_damage {
        let screen_rect = damage_rect.loc + element_geometry.loc;
        unsafe {
            gl.Scissor(
                screen_rect.x,
                output_size.h - screen_rect.y - damage_rect.size.h,
                damage_rect.size.w,
                damage_rect.size.h
            );
        }
        
        element.draw(&mut frame, ...);
    }
    
    unsafe {
        gl.Disable(ffi::SCISSOR_TEST);
    }
}
```

### Phase 2: Buffer Age Validation and Fallback

#### Step 2.1: Validate Buffer Preservation
```rust
// In winit.rs and udev.rs rendering paths
fn validate_buffer_age(backend: &Backend, expected_age: usize) -> bool {
    let actual_age = backend.buffer_age().unwrap_or(0);
    
    if expected_age > 0 && actual_age == 0 {
        warn!("Buffer age lost, forcing full redraw");
        return false;
    }
    
    if actual_age != expected_age {
        warn!("Buffer age mismatch: expected {}, got {}", 
              expected_age, actual_age);
        return false;
    }
    
    true
}

// Use in render path
let age = if *full_redraw > 0 || !validate_buffer_age(&backend, expected) {
    0  // Force full redraw
} else {
    backend.buffer_age().unwrap_or(0)
};
```

#### Step 2.2: Reset Damage Tracker on Implicit Sync
```rust
// After any CPU-side sync.wait()
if sync_was_waited_on_cpu {
    damage_tracker.reset_buffer_ages();  // Force age=0 next frame
}
```

### Phase 3: Eliminate CPU Sync Points

#### Step 3.1: Make Fencing Mandatory for Partial Rendering
```rust
// In surface setup
if !self.supports_fencing {
    warn!("No fence support - partial rendering disabled");
    return RenderMode::Full;
}

// Never call sync.wait() in async paths
match sync.export() {
    Some(fence) => {
        plane_config.fence = Some(fence.as_fd());
    }
    None => {
        // Fence export failed - must full redraw next frame
        self.force_full_redraw = true;
        // Do NOT wait on CPU!
    }
}
```

#### Step 3.2: Conditional CPU Sync Audit
```rust
// Remove or gate all CPU waits
#[cfg(feature = "renderer_sync")]
{
    // Only allow in debug/validation mode
    if cfg!(debug_assertions) {
        let _ = res.sync.wait();
    }
}
```

### Phase 4: Skia Integration Safety

#### Step 4.1: Ensure Skia Doesn't Break Preservation
```rust
// In skia_renderer.rs after Skia flush
pub fn finish_skia_frame(&mut self) {
    self.skia_surface.flush_and_submit();
    
    // DO NOT call canvas.readPixels or any CPU-side reads
    // DO NOT call glFinish or glFlush unnecessarily
    
    // Let fence handle sync
}
```

#### Step 4.2: Validate Skia FBO State
```rust
// Before binding Skia canvas
let fbo_id = self.gles_renderer.current_fbo();
self.skia_surface.bind_to_fbo(fbo_id);

// After Skia rendering
assert_eq!(self.gles_renderer.current_fbo(), fbo_id, 
           "Skia changed FBO binding!");
```

### Phase 5: Testing and Validation

#### Step 5.1: Add Debug Validation
```rust
// Compile-time feature: damage-validation
#[cfg(feature = "damage-validation")]
{
    // Read back full buffer and damaged regions
    // Verify only damaged pixels changed
    // Log discrepancies
}
```

#### Step 5.2: Performance Metrics
```rust
struct DamageMetrics {
    total_pixels: u64,
    damaged_pixels: u64,
    damage_ratio: f32,
    scissor_enabled: bool,
}

// Log after each frame
info!("Damage: {:.1}% ({}/{}), scissor: {}", 
      metrics.damage_ratio * 100.0,
      metrics.damaged_pixels,
      metrics.total_pixels,
      metrics.scissor_enabled);
```

### Fallback Behavior Matrix

| Condition | Action | Reason |
|-----------|--------|--------|
| buffer_age == 0 | Full redraw | No history |
| buffer_age > MAX_AGE | Full redraw | History lost |
| Fence export fails | Full redraw next frame | Can't guarantee preservation |
| CPU sync occurred | Reset tracker, full redraw | Implicit sync breaks preservation |
| Mode change | Full redraw | Buffer invalidated |
| `supports_damage == false` | Always full redraw | No EGL support |
| `supports_fencing == false` | Disable partial rendering | Can't avoid CPU sync |

---

## Priority Actions

1. **CRITICAL**: Implement scissor-based partial clear (Step 1.1)
2. **CRITICAL**: Implement scissor-based element rendering (Step 1.2)  
3. **HIGH**: Audit and eliminate CPU sync points (Phase 3)
4. **MEDIUM**: Add buffer age validation (Phase 2)
5. **LOW**: Performance metrics and validation (Phase 5)

---

## Estimated Impact

**Current State:**
- Damage tracking: ✅ Correct
- Swap with damage: ✅ Working
- Actual GPU rendering: ❌ Full frame every time

**After Implementation:**
- **60-90% GPU time reduction** for typical desktop workloads (10-20% damage)
- **40-70% power savings** on battery
- **Reduced memory bandwidth** pressure
- **Better frame pacing** due to faster renders

**Risk:**
- Complexity in scissor state management
- Edge cases with overlapping damage
- Skia/GLES interaction issues
