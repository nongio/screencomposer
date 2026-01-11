# Rendering Performance Measurement Guide

This guide explains how to measure and analyze ScreenComposer's rendering performance using the built-in metrics system.

## Overview

ScreenComposer includes a comprehensive metrics system that tracks:
- **Frame count** - Total frames rendered
- **Average render time** - Time spent rendering each frame (in milliseconds)
- **Damage ratio** - Percentage of screen that changes per frame
- **Damage rectangles** - Number of separate damaged regions per frame

## Quick Start

### 1. Run Baseline Measurement

To measure current performance:

```bash
# Winit backend (development/windowed mode)
./scripts/measure_baseline.sh winit

# Udev backend (production/DRM mode)
./scripts/measure_baseline.sh udev
```

This script will:
1. Build ScreenComposer in release mode
2. Run it for 30 seconds with the selected backend
3. Log all metrics to a timestamped file (includes backend name)
4. Display a summary

### 2. Perform Typical Tasks

While the compositor is running, perform realistic desktop tasks:
- **Cursor movement** - Move the mouse around
- **Window operations** - Open, close, move, resize windows
- **Workspace switching** - Switch between workspaces
- **App usage** - Use terminal, browser, etc.

### 3. Review Metrics

Metrics are logged every 5 seconds. Example output:

```
RENDER METRICS [winit]: 300 frames, avg 2.45ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
```

or for udev:

```
RENDER METRICS [udev]: 300 frames, avg 2.45ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
```

This means:
- **300 frames** rendered in the last 5 seconds (60 FPS)
- **2.45ms** average time per frame
- **15.2%** of the screen changed on average
- **3.2** separate damage rectangles per frame

## Understanding the Metrics

### Frame Render Time
- **< 5ms** - Excellent (allows plenty of headroom)
- **5-10ms** - Good (60 FPS achievable)
- **10-16ms** - Acceptable (can still hit 60 FPS)
- **> 16ms** - Poor (will drop below 60 FPS)

### Damage Ratio
Lower is better (means less work):
- **< 10%** - Typical for cursor movement only
- **10-30%** - Normal desktop usage
- **30-60%** - Active window movement/animation
- **> 60%** - Fullscreen video or major scene changes

### Damage Rectangles
The number of separate regions that changed:
- **1-5 rects** - Simple updates (cursor, single window)
- **5-20 rects** - Multiple windows updating
- **> 20 rects** - Complex scene with many updates

## Comparing Before/After

To measure improvement from optimization:

1. **Run baseline before changes:**
   ```bash
   ./scripts/measure_baseline.sh winit
   # Saves to: baseline_metrics_winit_YYYYMMDD_HHMMSS.log
   
   # Or for production hardware:
   ./scripts/measure_baseline.sh udev
   # Saves to: baseline_metrics_udev_YYYYMMDD_HHMMSS.log
   ```

2. **Make your changes** (e.g., implement partial rendering)

3. **Run baseline after changes:**
   ```bash
   ./scripts/measure_baseline.sh winit
   # Saves to new timestamped file
   ```

4. **Compare:**
   ```bash
   ./scripts/compare_metrics.sh baseline_metrics_winit_20260111_120000.log baseline_metrics_winit_20260111_130000.log
   ```
   
   The comparison script will:
   - Extract metrics from both logs
   - Show before/after values
   - Calculate improvement percentages
   - Warn if different backends were used

### Example Comparison

**Before partial rendering:**
```
RENDER METRICS [winit]: 300 frames, avg 8.32ms/frame, damage 12.5% (1037000/8294400 px), avg 2.8 rects/frame
```

**After partial rendering:**
```
RENDER METRICS [winit]: 300 frames, avg 2.15ms/frame, damage 12.5% (1037000/8294400 px), avg 2.8 rects/frame
```

**Analysis:**
- Render time improved from 8.32ms → 2.15ms (74% faster!)
- Damage ratio unchanged (expected - same workload)
- Same number of damage rects (confirms damage tracking unchanged)
- **Result:** 3.9x faster rendering for the same work

## Expected Improvements from Partial Rendering

Based on the documentation in `docs/plans/`, implementing true partial rendering should provide:

### Conservative Estimate
- **50-70% GPU time reduction** for typical desktop usage
- **Render time:** 8ms → 2-4ms
- **Power savings:** 30-50% reduction

### Aggressive Estimate (cursor-only movement)
- **95%+ GPU time reduction**
- **Render time:** 8ms → 0.5ms
- **Damage ratio:** ~0.1% (1000px of 2M+ total)

## Manual Testing

For more control, run ScreenComposer manually:

```bash
# Winit - run with info-level logging
RUST_LOG=info cargo run --release -- --winit

# Udev - run with info-level logging (may need sudo)
RUST_LOG=info cargo run --release -- --tty-udev

# Run with debug-level logging (more verbose)
RUST_LOG=debug cargo run --release -- --winit

# Save logs to file
RUST_LOG=info cargo run --release -- --winit 2> render_metrics_winit.log
RUST_LOG=info cargo run --release -- --tty-udev 2> render_metrics_udev.log
```

Watch the console output for periodic metrics updates every 5 seconds. The backend name is included in each log line.

## Programmatic Access

The metrics system is available in code:

```rust
// Access metrics from ScreenComposer state
let snapshot = state.render_metrics.get_stats();

// Print summary
snapshot.print_summary("Current Performance");

// Access individual metrics
println!("Avg render time: {:.2}ms", snapshot.avg_render_time_ms);
println!("Damage ratio: {:.1}%", snapshot.damage_ratio);
```

## Next Steps

After establishing a baseline:

1. **Implement partial rendering** following `docs/plans/partial_rendering_quick_start.md`
2. **Measure again** using the same workload
3. **Compare results** to verify improvement
4. **Iterate** if needed

## Troubleshooting

**No metrics showing:**
- Ensure `RUST_LOG=info` or higher is set
- Check that you're actually rendering (move cursor, open windows)

**Metrics seem wrong:**
- Verify you're running release build (`--release` flag)
- Check that the workload is consistent between runs
- Ensure no background tasks are interfering

**High variance:**
- Normal for interactive workloads
- Use longer measurement periods (60+ seconds)
- Repeat tests multiple times and average results
