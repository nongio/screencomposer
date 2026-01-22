## Keyboard Configuration

Otto uses XKB (X Keyboard Extension) for keyboard layout and remapping configuration.

Keyboard configuration is handled through the `[input]` section of `otto_config.toml` using three XKB parameters:
- `xkb_layout` - Keyboard layout (e.g., "us", "dvorak", "de")
- `xkb_variant` - Layout variant (e.g., "dvorak", "intl")
- `xkb_options` - Array of XKB options for remapping and behavior

The configuration is compiled into the XKB keymap at compositor startup, providing:
- **Performance** - No per-keystroke overhead (compiled into keymap)
- **Standards compliance** - Same options as other compositors
- **Rich feature set** - Access to all XKB's built-in options
- **Simple implementation** - Leverages XKB's well-tested code

## Configuration

Add to `otto_config.toml` under the `[input]` section:

```toml
[input]
xkb_layout = "us"
xkb_variant = "dvorak"  # optional
xkb_options = [
    "caps:escape",        # Caps Lock → Escape (Vim users)
    "ctrl:swapcaps",      # Swap Ctrl and Caps Lock
    "altwin:ctrl_win",    # Win/Super key acts as Ctrl (Mac-like)
    "compose:ralt",       # Right Alt as Compose key for accents
]
```

### Multiple Layouts

```toml
[input]
xkb_layout = "us,it"              # US, Italian
xkb_variant = "dvorak,,"              # Dvorak for US, default for others
xkb_options = [
    "grp:win_space_toggle",          # Win+Space to switch layouts
    "grp_led:caps",                  # Use Caps Lock LED as indicator
    "caps:escape",                   # Caps Lock as Escape in all layouts
]
```

## Common XKB Options

### Caps Lock Remapping
- `caps:escape` - Caps Lock → Escape
- `caps:ctrl_modifier` - Caps Lock → additional Ctrl
- `ctrl:nocaps` - Caps Lock → Ctrl (disables Caps Lock function)
- `ctrl:swapcaps` - Swap Ctrl and Caps Lock
- `caps:swapescape` - Swap Caps Lock and Escape
- `caps:none` - Disable Caps Lock completely

### Control Key
- `ctrl:nocaps` - Caps Lock as Ctrl
- `ctrl:swapcaps` - Swap Ctrl and Caps Lock
- `ctrl:aa_ctrl` - Both Alt keys act as Ctrl

### Alt/Win Keys
- `altwin:swap_alt_win` - Swap Alt and Win/Super
- `altwin:ctrl_win` - Win/Super acts as Ctrl (Mac-like)
- `altwin:meta_win` - Win/Super as Meta

### Compose Key (for accents/symbols)
- `compose:ralt` - Right Alt as Compose
- `compose:menu` - Menu key as Compose
- `compose:caps` - Caps Lock as Compose

### Layout Switching
- `grp:win_space_toggle` - Win+Space to switch
- `grp:alt_shift_toggle` - Alt+Shift to switch
- `grp:caps_toggle` - Caps Lock to switch
- `grp_led:caps` - Use Caps Lock LED as indicator

## Finding Available Options

```bash
# List all available XKB options
xkbcli list

# View current system configuration
localectl status

# Read the manual
man xkeyboard-config

# Browse all options
cat /usr/share/X11/xkb/rules/base.lst
```

## Debugging Tools

Otto provides scripts for testing and verifying keyboard configuration:

### Real-time Event Viewer
```bash
./scripts/show-keys.sh
```
Shows what keys you're pressing in real-time. Useful for:
- Verifying XKB remapping works (e.g., Caps Lock → Escape)
- Testing keyboard shortcuts
- Understanding modifier behavior

### Configuration Inspector
```bash
./scripts/check-xkb-config.sh
```
Shows:
- Current system XKB configuration
- Active compositor keymap (if Otto is running)
- Available tools and example configurations

### Debugging Shortcuts

When running Otto with `RUST_LOG=debug`, keyboard shortcut processing logs:
- Keysym name and hex value
- Active modifiers (ctrl, alt, shift, logo)
- Which shortcut matched (or why none matched)

Example:
```bash
RUST_LOG=debug cargo run -- --winit 2>&1 | grep "Shortcut check"
```

## Implementation Details

**Location**: `src/state/mod.rs` - keyboard initialization

The XKB configuration is applied during keyboard initialization:

```rust
let xkb_config = XkbConfig {
    layout: &layout,           // from config.input.xkb_layout
    variant: &variant,         // from config.input.xkb_variant
    options,                   // from config.input.xkb_options.join(",")
    ..Default::default()
};
seat.add_keyboard(xkb_config, repeat_delay, repeat_rate)
```

**Shortcut Matching**: `src/input_handler.rs`, `src/config/shortcuts.rs`

Shortcuts use XKB keysym names directly. Special cases:
- `ISO_Left_Tab` - Used for Shift+Tab combinations (XKB behavior)
- Alphanumeric keys are normalized (case-insensitive)

## Important Notes

1. **Restart Required**: Changes to XKB configuration require restarting Otto
2. **Scope**: XKB options apply to all clients connected to Otto
3. **Standard Names**: Use XKB keysym names (e.g., `Return` not `Enter`)
4. **Shift+Tab**: Creates `ISO_Left_Tab` keysym, not `Tab` with Shift modifier

## See Also

- `scripts/README.md` - Detailed tool documentation and examples
- `otto_config.example.toml` - Sample configurations
- `man xkeyboard-config` - Complete XKB options reference
