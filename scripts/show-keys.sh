#!/bin/bash
# Show keyboard input in real-time with clean formatting
# Useful for testing XKB remapping and keyboard shortcuts

echo "=== Otto Keyboard Event Viewer ==="
echo
echo "This tool shows you what keys you're pressing in real-time."
echo "Useful for:"
echo "  - Verifying XKB remapping is working (e.g., Caps Lock → Escape)"
echo "  - Testing keyboard shortcuts"
echo "  - Understanding modifier key behavior"
echo
echo "Press Ctrl+C to exit"
echo
echo "---"
echo

# Check which tool to use
if command -v wev &> /dev/null; then
    echo "Using 'wev' (Wayland event viewer)"
    echo
    
    # Parse wev output for clean key display
    current_mods=""
    last_keysym=""
    last_state=""
    
    # Use unbuffered output with stdbuf
    WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} stdbuf -oL wev 2>/dev/null | while IFS= read -r line; do
        # Capture current modifiers from depressed line
        # Format: "depressed: 00000009: Shift Mod1"
        if echo "$line" | grep -q "depressed:"; then
            # Extract everything after the second colon (modifier names)
            mods=$(echo "$line" | sed -n 's/.*depressed: [0-9a-fA-F]*: \(.*\)/\1/p' | xargs)
            
            # Convert common modifier names to friendly format
            mods=$(echo "$mods" | sed 's/Shift/Shift/g')
            mods=$(echo "$mods" | sed 's/Control/Ctrl/g')
            mods=$(echo "$mods" | sed 's/Mod1/Alt/g')
            mods=$(echo "$mods" | sed 's/Mod4/Logo/g')
            mods=$(echo "$mods" | sed 's/Lock/CapsLock/g')
            mods=$(echo "$mods" | sed 's/ /+/g')  # Replace spaces with +
            
            current_mods="$mods"
        fi
        
        # Capture key press/release
        if echo "$line" | grep -q "key: "; then
            if echo "$line" | grep -q "state: 1 (pressed)"; then
                last_state="PRESS"
            elif echo "$line" | grep -q "state: 0 (released)"; then
                last_state="RELEASE"
            fi
        fi
        
        # Show the keysym when we get it
        if echo "$line" | grep -q "sym: "; then
            # Extract keysym name and UTF-8 character
            # Format: "sym: F            (70), utf8: 'F'"
            keysym=$(echo "$line" | sed -n "s/.*sym: \([^ ]*\).*/\1/p")
            utf8=$(echo "$line" | sed -n "s/.*utf8: '\([^']*\)'.*/\1/p")
            
            if [ ! -z "$keysym" ] && [ "$last_state" = "PRESS" ]; then
                # Build the display string
                if [ ! -z "$current_mods" ]; then
                    display="${current_mods}+${keysym}"
                else
                    display="${keysym}"
                fi
                
                # Add UTF-8 character if printable and flush immediately
                if [ ! -z "$utf8" ]; then
                    printf "  %-35s (prints: '%s')\n" "$display" "$utf8"
                else
                    printf "  %-35s\n" "$display"
                fi
                
                # Force flush
                fflush stdout 2>/dev/null || true
            fi
            
            last_state=""
        fi
    done

elif command -v xkbcli &> /dev/null; then
    echo "Using 'xkbcli interactive-wayland'"
    echo "Shows: KEY_STATE keysym [modifiers]"
    echo
    
    WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} stdbuf -oL xkbcli interactive-wayland --uniline 2>/dev/null | while IFS= read -r line; do
        # Parse xkbcli output format
        # Example: "+71 space [ ]"
        if echo "$line" | grep -qE "^[+-][0-9]"; then
            state=$(echo "$line" | cut -c1)
            keysym=$(echo "$line" | awk '{print $2}')
            mods=$(echo "$line" | sed 's/.*\[\(.*\)\]/\1/')
            
            if [ "$state" = "+" ]; then  # Only show key press, not release
                if [ ! -z "$mods" ] && [ "$mods" != " " ]; then
                    # Convert modifier format: "C" = Ctrl, "A" = Alt, "S" = Shift, "4" = Logo
                    display_mods=$(echo "$mods" | sed 's/C/Ctrl+/g; s/A/Alt+/g; s/S/Shift+/g; s/4/Logo+/g')
                    printf "  %-35s\n" "${display_mods}${keysym}"
                else
                    printf "  %-35s\n" "$keysym"
                fi
            fi
        fi
    done

else
    echo "ERROR: No keyboard event viewer found!"
    echo
    echo "Please install one of the following:"
    echo "  - wev (recommended): sudo apt install wev"
    echo "  - xkbcli: sudo apt install libxkbcommon-tools"
    echo
    exit 1
fi
