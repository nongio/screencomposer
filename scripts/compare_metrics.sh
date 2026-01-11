#!/bin/bash
# Compare two rendering metric logs to show improvement

set -e

if [ $# -ne 2 ]; then
    echo "Usage: $0 <before.log> <after.log>"
    echo ""
    echo "Example:"
    echo "  $0 baseline_winit_before.log baseline_winit_after.log"
    echo "  $0 baseline_udev_before.log baseline_udev_after.log"
    exit 1
fi

BEFORE="$1"
AFTER="$2"

if [ ! -f "$BEFORE" ]; then
    echo "Error: File not found: $BEFORE"
    exit 1
fi

if [ ! -f "$AFTER" ]; then
    echo "Error: File not found: $AFTER"
    exit 1
fi

# Extract backend from log files
BEFORE_BACKEND=$(grep "=== BACKEND:" "$BEFORE" 2>/dev/null | head -1 | awk '{print $3}' || echo "unknown")
AFTER_BACKEND=$(grep "=== BACKEND:" "$AFTER" 2>/dev/null | head -1 | awk '{print $3}' || echo "unknown")

echo "======================================"
echo "Rendering Performance Comparison"
echo "======================================"
echo ""
echo "BEFORE: $BEFORE (backend: $BEFORE_BACKEND)"
echo "AFTER:  $AFTER (backend: $AFTER_BACKEND)"
echo ""

# Warn if backends differ
if [ "$BEFORE_BACKEND" != "$AFTER_BACKEND" ] && [ "$BEFORE_BACKEND" != "unknown" ] && [ "$AFTER_BACKEND" != "unknown" ]; then
    echo "⚠️  WARNING: Different backends detected!"
    echo "   Before: $BEFORE_BACKEND"
    echo "   After:  $AFTER_BACKEND"
    echo "   Comparison may not be meaningful."
    echo ""
fi

# Extract metrics using awk for better parsing
extract_metrics() {
    local file="$1"
    grep "RENDER METRICS:" "$file" | tail -1 | awk '{
        # Extract frames
        match($0, /([0-9]+) frames/, frames_arr)
        frames = frames_arr[1]
        
        # Extract avg time
        match($0, /avg ([0-9.]+)ms/, time_arr)
        avg_time = time_arr[1]
        
        # Extract damage percentage
        match($0, /damage ([0-9.]+)%/, damage_arr)
        damage_pct = damage_arr[1]
        
        # Extract rect count
        match($0, /avg ([0-9.]+) rects/, rect_arr)
        avg_rects = rect_arr[1]
        
        print frames, avg_time, damage_pct, avg_rects
    }'
}

# Get metrics from both files
BEFORE_METRICS=$(extract_metrics "$BEFORE")
AFTER_METRICS=$(extract_metrics "$AFTER")

if [ -z "$BEFORE_METRICS" ]; then
    echo "Error: No metrics found in $BEFORE"
    echo "Make sure the file contains 'RENDER METRICS:' lines"
    exit 1
fi

if [ -z "$AFTER_METRICS" ]; then
    echo "Error: No metrics found in $AFTER"
    echo "Make sure the file contains 'RENDER METRICS:' lines"
    exit 1
fi

# Parse values
read BEFORE_FRAMES BEFORE_TIME BEFORE_DAMAGE BEFORE_RECTS <<< "$BEFORE_METRICS"
read AFTER_FRAMES AFTER_TIME AFTER_DAMAGE AFTER_RECTS <<< "$AFTER_METRICS"

echo "BEFORE (from $BEFORE):"
echo "  Frames:       $BEFORE_FRAMES"
echo "  Avg time:     ${BEFORE_TIME}ms"
echo "  Damage:       ${BEFORE_DAMAGE}%"
echo "  Avg rects:    $BEFORE_RECTS"
echo ""

echo "AFTER (from $AFTER):"
echo "  Frames:       $AFTER_FRAMES"
echo "  Avg time:     ${AFTER_TIME}ms"
echo "  Damage:       ${AFTER_DAMAGE}%"
echo "  Avg rects:    $AFTER_RECTS"
echo ""

# Calculate improvements using awk for floating point
echo "IMPROVEMENTS:"
echo ""

# Time improvement
TIME_IMPROVEMENT=$(awk "BEGIN {
    if ($BEFORE_TIME > 0) {
        improvement = (($BEFORE_TIME - $AFTER_TIME) / $BEFORE_TIME) * 100
        speedup = $BEFORE_TIME / $AFTER_TIME
        printf \"  Render time: %.2fms → %.2fms (%.1f%% faster, %.2fx speedup)\", 
            $BEFORE_TIME, $AFTER_TIME, improvement, speedup
    } else {
        printf \"  Render time: N/A\"
    }
}")
echo "$TIME_IMPROVEMENT"

# Frame rate (if times are valid)
if [ $(echo "$BEFORE_TIME > 0 && $AFTER_TIME > 0" | bc -l) -eq 1 ]; then
    BEFORE_FPS=$(awk "BEGIN {printf \"%.1f\", 1000 / $BEFORE_TIME}")
    AFTER_FPS=$(awk "BEGIN {printf \"%.1f\", 1000 / $AFTER_TIME}")
    echo "  Max FPS:     ${BEFORE_FPS} → ${AFTER_FPS}"
fi

# Damage comparison
DAMAGE_DIFF=$(awk "BEGIN {
    diff = $AFTER_DAMAGE - $BEFORE_DAMAGE
    if (diff > 0) {
        printf \"  Damage:      %.1f%% → %.1f%% (+%.1f%% - expected to be similar)\", 
            $BEFORE_DAMAGE, $AFTER_DAMAGE, diff
    } else if (diff < 0) {
        printf \"  Damage:      %.1f%% → %.1f%% (%.1f%% - expected to be similar)\", 
            $BEFORE_DAMAGE, $AFTER_DAMAGE, diff
    } else {
        printf \"  Damage:      %.1f%% (unchanged ✓)\", $BEFORE_DAMAGE
    }
}")
echo "$DAMAGE_DIFF"

# Rect count comparison
RECT_DIFF=$(awk "BEGIN {
    diff = $AFTER_RECTS - $BEFORE_RECTS
    if (diff > 0.1 || diff < -0.1) {
        printf \"  Avg rects:   %.1f → %.1f (%.1f - expected to be similar)\", 
            $BEFORE_RECTS, $AFTER_RECTS, diff
    } else {
        printf \"  Avg rects:   %.1f (unchanged ✓)\", $BEFORE_RECTS
    }
}")
echo "$RECT_DIFF"

echo ""
echo "======================================"
echo ""

# Interpretation
echo "INTERPRETATION:"
echo ""

TIME_CHANGE=$(awk "BEGIN {
    if ($BEFORE_TIME > 0) {
        print (($BEFORE_TIME - $AFTER_TIME) / $BEFORE_TIME) * 100
    } else {
        print 0
    }
}")

if [ $(echo "$TIME_CHANGE > 10" | bc -l) -eq 1 ]; then
    echo "✅ Significant improvement! Render time reduced by ${TIME_CHANGE%.*}%"
elif [ $(echo "$TIME_CHANGE > 0" | bc -l) -eq 1 ]; then
    echo "✓ Minor improvement. Render time reduced by ${TIME_CHANGE%.*}%"
elif [ $(echo "$TIME_CHANGE < -10" | bc -l) -eq 1 ]; then
    echo "⚠ Performance regression! Render time increased."
else
    echo "≈ No significant change in render time."
fi

DAMAGE_CHANGE=$(awk "BEGIN {print ($AFTER_DAMAGE - $BEFORE_DAMAGE) * ($AFTER_DAMAGE - $BEFORE_DAMAGE)}")
if [ $(echo "$DAMAGE_CHANGE < 1" | bc -l) -eq 1 ]; then
    echo "✓ Damage ratio is consistent (expected for same workload)"
else
    echo "⚠ Damage ratio changed significantly (workloads may differ)"
fi

echo ""
