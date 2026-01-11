# Rendering Performance Metrics - Implementation Summary

## What Was Added

A comprehensive rendering performance measurement system has been integrated into ScreenComposer to enable systematic benchmarking before and after implementing improved rendering techniques.

## Key Components

### 1. Metrics Module (`src/render_metrics.rs`)
- **Frame timing** - Measures render time per frame using RAII timers
- **Damage tracking** - Records damaged pixels vs total pixels
- **Automatic logging** - Logs stats every 5 seconds
- **Thread-safe** - Uses atomic counters for lock-free updates

### 2. Integration Points
- **ScreenComposer state** - Metrics accessible from main compositor state
- **Winit backend** - Captures timing and damage in windowed mode ✅
- **Udev backend** - Captures timing and damage in DRM/KMS mode ✅
- **Unified logging** - Both backends use same metrics instance

### 3. Measurement Script
- `scripts/measure_baseline.sh` - Automated 30-second baseline capture
- Logs to timestamped files for comparison
- Extracts summary automatically

### 4. Documentation
- `docs/measuring-rendering-performance.md` - Complete usage guide
- Explains metrics interpretation
- Provides before/after comparison workflow

## Usage

### Quick Measurement

**Winit backend (default):**
```bash
./scripts/measure_baseline.sh
# or explicitly
./scripts/measure_baseline.sh winit
```

**Udev backend (requires DRM access):**
```bash
./scripts/measure_baseline.sh udev
# May need sudo on some systems
```

### Manual Testing

**Winit mode (windowed, easier for development):**
```bash
RUST_LOG=info cargo run --release -- --winit
```

**Udev mode (bare metal/DRM, production):**
```bash
RUST_LOG=info cargo run --release -- --tty-udev
```

Both modes produce the same metrics output format with backend identification.

### Example Output
```
RENDER METRICS [winit]: 300 frames, avg 2.45ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
```

or

```
RENDER METRICS [udev]: 300 frames, avg 2.45ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
```

## Metrics Explained

| Metric | Meaning | Good Value |
|--------|---------|------------|
| Frames | Total frames rendered | ~60/sec for 60Hz |
| Avg time | Milliseconds per frame | < 10ms |
| Damage % | Percentage of screen changed | Varies by workload |
| Rects | Number of damaged regions | Lower is simpler |

## Next Steps

### 1. Establish Baseline
Run measurements with current implementation:
```bash
./scripts/measure_baseline.sh
# Save output for comparison
```

### 2. Implement Partial Rendering
Follow the plan in `docs/plans/partial_rendering_quick_start.md`:
- Modify `src/skia_renderer.rs`
- Use per-rect clipping in `clear()` and `render_texture_from_to()`

### 3. Measure Improvement
Run the same measurement:
```bash
./scripts/measure_baseline.sh
# Compare with baseline
```

### 4. Expected Results
Based on docs, partial rendering should show:
- **50-70% faster** rendering (8ms → 2-4ms typical)
- **95%+ faster** for cursor-only movement (8ms → 0.5ms)
- Same damage ratio (confirms correct implementation)

## Technical Details

### Frame Timing
Uses RAII pattern - timer starts when `start_frame()` is called and records duration on drop:
```rust
let _timer = state.render_metrics.start_frame();
// ... render code ...
// Timer automatically records when dropped
```

### Damage Recording
Captures output size and damage rectangles:
```rust
state.render_metrics.record_damage(
    (width, height),
    &damage_rectangles
);
```

### Automatic Logging
Logs every 5 seconds automatically, no manual intervention needed. Can force immediate log:
```rust
state.render_metrics.maybe_log_stats(true);  // Force log
```

## Files Modified

- `src/render_metrics.rs` - **New** metrics module
- `src/lib.rs` - Module declaration
- `src/state/mod.rs` - Add metrics to ScreenComposer state (shared by both backends)
- `src/winit.rs` - **Integrate** timing and damage capture (winit backend)
- `src/udev.rs` - **Integrate** timing and damage capture (udev backend)
- `scripts/measure_baseline.sh` - New measurement script
- `scripts/compare_metrics.sh` - New comparison script
- `docs/measuring-rendering-performance.md` - Usage documentation
- `docs/metrics-backend-integration.md` - Backend integration details

## Benefits

1. **Systematic measurement** - Objective, repeatable benchmarks
2. **Before/after comparison** - Verify optimization effectiveness
3. **Continuous monitoring** - Track performance over time
4. **Low overhead** - Uses atomics, minimal impact on performance
5. **Automatic logging** - No manual data collection needed

## Example Workflow

```bash
# 1. Measure current performance (winit for development)
./scripts/measure_baseline.sh winit > before_winit.log

# Or on production hardware (udev)
./scripts/measure_baseline.sh udev > before_udev.log

# 2. Implement partial rendering improvements
# ... edit src/skia_renderer.rs ...

# 3. Rebuild and measure again
./scripts/measure_baseline.sh winit > after_winit.log
# or
./scripts/measure_baseline.sh udev > after_udev.log

# 4. Compare
./scripts/compare_metrics.sh before_winit.log after_winit.log

# 5. Calculate improvement
# Example: 8.32ms → 2.15ms = 74% faster
```

## References

- `docs/plans/partial_rendering_quick_start.md` - Implementation guide
- `docs/plans/skia_partial_rendering_plan.md` - Detailed technical plan
- `docs/plans/damage_audit_report.md` - Damage tracking analysis
- `docs/rendering.md` - Rendering pipeline overview
- `docs/render_loop.md` - Render loop architecture

