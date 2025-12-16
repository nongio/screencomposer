#!/bin/bash
# Script to run ScreenComposer with a test application
# Usage: ./test-screenshare.sh [program] [args...]
# Example: ./test-screenshare.sh obs --safe-mode
#          ./test-screenshare.sh geeqie
#          ./test-screenshare.sh firefox

PROGRAM="${1:-obs}"
shift || true
PROGRAM_ARGS="$@"

# Determine program-specific arguments
case "$PROGRAM" in
    obs)
        DEFAULT_ARGS="--safe-mode --disable-missing-files-check --verbose"
        ;;
    geeqie)
        DEFAULT_ARGS=""
        ;;
    firefox)
        DEFAULT_ARGS="-new-instance"
        ;;
    *)
        DEFAULT_ARGS=""
        ;;
esac

# Use provided args or defaults
if [ -z "$PROGRAM_ARGS" ]; then
    PROGRAM_ARGS="$DEFAULT_ARGS"
fi

cd /home/riccardo/dev/screen-composer-run2

# Stop any existing instances
pkill -9 screen-composer 2>/dev/null
pkill -9 "$PROGRAM" 2>/dev/null

# Wait a moment for cleanup
sleep 1

# Start the compositor in background
RUST_LOG=debug cargo run --release -- --tty-udev < /dev/tty5 > screencomposer.log 2>&1 &
COMPOSITOR_PID=$!
echo "Compositor started (PID: $COMPOSITOR_PID)"
echo "Waiting for compositor to be ready..."
sleep 2

echo "Starting $PROGRAM..."
WAYLAND_DISPLAY=wayland-1 QT_QPA_PLATFORM=wayland $PROGRAM $PROGRAM_ARGS > "${PROGRAM}.log" 2>&1 &
PROGRAM_PID=$!
echo "$PROGRAM started (PID: $PROGRAM_PID)"
echo "Waiting 5 seconds..."
sleep 5

echo "Closing $PROGRAM..."
pkill -INT "$PROGRAM"
sleep 1
pkill -TERM "$PROGRAM" 2>/dev/null
sleep 1
pkill -9 "$PROGRAM" 2>/dev/null
wait $PROGRAM_PID 2>/dev/null

echo "Closing compositor..."
pkill -TERM screen-composer
sleep 1
pkill -9 screen-composer 2>/dev/null
wait $COMPOSITOR_PID 2>/dev/null

echo ""
echo "Test complete! Check logs:"
echo "  - screencomposer.log"
echo "  - ${PROGRAM}.log"
