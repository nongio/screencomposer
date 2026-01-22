# Keyboard Remapping Refactor Plan

## Problem

Otto currently implements custom keysym-level remapping via `[modifier_remap]` and `[key_remap]` config tables. This reinvents functionality already available in XKB's native options system, adding maintenance overhead and running transformation logic on every keystroke.

## Proposed Solution

Replace custom remapping with XKB options support, aligning with how Sway, Hyprland, and other compositors handle keyboard customization.

## Implementation Steps

### 1. Add XKB options to config
**File:** `src/config/mod.rs`

Add to `InputConfig` struct:
```rust
pub struct InputConfig {
    // ... existing fields
    #[serde(default)]
    pub xkb_layout: Option<String>,
    #[serde(default)]
    pub xkb_variant: Option<String>,
    #[serde(default)]
    pub xkb_options: Vec<String>,
}
```

### 2. Update keyboard initialization
**File:** `src/state/mod.rs` (around line 486)

Replace:
```rust
seat.add_keyboard(XkbConfig::default(), k.0, k.1)
```

With:
```rust
let xkb_config = Config::with(|c| XkbConfig {
    layout: c.input.xkb_layout.clone().unwrap_or_default(),
    variant: c.input.xkb_variant.clone().unwrap_or_default(),
    options: c.input.xkb_options.join(","),
    ..Default::default()
});
seat.add_keyboard(xkb_config, k.0, k.1)
```

### 3. Update config examples
**File:** `otto_config.example.toml`

Add section:
```toml
[input]
xkb_layout = "us"
# xkb_variant = "dvorak"
xkb_options = [
    "ctrl:swapcaps",    # Swap Ctrl and Caps Lock
    "caps:escape",      # Make Caps Lock an Escape key
    # "altwin:swap_alt_win",  # Swap Alt and Win keys
]
```

### 4. Remove old implementation
- Remove `modifier_remap` and `key_remap` from `Config` struct
- Remove `rebuild_remap_tables()`, `apply_modifier_remap()` methods
- Remove `ModifierKind` enum and related code
- Remove remap calls from `src/input_handler.rs` and `src/focus.rs`
- Remove `sanitize_remap_tables()` from config loading

### 5. Update documentation
**Files:** `docs/developer/keyboard_mapping.md`, `README.md`

Replace custom remap documentation with XKB options guide. Reference:
- `man xkeyboard-config` for available options
- Common options: `ctrl:*`, `caps:*`, `altwin:*`, `grp:*`

## Migration Notes

**Breaking change:** Users with existing `[modifier_remap]` or `[key_remap]` configs need to migrate to XKB options.

Common migrations:
- `modifier_remap: logo = "ctrl"` → `xkb_options = ["altwin:ctrl_win"]`
- `key_remap: Caps_Lock = "Escape"` → `xkb_options = ["caps:escape"]`
- Swapping Ctrl/Caps → `xkb_options = ["ctrl:swapcaps"]`

## Benefits

1. **Less code** - Remove ~200+ lines of custom remap logic
2. **Better performance** - XKB compiles options into keymap (no per-keystroke overhead)
3. **Standards compliance** - Uses same approach as Sway, Hyprland, wlroots compositors
4. **More options** - Users get access to all XKB's built-in remapping options
5. **Easier maintenance** - Leverage XKB's well-tested implementation

## References

- XKB options list: `/usr/share/X11/xkb/rules/base.lst`
- Smithay XkbConfig: https://docs.rs/smithay/latest/smithay/input/keyboard/struct.XkbConfig.html
- Sway config example: https://github.com/swaywm/sway/wiki#keyboard-layout
