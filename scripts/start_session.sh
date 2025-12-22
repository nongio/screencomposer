#!/bin/bash
# ScreenComposer Session Startup Script
# Sets up D-Bus, environment variables, and necessary services

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# D-Bus session setup
# When running on a TTY, we need to either connect to an existing session
# or create a new one and persist it for other processes
DBUS_ENV_FILE="$XDG_RUNTIME_DIR/dbus-session"

if [ -n "$DBUS_SESSION_BUS_ADDRESS" ]; then
    log_info "D-Bus session already set: $DBUS_SESSION_BUS_ADDRESS"
elif [ -f "$DBUS_ENV_FILE" ]; then
    log_info "Loading D-Bus session from $DBUS_ENV_FILE"
    source "$DBUS_ENV_FILE"
    export DBUS_SESSION_BUS_ADDRESS
    log_info "D-Bus session loaded: $DBUS_SESSION_BUS_ADDRESS"
else
    log_info "Starting new D-Bus session"
    # Start D-Bus session and export the address
    if ! eval $(dbus-launch --sh-syntax); then
        log_error "Failed to start D-Bus session"
        exit 1
    fi
    
    # Save D-Bus address for other processes on this TTY
    echo "export DBUS_SESSION_BUS_ADDRESS='$DBUS_SESSION_BUS_ADDRESS'" > "$DBUS_ENV_FILE"
    echo "export DBUS_SESSION_BUS_PID='$DBUS_SESSION_BUS_PID'" >> "$DBUS_ENV_FILE"
    chmod 600 "$DBUS_ENV_FILE"
    
    log_info "D-Bus session started: $DBUS_SESSION_BUS_ADDRESS"
    log_info "D-Bus environment saved to $DBUS_ENV_FILE"
    log_info "Run 'source $DBUS_ENV_FILE' in other terminals to connect"
fi

# Export essential environment variables
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export XDG_SESSION_TYPE="wayland"
export XDG_SESSION_CLASS="user"
export XDG_CURRENT_DESKTOP="screencomposer"

# Wayland display will be set by compositor
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"

log_info "Environment setup:"
log_info "  XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
log_info "  DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS"
log_info "  XDG_SESSION_TYPE=$XDG_SESSION_TYPE"
log_info ""
log_info "To run commands in this session from another terminal:"
log_info "  source $DBUS_ENV_FILE"

# Create runtime directory if it doesn't exist
if [ ! -d "$XDG_RUNTIME_DIR" ]; then
    log_warn "XDG_RUNTIME_DIR doesn't exist, creating it"
    mkdir -p "$XDG_RUNTIME_DIR"
    chmod 700 "$XDG_RUNTIME_DIR"
fi

# Ensure KDE Wallet service is running
# On Arch, kwallet may be started on-demand via D-Bus activation
if systemctl --user is-active --quiet kwallet5.service 2>/dev/null; then
    log_info "kwallet5 service is active"
elif systemctl --user is-active --quiet org.kde.kwalletd5.service 2>/dev/null; then
    log_info "org.kde.kwalletd5 service is active"
elif systemctl --user list-unit-files | grep -qE 'kwallet5|kwalletd5'; then
    log_info "Starting kwallet service via systemctl"
    systemctl --user start kwallet5.service 2>/dev/null || \
    systemctl --user start org.kde.kwalletd5.service 2>/dev/null || \
    log_warn "Failed to start kwallet service"
else
    # On Arch, kwallet is often D-Bus activated, not a systemd service
    log_info "kwallet not found as systemd service (will be D-Bus activated on demand)"
fi

# Ensure portal backend is built
if [ ! -f "target/release/xdg-desktop-portal-screencomposer" ]; then
    log_error "Portal backend not built in release mode!"
    log_info "Please run: cargo build -p xdg-desktop-portal-screencomposer --release"
    exit 1
fi

# Start portal backend in background
log_info "Starting xdg-desktop-portal-screencomposer"
PORTAL_LOG="$PWD/components/xdg-desktop-portal-sc/portal.log"
mkdir -p "$(dirname "$PORTAL_LOG")"

# Kill existing portal if running
pkill -f xdg-desktop-portal-screencomposer || true
sleep 0.5

# Start portal backend
RUST_LOG=info target/release/xdg-desktop-portal-screencomposer > "$PORTAL_LOG" 2>&1 &
PORTAL_PID=$!
log_info "Portal backend started (PID: $PORTAL_PID, log: $PORTAL_LOG)"

# Wait a moment for portal to register
sleep 1

# Verify portal is running
if ! busctl --user list | grep -q "org.freedesktop.impl.portal.desktop.screencomposer"; then
    log_error "Portal backend failed to start!"
    cat "$PORTAL_LOG"
    exit 1
fi
log_info "Portal backend registered on D-Bus"

# Ensure PipeWire services are running
# On Arch, PipeWire is typically managed via systemd user services
for service in pipewire.service pipewire-pulse.service wireplumber.service; do
    if systemctl --user is-active --quiet "$service" 2>/dev/null; then
        log_info "$service is active"
    elif systemctl --user list-unit-files | grep -q "^$service"; then
        log_info "Starting $service via systemctl"
        systemctl --user start "$service" || log_warn "Failed to start $service"
    else
        log_warn "$service not found in systemd user services"
    fi
done

# Wait a moment for services to initialize
sleep 1

# Verify PipeWire is running
if ! pgrep -x pipewire > /dev/null; then
    log_error "PipeWire not running - screenshare will not work!"
    log_info "Install pipewire and enable user services:"
    log_info "  systemctl --user enable --now pipewire.service pipewire-pulse.service wireplumber.service"
else
    log_info "PipeWire is running"
fi

# Ensure compositor is built in release mode
if [ ! -f "target/release/screen-composer" ]; then
    log_error "Compositor not built in release mode!"
    log_info "Please run: cargo build --release"
    exit 1
fi

# Start the compositor (udev backend only)
log_info "Starting ScreenComposer compositor (udev backend)"
COMPOSITOR_LOG="$PWD/screencomposer.log"

if [ "$EUID" -ne 0 ] && [ -z "$LIBSEAT_BACKEND" ]; then
    log_warn "Running DRM backend without root - you may need libseat or run with sudo"
fi

RUST_LOG=info target/release/screen-composer --tty-udev 2>&1 | tee "$COMPOSITOR_LOG"

RUST_LOG=info cargo run --release -- --tty-udev 2>&1 | tee "$COMPOSITOR_LOG"    ;;
esac


# Clean up D-Bus session file
if [ -f "$DBUS_ENV_FILE" ]; then
    rm -f "$DBUS_ENV_FILE"
    log_info "Cleaned up D-Bus session file"
fi

# Stop D-Bus session if we started it
if [ -n "$DBUS_SESSION_BUS_PID" ]; then
    kill $DBUS_SESSION_BUS_PID 2>/dev/null || true
    log_info "Stopped D-Bus session"
fi

# Cleanup on exit
log_info "Compositor exited, cleaning up..."
kill $PORTAL_PID 2>/dev/null || true
log_info "Session ended"
