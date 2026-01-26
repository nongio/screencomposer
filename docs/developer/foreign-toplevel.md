## Foreign Toplevel Management

Otto implements dual-protocol support for foreign toplevel management, enabling external applications (taskbars, launchers, window switchers) to enumerate and interact with compositor windows.

## Protocol Support

Otto supports two foreign toplevel protocols:

### ext-foreign-toplevel-list-v1
- **Status**: Fully implemented via Smithay
- **Spec**: [ext-foreign-toplevel-list-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml)
- **Features**: Read-only window list with title and app_id
- **Use case**: Modern protocol for simple window enumeration

### wlr-foreign-toplevel-management-unstable-v1
- **Status**: Partially implemented
- **Spec**: [wlr-foreign-toplevel-management](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-foreign-toplevel-management-unstable-v1.xml)
- **Features**: Window list with control actions (activate, close, minimize, etc.)
- **Use case**: Widely adopted by wlroots-based tools (rofi, waybar)

### Unified Abstraction

`src/state/foreign_toplevel_shared.rs` provides a unified API via `ForeignToplevelHandles`:

```rust
pub struct ForeignToplevelHandles {
    pub ext: Option<ExtHandle>,
    pub wlr: Option<WlrForeignToplevelHandle>,
}
```

This struct wraps both protocol handles and provides methods that broadcast state changes to both protocols simultaneously:

- `send_title(&str)` - Update window title
- `send_app_id(&str)` - Update application ID
- `send_done()` - Signal end of state batch (ext protocol only)
- `send_closed()` - Notify window destruction

### Protocol Handlers

**ext-foreign-toplevel-list** (`src/state/foreign_toplevel_list_handler.rs`):
- Uses Smithay's built-in implementation
- Simple delegate pattern with no additional code needed

**wlr-foreign-toplevel-management** (`src/state/wlr_foreign_toplevel.rs`):
- Custom implementation
- Manages global `WlrForeignToplevelManagerState` with active manager instances
- Each window gets a `WlrForeignToplevelHandle` that holds window metadata and client resources
- Implements `Dispatch` traits for request handling

## Integration Flow

### Window Creation

When a new XDG toplevel surface is mapped (`src/shell/xdg.rs`):

1. Extract app_id and title from the window
2. Create ext-foreign-toplevel-list handle via Smithay
3. Create wlr-foreign-toplevel handle via custom implementation
4. Wrap both in `ForeignToplevelHandles`
5. Store in `state.foreign_toplevels` HashMap keyed by surface ObjectId

```rust
let ext_handle = self.foreign_toplevel_list_state
    .new_toplevel::<Self>(&app_id, &title);
let wlr_handle = self.wlr_foreign_toplevel_state
    .new_toplevel::<Self>(&display_handle, &app_id, &title);
let handles = ForeignToplevelHandles::new(ext_handle, wlr_handle);
self.foreign_toplevels.insert(surface_id, handles);
```

### Window Updates

Window state changes are broadcasted from `src/shell/mod.rs`:

- Title changes trigger `handle.send_title(&title)`
- App ID changes trigger `handle.send_app_id(&app_id)`
- Both methods update internal state and send events to all subscribed clients

The implementation prevents redundant updates by checking if values actually changed before broadcasting.

### Window Destruction

When a window is unmapped (`src/shell/xdg.rs`):

```rust
if let Some(handle) = self.foreign_toplevels.remove(&id) {
    handle.send_closed();
}
```

This notifies all connected taskbars/launchers to remove the window from their lists.

## Current Features

**Implemented:**
- ✅ Window list enumeration (both protocols)
- ✅ Title updates (both protocols)
- ✅ App ID updates (both protocols)
- ✅ Window closed notifications (both protocols)
- ✅ Automatic state synchronization across protocols
- ✅ Multi-client support (multiple taskbars can connect simultaneously)

**Not Implemented (wlr protocol only):**
- ❌ Window activation (focus) - `Activate` request logs debug message
- ❌ Window close - `Close` request logs debug message
- ❌ Minimize/unminimize - `SetMinimized`/`UnsetMinimized` log debug messages
- ❌ Maximize/unmaximize - `SetMaximized`/`UnsetMaximized` log debug messages
- ❌ Fullscreen - `SetFullscreen`/`UnsetFullscreen` log debug messages
- ❌ Rectangle hints - `SetRectangle` logs debug message
- ❌ State reporting - No `state` events sent (activated, maximized, minimized, fullscreen)
- ❌ Output tracking - No `output_enter`/`output_leave` events

## Implementation Details

### wlr Protocol State Management

The wlr implementation maintains two levels of state:

1. **Global State** (`WlrForeignToplevelManagerState`):
   - Stores list of active manager instances (one per connected client)
   - Creates toplevels and broadcasts them to all managers
   - Handles manager registration/unregistration

2. **Per-Window State** (`WlrToplevelData`):
   - Stores app_id, title
   - Maintains list of protocol resources (one per manager instance)
   - Wrapped in `Arc<Mutex<>>` for shared access

When a new manager binds:
- All existing windows are re-sent to the new client
- Each window's resources list gains a new entry
- Future updates broadcast to all resources including the new one

### Request Handling

All wlr protocol requests are currently stubbed with debug logs in `src/state/wlr_foreign_toplevel.rs`:

```rust
fn request(...) {
    match request {
        Request::Activate { .. } => {
            tracing::debug!("wlr foreign toplevel: activate requested");
        }
        Request::Close => {
            tracing::debug!("wlr foreign toplevel: close requested");
        }
        // ... etc
    }
}
```

To implement these actions, the handler would need:
- Access to compositor state (workspaces, window map)
- Ability to look up window by handle
- Methods to invoke window operations (focus, close, minimize, etc.)

## Testing

Verified working with:
- ✅ `rofi -modi window -show window` - Shows window list with correct titles
- ✅ Window list updates when windows open/close
- ✅ Multiple windows display correctly

Test commands:
```bash
# Terminal 1: Run compositor
cargo run -- --winit

# Terminal 2: Test with rofi
WAYLAND_DISPLAY=wayland-1 rofi -modi window -show window

# Terminal 3: Test with waybar (if installed)
WAYLAND_DISPLAY=wayland-1 waybar
```

**Note**: Window actions (clicking to focus, close buttons) will not work due to unimplemented request handlers.

## Known Limitations

1. **Read-only for wlr protocol**: While the protocol supports window control actions, Otto currently only implements the read-only window list functionality. All control requests are logged but ignored.

2. **No state tracking**: Windows don't report their current state (minimized, maximized, fullscreen, activated) to external applications.

3. **No output information**: Windows don't report which output they're displayed on.

4. **No parent tracking**: Child/transient window relationships are not exposed.

## Future Work

To enable full external dock/taskbar functionality:

1. **Implement window activation**: Look up window by foreign toplevel handle and call `workspaces.focus_app()` or similar
2. **Implement state events**: Track and broadcast window state changes (activated, minimized, etc.)
3. **Implement minimize actions**: Integrate with existing `workspaces.minimize_window()` / `unminimize_window()`
4. **Implement close action**: Send close request to window surface
5. **Add output tracking**: Send `output_enter`/`output_leave` based on window position

## Related Documentation

- [Dock Design](./dock-design.md) - Current built-in dock implementation
- [Wayland Protocols](./wayland.md) - General Wayland protocol handling in Otto
- [Smithay foreign_toplevel_list](https://smithay.github.io/smithay/smithay/wayland/foreign_toplevel_list/)
