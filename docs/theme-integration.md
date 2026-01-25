# Theme Integration with Legacy Apps

Otto exposes theme settings (color scheme, icons, cursors) via the **XDG Desktop Portal Settings** interface. Modern GTK4 and Qt6 apps automatically detect your theme preference through the portal.

## Compatibility

✅ **Works automatically:**
- Modern GTK4 apps (GNOME 40+)
- Modern Qt6 apps
- Electron apps with recent versions
- Any app using `org.freedesktop.portal.Settings`

⚠️ **May need manual configuration:**
- Older GTK3 apps (check gsettings)
- Older Qt5 apps (check qt5ct/qt6ct or kdeglobals)
- Chrome/Chromium (checks gsettings first)

## Manual Theme Sync (Optional)

If some apps don't detect your theme, you can manually sync Otto's config to system settings.

### For GTK Apps (gsettings)

```bash
# Light theme
gsettings set org.gnome.desktop.interface color-scheme 'prefer-light'
gsettings set org.gnome.desktop.interface gtk-theme 'Adwaita'

# Dark theme
gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark'
gsettings set org.gnome.desktop.interface gtk-theme 'Adwaita-dark'

# Also sync cursor/icon themes if needed
gsettings set org.gnome.desktop.interface cursor-theme 'Notwaita-Black'
gsettings set org.gnome.desktop.interface icon-theme 'Papirus'
```

### For KDE/Qt Apps (kdeglobals)

```bash
# Light theme
kwriteconfig5 --file kdeglobals --group General --key ColorScheme "BreezeLight"

# Dark theme
kwriteconfig5 --file kdeglobals --group General --key ColorScheme "BreezeDark"
```

Restart KDE apps to see changes.

### For Qt5ct/Qt6ct Users

If you use `qt5ct` or `qt6ct` (check with `echo $QT_QPA_PLATFORMTHEME`):

Edit `~/.config/qt5ct/qt5ct.conf` and `~/.config/qt6ct/qt6ct.conf`:

```ini
[Appearance]
style=Adwaita         # Light theme
# style=Adwaita-Dark  # Dark theme
```

Restart Qt apps to see changes.

## Automatic Sync Script (Optional)

Create a script to sync on Otto startup:

```bash
#!/bin/bash
# ~/.config/otto/sync-theme.sh

THEME=$(grep 'theme_scheme' ~/.config/otto/otto_config.toml | grep -o '"[^"]*"' | tr -d '"')

if [ "$THEME" = "Dark" ]; then
    gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark'
    gsettings set org.gnome.desktop.interface gtk-theme 'Adwaita-dark'
    kwriteconfig5 --file kdeglobals --group General --key ColorScheme "BreezeDark"
else
    gsettings set org.gnome.desktop.interface color-scheme 'prefer-light'
    gsettings set org.gnome.desktop.interface gtk-theme 'Adwaita'
    kwriteconfig5 --file kdeglobals --group General --key ColorScheme "BreezeLight"
fi
```

Make executable and run on login.

## Testing

Verify portal is working:

```bash
# Check portal settings
gdbus call --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop \
  --method org.freedesktop.portal.Settings.ReadAll \
  "['org.freedesktop.appearance']"

# Should return:
# ({'org.freedesktop.appearance': {'color-scheme': <uint32 2>}},)
# 1 = dark, 2 = light
```

## Troubleshooting

**App still shows wrong theme:**
1. Check if it's using the portal: `journalctl --user -f | grep portal`
2. Try restarting `xdg-desktop-portal`: `systemctl --user restart xdg-desktop-portal`
3. Check portal config: `cat ~/.config/xdg-desktop-portal/portals.conf`
4. Manually sync using commands above

**Why not auto-sync?**
Every system has different requirements (qt5ct, GNOME, KDE, etc.). Manual configuration gives you full control over which settings to sync and how.
