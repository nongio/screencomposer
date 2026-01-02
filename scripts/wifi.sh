#!/bin/bash

if ! declare -f log_info >/dev/null 2>&1; then
    log_info() { printf '[INFO] %s\n' "$*"; }
fi
if ! declare -f log_warn >/dev/null 2>&1; then
    log_warn() { printf '[WARN] %s\n' "$*"; }
fi

wifi_list() {
    local recent
    recent=$(nmcli -t -f NAME,TYPE,TIMESTAMP connection show 2>/dev/null | \
        awk -F: 'tolower($2) ~ /(wifi|802-11-wireless)/ && $3>0 {print $3 ":" $1}' | \
        sort -nr | cut -d: -f2-)
    if [ -n "$recent" ]; then
        printf '%s\n' "$recent"
        return 0
    fi
    nmcli -t -f NAME,TYPE connection show 2>/dev/null | \
        awk -F: 'tolower($2) ~ /(wifi|802-11-wireless)/ {print $1}'
}

wifi_connect() {
    local name="$1"
    if [ -z "$name" ]; then
        log_warn "wifi_connect requires a connection name"
        return 1
    fi
    log_info "Ensuring Wi-Fi autoconnect for '$name'"
    nmcli con modify "$name" connection.autoconnect yes || \
        log_warn "Failed to set autoconnect for '$name'"
    local output
    if output=$(nmcli con up "$name" 2>&1); then
        return 0
    fi
    printf '%s\n' "$output" >&2
    log_warn "Failed to bring up '$name'"
    if printf '%s\n' "$output" | grep -qiE 'secrets were required|password .* not given|no secrets'; then
        if command -v busctl >/dev/null 2>&1; then
            if ! busctl --user list 2>/dev/null | grep -qE 'org\.kde\.kwalletd6|org\.kde\.kwalletd5|org\.freedesktop\.secrets'; then
                log_warn "No secrets agent detected on D-Bus; wallet may be unavailable"
            fi
        fi
        if [ -t 0 ]; then
            log_info "Retrying with interactive prompt (--ask)"
            nmcli --ask con up "$name" || \
                log_warn "Failed to bring up '$name' with --ask"
        else
            log_warn "No TTY available for --ask; secrets agent may be missing"
        fi
    fi
}

wifi_autoconnect() {
    # Optional Wi-Fi auto-connect via NetworkManager (uses most recently used Wi-Fi).
    if command -v nmcli >/dev/null 2>&1; then
        if systemctl is-active --quiet NetworkManager 2>/dev/null; then
            last_wifi=$(wifi_list | head -n 1)
            if [ -n "$last_wifi" ]; then
                wifi_connect "$last_wifi"
            else
                log_warn "No Wi-Fi connection found to auto-connect"
            fi
        else
            log_warn "NetworkManager not active; Wi-Fi auto-connect skipped"
        fi
    fi
}
