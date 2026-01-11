#!/bin/bash
# Baseline rendering performance measurement script
# Run ScreenComposer with metrics logging enabled

set -e

# Parse backend argument
BACKEND="${1:-winit}"

if [ "$BACKEND" != "winit" ] && [ "$BACKEND" != "udev" ]; then
    echo "Usage: $0 [winit|udev]"
    echo ""
    echo "Examples:"
    echo "  $0          # Use winit (default)"
    echo "  $0 winit    # Use winit backend"
    echo "  $0 udev     # Use udev backend (requires DRM access)"
    exit 1
fi

echo "======================================"
echo "ScreenComposer Rendering Baseline Test"
echo "Backend: $BACKEND"
echo "======================================"
echo ""

if [ "$BACKEND" = "winit" ]; then
    echo "This script will run ScreenComposer in winit mode for 30 seconds"
    echo "and measure rendering performance metrics."
    echo ""
    echo "Instructions:"
    echo "1. The compositor will start in a window"
    echo "2. Perform typical desktop tasks:"
    echo "   - Move the cursor around"
    echo "   - Open/close some windows (if you have apps)"
    echo "   - Drag windows"
    echo "   - Switch workspaces"
    echo "3. The test will automatically stop after 30 seconds"
else
    echo "This script will run ScreenComposer in udev mode for 30 seconds"
    echo "and measure rendering performance metrics."
    echo ""
    echo "⚠️  IMPORTANT: This requires DRM/KMS access (usually needs sudo or seat session)"
    echo ""
    echo "Instructions:"
    echo "1. The compositor will take over your display"
    echo "2. Perform typical desktop tasks:"
    echo "   - Move the cursor around"
    echo "   - Switch workspaces (if configured)"
    echo "3. The test will automatically stop after 30 seconds"
    echo "4. Your display will return to normal"
fi

echo ""
echo "Press Enter to start..."
read

# Build release version
echo "Building release version..."
cargo build --release

# Create log file with backend name
LOG_FILE="baseline_metrics_${BACKEND}_$(date +%Y%m%d_%H%M%S).log"

echo "Starting ScreenComposer ($BACKEND backend)..."
echo "Logging to: $LOG_FILE"
echo ""

# Determine command based on backend
if [ "$BACKEND" = "winit" ]; then
    CMD="cargo run --release -- --winit"
else
    # Udev might need sudo
    if [ "$EUID" -ne 0 ] && [ -z "$XDG_SESSION_ID" ]; then
        echo "⚠️  Warning: udev backend may need sudo or a seat session"
        echo "Running with sudo..."
        CMD="sudo -E env RUST_LOG=info cargo run --release -- --tty-udev"
    else
        CMD="cargo run --release -- --tty-udev"
    fi
fi

# Run for 30 seconds with metrics logging
# Add backend marker to the log
echo "=== BACKEND: $BACKEND ===" | tee "$LOG_FILE"
timeout 30s env RUST_LOG=info $CMD 2>&1 | tee -a "$LOG_FILE" || true

echo ""
echo "======================================"
echo "Test Complete!"
echo "======================================"
echo ""
echo "Backend: $BACKEND"
echo "Log saved to: $LOG_FILE"
echo ""
echo "Summary of rendering metrics:"
grep "RENDER METRICS:" "$LOG_FILE" || echo "No metrics found in log"
echo ""
echo "To analyze further, search for 'RENDER METRICS:' in the log file."
