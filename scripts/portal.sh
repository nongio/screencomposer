#!/bin/bash

portal_setup() {
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
    RUST_LOG=$LOG_LEVEL target/release/xdg-desktop-portal-screencomposer > "$PORTAL_LOG" 2>&1 &
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
}
