## Color Scheme Setting

This document explains how Otto's color scheme configuration works and how applications determine their theme preference (dark/light mode).

### Current State

Otto has a `theme_scheme` configuration option in `otto_config.toml`:

```toml
# Theme configuration
theme_scheme = "Light"  # or "Dark"
```

**What this controls:**

The `theme_scheme` setting controls:
1. **Otto's own UI** — dock, app switcher, compositor chrome
2. **Client applications** — via the Settings portal implementation

The setting is read from config in `src/config/mod.rs` and:
- Used internally by compositor UI components like `src/workspaces/dock/render.rs` and `src/theme/mod.rs`
- Exposed to applications via the `org.freedesktop.portal.Settings` interface through the portal backend

### The XDG Settings Portal

The standard way for compositors to communicate theme preferences to applications is through the **Settings portal** (`org.freedesktop.portal.Settings`), part of the xdg-desktop-portal specification.

**How it works:**

1. Applications query the portal for the `color-scheme` setting under the `org.freedesktop.appearance` namespace
2. The portal backend (e.g., `xdg-desktop-portal-otto`) bridges this request to the compositor
3. The compositor returns the preference value:
   - `0` = no preference
   - `1` = prefer dark
   - `2` = prefer light

### Otto's Implementation
Otto now implements the Settings portal with the following components:

**Compositor D-Bus Service** (`src/settings_service.rs`):
- Implements `org.otto.Settings` interface at `/org/otto/Settings`
- Exposes `GetColorScheme()` method that returns the current theme preference
- Registered during compositor startup in the screenshare D-Bus service initialization

**Portal Backend** (`components/xdg-desktop-portal-otto/`):
- Implements `org.freedesktop.impl.portal.Settings` interface
- Bridges between the portal API and the compositor's `org.otto.Settings` service
- Handles `ReadAll()` and `Read()` methods per spec
- Supports namespace filtering and glob patterns
- Exposes `org.freedesktop.appearance` namespace with `color-scheme` setting

**Configuration Integration**:
- Portal backend connects to compositor via D-Bus proxy (`src/otto_client/settings.rs`)
- Compositor reads `theme_scheme` from `otto_config.toml` via static config singleton
- Color scheme values per spec:
  - `1` = prefer dark
  - `2` = prefer light

**Portal Registration** (`otto.portal`):
- Registered in `org.freedesktop.impl.portal.desktop.otto` portal backend
- Listed alongside ScreenCast interface in portal capabilities

### Spec Reference

- [XDG Desktop Portal Settings interface](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)
- [freedesktop.org appearance settings](https://github.com/flatpak/xdg-desktop-portal/blob/main/data/org.freedesktop.appearance.xml)

### Future Enhancements

**Dynamic Configuration Reload**:
- Currently requires compositor restart to change theme
- Future: Implement `SettingChanged` signal emission when config is reloaded
- Would allow applications to respond to theme changes without restart

**Additional Settings**:
- `accent-color` — user-selected accent color for UI elements
- `contrast` — high contrast mode for accessibility
- Other `org.freedesktop.appearance` namespace settings

**Testing**:
Applications should now correctly pick up Otto's theme preference. You can verify with:

```bash
# Query the portal backend directly
gdbus call --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop \
  --method org.freedesktop.portal.Settings.Read \
  org.freedesktop.appearance color-scheme

# Check GNOME settings (if GNOME apps installed)
gsettings get org.gnome.desktop.interface color-scheme
```
