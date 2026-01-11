# Metrics Quick Reference

## Running Measurements

```bash
# Winit (development/windowed)
./scripts/measure_baseline.sh winit

# Udev (production/DRM - may need sudo)
./scripts/measure_baseline.sh udev

# Default (winit)
./scripts/measure_baseline.sh
```

## Comparing Results

```bash
./scripts/compare_metrics.sh before.log after.log
```

## Manual Testing

```bash
# Winit
RUST_LOG=info cargo run --release -- --winit

# Udev
RUST_LOG=info cargo run --release -- --tty-udev

# X11
RUST_LOG=info cargo run --release -- --x11
```

## Output Format

```
RENDER METRICS [backend]: N frames, avg X.XXms/frame, damage Y.Y% (damaged/total px), avg Z.Z rects/frame
```

**Examples:**
```
RENDER METRICS [winit]: 300 frames, avg 2.45ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
RENDER METRICS [udev]: 300 frames, avg 2.15ms/frame, damage 15.2% (1234567/8294400 px), avg 3.2 rects/frame
```

## Backend Differences

| Backend | Use Case | Access | Performance |
|---------|----------|--------|-------------|
| **winit** | Development, testing | No special permissions | Good for iteration |
| **udev** | Production, real hardware | Needs DRM/seat access | True production perf |
| **x11** | X11 environments | No special permissions | X11 client mode |

## Metrics Interpretation

### Render Time
- **< 5ms** - Excellent
- **5-10ms** - Good
- **10-16ms** - Acceptable
- **> 16ms** - Will drop frames

### Damage Ratio
- **< 10%** - Cursor only
- **10-30%** - Normal desktop
- **30-60%** - Active windows
- **> 60%** - Fullscreen video

## Common Workflows

### Baseline Before Optimization
```bash
./scripts/measure_baseline.sh winit
# Note the log filename shown
```

### Test After Changes
```bash
./scripts/measure_baseline.sh winit
# Note the new log filename
```

### Compare
```bash
./scripts/compare_metrics.sh \
  baseline_metrics_winit_20260111_120000.log \
  baseline_metrics_winit_20260111_130000.log
```

### Cross-Backend Comparison
```bash
# Measure in winit
./scripts/measure_baseline.sh winit

# Measure in udev
./scripts/measure_baseline.sh udev

# Compare (script will warn about different backends)
./scripts/compare_metrics.sh \
  baseline_metrics_winit_*.log \
  baseline_metrics_udev_*.log
```

## Log Files

**Location:** Current directory  
**Format:** `baseline_metrics_{backend}_{YYYYMMDD_HHMMSS}.log`  
**Example:** `baseline_metrics_winit_20260111_143022.log`

## Troubleshooting

**No metrics showing:**
- Check RUST_LOG=info is set
- Ensure compositor is rendering (move cursor)

**Udev permission denied:**
- Run with sudo: `sudo ./scripts/measure_baseline.sh udev`
- Or ensure you have DRM/seat access

**Different backends in comparison:**
- Script will warn you
- Compare same backend for valid results
