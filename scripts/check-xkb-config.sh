#!/bin/bash
# Check current XKB configuration
# This script helps you understand your current keyboard setup

echo "=== Current XKB Configuration ==="
echo

echo "--- System Default (from localectl) ---"
localectl status | grep -E "X11 Layout|X11 Model|X11 Variant|X11 Options|VC Keymap"
echo

if [ -n "$WAYLAND_DISPLAY" ]; then
    echo "--- Active Wayland Keymap (from compositor) ---"
    if command -v xkbcli &> /dev/null; then
        # Dump the current keymap and extract configuration
        KEYMAP=$(xkbcli dump-keymap-wayland 2>/dev/null)
        if [ $? -eq 0 ]; then
            echo "$KEYMAP" | grep -E "xkb_symbols|xkb_types|xkb_compat|xkb_geometry" | head -10
            echo
            echo "Full keymap dumped. To view all details:"
            echo "  xkbcli dump-keymap-wayland"
            echo
            echo "To test interactively:"
            echo "  xkbcli interactive-wayland"
        else
            echo "Could not dump keymap (is Otto running?)"
        fi
    else
        echo "xkbcli not found. Install libxkbcommon-tools package."
    fi
    echo
fi

echo "--- Available XKB Tools ---"
echo "List all options:           xkbcli list"
echo "List layouts:               xkbcli list | grep -A2 'layouts:' | head -50"
echo "List options:               xkbcli list | grep -A2 'options:' | head -50"
echo "View all available options: cat /usr/share/X11/xkb/rules/base.lst"
echo "Manual page:                man xkeyboard-config"
echo "Test keys live:             ./scripts/show-keys.sh"
echo

echo "--- Otto Config Location ---"
echo "Edit your Otto keyboard config in:"
echo "  otto_config.toml (under [input] section)"
echo "  xkb_layout, xkb_variant, xkb_options"
echo

echo "=== Example Configurations ==="
echo
echo "# Caps Lock as Escape (Vim users):"
echo '  xkb_options = ["caps:escape"]'
echo
echo "# Swap Ctrl and Caps Lock:"
echo '  xkb_options = ["ctrl:swapcaps"]'
echo
echo "# Mac-like (Win key as Ctrl):"
echo '  xkb_options = ["altwin:ctrl_win"]'
echo
echo "# Multiple layouts with switching:"
echo '  xkb_layout = "us,ru,fr"'
echo '  xkb_options = ["grp:win_space_toggle", "grp_led:caps"]'
echo
