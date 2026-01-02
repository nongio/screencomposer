#!/bin/bash

pipewire_setup() {
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
}
