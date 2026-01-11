# Rendering Metrics - Backend Integration Summary

## Integration Status: ✅ COMPLETE (Both Backends)

The rendering metrics system is fully integrated into **both** backends:
- ✅ **Winit** (development/windowed mode)
- ✅ **Udev** (production/DRM mode)

---

## Winit Backend Integration

**File:** `src/winit.rs`

### Frame Timing
```rust
// Line ~543: Start timing when render begins
let _frame_timer = state.render_metrics.start_frame();
```

### Damage Recording
```rust
// Line ~609-615: Record damage after rendering
if let Some(ref damage) = render_output_result.damage {
    let mode = output.current_mode().unwrap();
    let output_size = (mode.size.w, mode.size.h);
    state.render_metrics.record_damage(output_size, damage);
}
```

### Periodic Logging
```rust
// Line ~717: Log stats every 5 seconds in main loop
state.render_metrics.maybe_log_stats(false);
```

---

## Udev Backend Integration

**File:** `src/udev.rs`

### Metrics in SurfaceData
```rust
// Line 877: SurfaceData struct has metrics field
struct SurfaceData {
    // ... other fields ...
    render_metrics: Option<Arc<crate::render_metrics::RenderMetrics>>,
}
```

### Initialization
```rust
// Line 1343: Metrics passed from ScreenComposer state
let surface = SurfaceData {
    // ... other fields ...
    render_metrics: Some(self.render_metrics.clone()),
};
```

### Frame Timing
```rust
// Line 2000: Start timing at beginning of render_surface()
let _frame_timer = surface.render_metrics.as_ref().map(|m: &Arc<_>| m.start_frame());
```

### Damage Recording
```rust
// Line 2226-2230: Record damage after compositor renders
if let (Some(ref damage_rects), Some(ref metrics)) = (&damage, &surface.render_metrics) {
    let mode = output.current_mode().unwrap();
    let output_size = (mode.size.w, mode.size.h);
    metrics.as_ref().record_damage(output_size, damage_rects);
}
```

### Periodic Logging
```rust
// Line 597: Log stats every 5 seconds in main event loop
state.render_metrics.maybe_log_stats(false);
```

---

## Common State Storage

**File:** `src/state/mod.rs`

Both backends share the same metrics instance from `ScreenComposer`:

```rust
pub struct ScreenComposer<BackendData: Backend + 'static> {
    // ... other fields ...
    pub render_metrics: Arc<crate::render_metrics::RenderMetrics>,
}
```

**Initialized in:** `ScreenComposer::init()` (line ~561)
```rust
render_metrics: Arc::new(crate::render_metrics::RenderMetrics::new()),
```

---

## How It Works

### 1. Shared Metrics Instance
- Created once in `ScreenComposer::init()`
- Stored as `Arc<RenderMetrics>` (thread-safe, reference-counted)
- Both backends access the same instance

### 2. Winit Path
```
Event Loop
  → state.render_metrics.start_frame() [timing starts]
  → render_output() [actual rendering]
  → state.render_metrics.record_damage() [damage tracked]
  → [timer drops, duration recorded]
  → state.render_metrics.maybe_log_stats() [periodic logging]
```

### 3. Udev Path
```
Event Loop
  → render_surface()
    → surface.render_metrics.start_frame() [timing starts]
    → compositor.render_frame() [actual rendering]
    → metrics.record_damage() [damage tracked]
    → [timer drops, duration recorded]
  → state.render_metrics.maybe_log_stats() [periodic logging]
```

### 4. RAII Timer Pattern
Both backends use the same automatic timing pattern:
```rust
let _timer = metrics.start_frame();  // Timer starts
// ... rendering happens ...
// Timer automatically records duration when dropped at end of scope
```

---

## Testing Both Backends

### Test Winit (Development Mode)
```bash
RUST_LOG=info cargo run --release -- --winit
```

**Expected output:**
```
RENDER METRICS: 300 frames, avg 2.45ms/frame, damage 15.2% (...)
```

### Test Udev (Production Mode)
```bash
# On bare metal or with proper DRM access
RUST_LOG=info cargo run --release -- --tty-udev
```

**Expected output:**
```
RENDER METRICS: 300 frames, avg 2.45ms/frame, damage 15.2% (...)
```

---

## Metrics Output Format

Both backends produce identical log format:

```
RENDER METRICS: <frames> frames, avg <time>ms/frame, damage <pct>% (<damaged>/<total> px), avg <rects> rects/frame
```

**Example:**
```
RENDER METRICS: 300 frames, avg 2.45ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
```

**Meaning:**
- **300 frames** - Rendered in last 5 seconds (~60 FPS)
- **2.45ms** - Average time per frame
- **15.2%** - Percentage of screen changed
- **1234567/8294400 px** - Damaged pixels / total pixels
- **3.2 rects** - Average damage rectangles per frame

---

## Why Both Backends?

### Winit Benefits
- ✅ Easier to test during development
- ✅ No special permissions needed
- ✅ Can run on any Linux desktop
- ✅ Good for iterative development

### Udev Benefits
- ✅ Real production performance
- ✅ Tests actual DRM/KMS code paths
- ✅ Direct hardware acceleration
- ✅ Production deployment scenario

### Having Both
- ✅ Develop and test quickly with winit
- ✅ Validate on real hardware with udev
- ✅ Ensure optimizations work in both modes
- ✅ Catch backend-specific issues

---

## Summary

✅ **Metrics are fully integrated in BOTH backends**  
✅ **Same measurement code in both paths**  
✅ **Unified logging format**  
✅ **Ready for baseline measurement and iteration**

You can now:
1. Develop with winit: `cargo run --release -- --winit`
2. Test on hardware with udev: `cargo run --release -- --tty-udev`
3. Get consistent metrics from both
4. Measure improvements in real-world scenarios
