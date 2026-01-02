#!/bin/bash

setup_dbus_session() {
    # D-Bus session setup
    # When running on a TTY, we need to either connect to an existing session
    # or create a new one and persist it for other processes
    DBUS_ENV_FILE="$XDG_RUNTIME_DIR/dbus-session"

    if [ -n "$DBUS_SESSION_BUS_ADDRESS" ]; then
        log_info "D-Bus session already set: $DBUS_SESSION_BUS_ADDRESS"
        return 0
    fi
    if [ -f "$DBUS_ENV_FILE" ]; then
        log_info "Loading D-Bus session from $DBUS_ENV_FILE"
        source "$DBUS_ENV_FILE"
        export DBUS_SESSION_BUS_ADDRESS
        log_info "D-Bus session loaded: $DBUS_SESSION_BUS_ADDRESS"
        return 0
    fi

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
}
