## Color Scheme Setting

This document explains how Otto's color scheme configuration works and how applications determine their theme preference (dark/light mode).

### Current State

Otto has a `theme_scheme` configuration option in `otto_config.toml`:

```toml
# Theme configuration
theme_scheme = "Light"  # or "Dark"
```

**What this controls:**

Currently, `theme_scheme` only affects **Otto's own UI** (dock, app switcher, compositor chrome). The setting is read from config in `src/config/mod.rs` and used internally by compositor UI components like `src/workspaces/dock/render.rs` and `src/theme/mod.rs`.

**What it does NOT control:**

Client applications (Chrome, Firefox, GNOME apps, Qt apps) do **not** receive this preference from Otto. They determine their theme from other sources:

- `GTK_THEME` environment variable
- `QT_STYLE_OVERRIDE` environment variable  
- gsettings/dconf system-wide settings
- Application-specific preferences
- Other desktop portal backends if running

### The XDG Settings Portal

The standard way for compositors to communicate theme preferences to applications is through the **Settings portal** (`org.freedesktop.portal.Settings`), part of the xdg-desktop-portal specification.

**How it works:**

1. Applications query the portal for the `color-scheme` setting under the `org.freedesktop.appearance` namespace
2. The portal backend (e.g., `xdg-desktop-portal-otto`) bridges this request to the compositor
3. The compositor returns the preference value:
   - `0` = no preference
   - `1` = prefer dark
   - `2` = prefer light

**Current implementation status:**

Otto does **not yet implement** the Settings portal. Only the ScreenCast portal is implemented in `components/xdg-desktop-portal-otto/`.

### Implementation Path

To make Otto's `theme_scheme` config control application themes, you would need to:

1. **Add Settings portal backend** in `components/xdg-desktop-portal-otto/`:
   - Implement `org.freedesktop.impl.portal.Settings` interface
   - Handle `ReadAll()` and `Read()` methods
   - Emit `SettingChanged` signal when theme changes

2. **Expose compositor state via D-Bus** (two options):
   - Option A: Create a compositor-side D-Bus service (like `org.otto.ScreenCast` for screensharing)
   - Option B: Read Otto's config file directly from the portal backend

3. **Config reload handling**:
   - If config is reloaded at runtime, emit `SettingChanged` signal to notify applications

4. **Additional settings to expose**:
   - `color-scheme` (primary)
   - `accent-color` (future)
   - `contrast` (accessibility, future)

### Reference Implementation

Other compositors that implement this:

- **GNOME Shell** — gsettings → mutter → portal backend
- **KDE Plasma** — KConfig → KWin → portal backend  
- **Sway/wlroots** — various portal backends read desktop-specific configs

### Spec Reference

- [XDG Desktop Portal Settings interface](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)
- [freedesktop.org appearance settings](https://github.com/flatpak/xdg-desktop-portal/blob/main/data/org.freedesktop.appearance.xml)

### Where to Start

If implementing this feature:

1. Read `docs/developer/screenshare.md` — the Settings portal would follow a similar architecture (portal backend ↔ compositor communication)
2. Check `components/xdg-desktop-portal-otto/src/portal/interface.rs` — add a new Settings interface alongside ScreenCast
3. Decide whether to expose via D-Bus service or direct config file reading
4. Test with applications: `gsettings get org.gnome.desktop.interface color-scheme` should reflect Otto's config after implementation
