# XDG Desktop Portal Integration

ScreenComposer includes a custom XDG Desktop Portal backend for integrating with
desktop applications that use the freedesktop portal APIs.

> **See also:** [screenshare.md](screenshare.md) for detailed compositor-side
> implementation documentation.

## Component: xdg-desktop-portal-screencomposer

Located in `components/xdg-desktop-portal-sc/`, this is a standalone binary that
implements the portal backend interfaces on D-Bus.

### Currently Supported Portals

| Portal | Status | Description |
|--------|--------|-------------|
| ScreenCast | ✅ Implemented | Screen sharing via PipeWire |

### ScreenCast Portal

The ScreenCast portal enables applications like Chromium, Firefox, OBS Studio,
and other PipeWire-capable clients to share your screen.

**Supported features:**
- Monitor (output) capture
- Cursor modes: Hidden, Embedded, Metadata
- PipeWire stream creation with SHM buffers
- Video format negotiation (BGRA/RGBA)
- Damage tracking for efficient updates

**Not yet implemented:**
- Window capture
- Restore tokens (session persistence)
- Permission dialogs (currently auto-grants)
- DMA-BUF zero-copy path

### Architecture

```
Application (Chromium/OBS/Firefox)
       ↓
xdg-desktop-portal (system service)
       ↓  org.freedesktop.impl.portal.ScreenCast
xdg-desktop-portal-screencomposer (this component)
       ↓  org.screencomposer.ScreenCast
ScreenComposer compositor
       ↓
PipeWire stream
```

The portal backend acts as a translator between the standard XDG Desktop Portal
interface and ScreenComposer's internal D-Bus API.

### Running

The portal backend runs as a separate process alongside the compositor:

```bash
# Build and run the portal backend
cargo run -p xdg-desktop-portal-screencomposer
```

The portal registers on the D-Bus session bus as
`org.freedesktop.impl.portal.desktop.screencomposer`.

### Configuration

The system's `xdg-desktop-portal` service needs to be configured to use this
backend. This is typically done via a `.portal` file in
`/usr/share/xdg-desktop-portal/portals/` or `~/.local/share/xdg-desktop-portal/portals/`.

Example `screencomposer.portal`:

```ini
[portal]
DBusName=org.freedesktop.impl.portal.desktop.screencomposer
Interfaces=org.freedesktop.impl.portal.ScreenCast
UseIn=screencomposer
```

### Debugging

Set the `RUST_LOG` environment variable to control log verbosity:

```bash
RUST_LOG=debug cargo run -p xdg-desktop-portal-screencomposer
```

### Future Plans

- Window capture support
- Permission dialog UI
- Restore tokens for session persistence
- Additional portals (Screenshot, RemoteDesktop)
