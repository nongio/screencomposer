# Foreign Toplevel Management

ScreenComposer implements dual-protocol support for foreign toplevel management, providing window list information to taskbars, launchers, and window switchers.

## Implemented Protocols

### ext-foreign-toplevel-list-v1 (Newer)
- **Status**: ✅ Fully implemented via Smithay
- **Features**: Window list with title and app_id

### wlr-foreign-toplevel-management-unstable-v1 (Older)
- **Status**: ✅ Basic implementation complete
- **Features**: Window list with title, app_id, and control actions

## Architecture

### Unified Abstraction (`src/state/foreign_toplevel_shared.rs`)
Provides `ForeignToplevelHandles` struct that wraps both protocol handles:
- Single API for window state updates
- Automatic broadcasting to both protocols
- Centralized state management

### Protocol Handlers

**ext-foreign-toplevel-list**: `src/state/foreign_toplevel_list_handler.rs`
- Smithay built-in implementation
- Simple delegate pattern

**wlr-foreign-toplevel-management**: `src/state/wlr_foreign_toplevel.rs`
- Custom implementation
- Manages manager instances and toplevel handles
- Implements Dispatch traits for ScreenComposer

### Integration Points

**Window Creation** (`src/shell/xdg.rs`):
```rust
let ext_handle = self.foreign_toplevel_list_state.new_toplevel(...);
let wlr_handle = self.wlr_foreign_toplevel_state.new_toplevel(...);
let handles = ForeignToplevelHandles::new(ext_handle, wlr_handle);
self.foreign_toplevels.insert(surface_id, handles);
```

**Window Updates** (`src/shell/mod.rs`):
```rust
if title_changed {
    handle.send_title(&title);  // Broadcasts to both protocols
}
```

**Window Destruction**:
```rust
handle.send_closed();  // Notifies both protocols
```

## Current Features

- ✅ Window list enumeration
- ✅ Title updates
- ✅ App ID updates
- ✅ Window closed notifications
- ✅ Automatic state synchronization across protocols

## TODOs

### High Priority
- [ ] Implement window activation (focus) action
  - Handle `Activate` request in wlr protocol
  - Find window by handle and set keyboard focus
  
- [ ] Implement window close action
  - Handle `Close` request in wlr protocol
  - Send close event to window

### Medium Priority
- [ ] Implement minimize/unminimize actions
  - Handle `SetMinimized`/`UnsetMinimized` requests
  - Track minimized state in window
  - Update window visibility
  
- [ ] Implement maximize/unmaximize actions
  - Handle `SetMaximized`/`UnsetMaximized` requests
  - Integrate with existing maximize logic

- [ ] Implement fullscreen actions
  - Handle `SetFullscreen`/`UnsetFullscreen` requests
  - Integrate with existing fullscreen logic

### Low Priority
- [ ] Add window state reporting
  - Send `state` events (activated, maximized, minimized, fullscreen)
  - Track state changes

- [ ] Add output tracking
  - Send `output_enter`/`output_leave` events
  - Report which output each window is on

- [ ] Implement `set_rectangle` for minimize animations
  - Store minimize target rectangle
  - Use for genie effect animation

- [ ] Add parent window support
  - Track and report parent-child window relationships

- [ ] Implement window filtering
  - Allow compositor to filter which windows are exposed
  - Security/privacy considerations

### Nice to Have
- [ ] Add window icons support (wlr protocol v2+)
- [ ] Add workspace/desktop information
- [ ] Performance optimization for large window counts
- [ ] Add protocol version negotiation logging

## Testing

Verified working with:
- ✅ `rofi -modi window -show window` - Window switcher works
- ✅ Multiple windows shown with correct titles and app IDs
- ✅ Window list updates when windows open/close

To test:
```bash
# Terminal 1: Run compositor
cargo run --features winit -- --winit

# Terminal 2: Test rofi
WAYLAND_DISPLAY=wayland-1 rofi -modi window -show window

# Test waybar (if installed)
WAYLAND_DISPLAY=wayland-1 waybar
```

## Known Limitations

1. Window actions (activate, close, etc.) are stubbed with debug logs
2. Window state (minimized, maximized, fullscreen) not reported
3. Output tracking not implemented
4. Parent window relationships not tracked

## Dock Migration to External App

### Current State
ScreenComposer has a built-in dock implementation (`src/workspaces/dock/`) that's tightly integrated with the compositor's internal window management.

### Migration Path
With wlr-foreign-toplevel-management protocol support, the dock can be migrated to an external standalone application. This provides several benefits:

**Advantages:**
- **Separation of concerns**: Dock becomes an independent component
- **User choice**: Users can use alternative docks (waybar, sfwbar, etc.)
- **Simplified compositor**: Reduces compositor complexity and code size
- **Easier customization**: Dock styling/behavior without compositor changes
- **Independent updates**: Dock can be updated without recompiling compositor

**Requirements for Migration:**
To fully replace the built-in dock with an external app, the following must be implemented:

1. **Window Activation** (High Priority)
   - Implement `Activate` request handler
   - Required for: Clicking dock items to focus windows

2. **Window State Events** (High Priority)
   - Send `state` events with `activated` state
   - Required for: Highlighting active window in dock

3. **Window Close** (Medium Priority)
   - Implement `Close` request handler
   - Required for: Close button on dock items

4. **Minimize Actions** (Medium Priority)
   - Implement `SetMinimized`/`UnsetMinimized` handlers
   - Required for: Minimize to dock functionality
   - Note: Current built-in dock uses genie minimize effect

5. **Set Rectangle** (Nice to Have)
   - Implement `SetRectangle` handler
   - Required for: Genie effect minimize animation target

### External Dock Implementation

A standalone dock app would:
- Bind to `zwlr_foreign_toplevel_manager_v1`
- Use `sc-layer-shell` protocol for positioning/layout
- Implement click handlers that call protocol actions
- Subscribe to window state changes for visual feedback


### Compatibility Notes
- External dock requires fully implemented wlr-foreign-toplevel protocol
- Should also support ext-foreign-toplevel-list for future compatibility

## References

- [ext-foreign-toplevel-list-v1 spec](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml)
- [wlr-foreign-toplevel-management spec](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-foreign-toplevel-management-unstable-v1.xml)
- [Smithay foreign_toplevel_list module](https://smithay.github.io/smithay/smithay/wayland/foreign_toplevel_list/)
- [Built-in dock implementation](../src/workspaces/dock/)
