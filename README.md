# Otto
A wayland compositor and stacking window manager,  built on top of [LayersEngine](https://github.com/nongio/layers);

The compositor is heavily inspired by MacOS, the goal is to learn and experiment with wayland, rust, skia and see how far can I go with it.

## :information_source: Disclamer
The project is not currently ready for a daily usage but any contributor curious to support is welcome. MANY things are hardcoded while trying to figure out the best way to implement them. Sudden crashes are expected, especially with the tty-udev backend which may result in a system freeze.

## :framed_picture: How does it look like?
<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/1-dock.gif?raw=true" alt="Dock">
  <figcaption>Dock task manager showing running applications.</figcaption>
</figure>

<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/2-dock-minimize-window.gif?raw=true" alt="Minimize window to Dock">
  <figcaption>Minimizing windows with animated genie effect to the Dock.</figcaption>
</figure>

<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/3-dock-switch-app-and-workspace.gif?raw=true" alt="Switch apps and workspaces">
  <figcaption>Switching between applications and workspaces from the Dock.</figcaption>
</figure>

<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/4-app-switcher.gif?raw=true" alt="Application switcher">
  <figcaption>Application switcher showcasing icons and application names, with background blur blending.</figcaption>
</figure>

<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/5-move-window-to-workspace.gif?raw=true" alt="Move window to workspace">
  <figcaption>Moving windows between workspaces.</figcaption>
</figure>

<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/6-windows-expose-1.gif?raw=true" alt="Expose windows">
  <figcaption>Expose windows, showing all open windows.</figcaption>
</figure>

<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/7-scroll-workspaces.gif?raw=true" alt="Scroll workspaces">
  <figcaption>Scrolling through workspaces with smooth animations.</figcaption>
</figure>

## Is it usable?
Yes-ish, you can try it out, though it is not ready for daily usage. The project is still in the early stages of development, and many features are missing. The project is not yet packaged for any distribution, so you'll need to build it yourself.
Following a draft roadmap of features and improvements.

## Features / Roadmap

### Core
- [x] Single screen support
- [ ] Multiple screens

### Ideas
- [ ] virtual screens available remotely (VNC/RDP)

### Basic window management
- [x] Move windows
- [x] Resize windows
- [x] Window fullscreen/maximize (animated)
- [x] Minimize windows (animated / integrated with Dock)

### Applications switcher (default shortcut: `Ctrl + Tab`)
- [x] Background search for application metadata and icons (following XDG specs)
- [x] Close applications from application switcher
- [x] Cycle between same app windows

### Expose windows (default shortcut: `PageDown` or gesture: `three fingers swipe up`)
- [x] All windows (algorithm from gnome-shell/kde)
- [x] Preview windows with name
- [x] Show all desktop

### Dock task manager
- [x] Show running applications in their opening order
- [x] Show minimized windows
- [x] Show pinned/bookmarked applications

- [ ] Show favorite locations
- [ ] Refactor out of the compositor core

### Screenshare
- [x] XDG Desktop Portal backend (see [docs/xdg-desktop-portal.md](./docs/xdg-desktop-portal.md))
- [x] ScreenCast portal for PipeWire screen sharing
- [x] Fullscreenshare with blitting from GPU and dmabuf

- [ ] Window capture support
- [ ] Screenshot support
- [ ] Permission dialog UI

### Miscellaneus
- [x] Theming dark/light

- [ ] Brightness control widget
- [ ] Audio volume control widget
- [ ] Keyboard light control widget
- [ ] Notifications

### Input
- [x] Enable natural scrolling
- [x] 2 finger scrolling
- [x] Keyboard remapping

- [ ] Scroll acceleration

### Workspace
- [x] Configurable background
- [x] Multiple workspaces
- [x] Workspace switcher (animated)
- [x] Drag windows between workspaces

### Config
- [x] Centralized config manager

### Keyboard Shortcuts
Hotkeys are now fully configurable via the `otto_config.toml` file. See the `[keyboard_shortcuts]` section to customize keybindings for your setup. Example:

```toml
[keyboard_shortcuts]
"Ctrl+Alt+BackSpace" = "Quit"
"Ctrl+Shift+Q" = "Quit"
"Ctrl+Return" = { run = { cmd = "terminator", args = [] } }
"Logo+Space" = { open_default = "file_manager" }
"Logo+B" = { open_default = "browser" }
"Ctrl+1" = { builtin = "Workspace", index = 0 }
```

## Supported Wayland Protocols
Otto implements a comprehensive set of Wayland protocols, including:
- Core: `wl_compositor`, `wl_shm`, `wl_seat`, `wl_data_device_manager`
- Shells: `xdg_wm_base` (XDG shell), `wlr_layer_shell_v1` (Layer shell 1.0)
- Output management: `wl_output`, `xdg_output`, `wp_presentation`
- Rendering: `zwp_linux_dmabuf_v1`, `wp_viewporter`
- Input: pointer gestures, relative pointer, keyboard shortcuts inhibit, text input, input method
- Selection: primary selection, data control (wlr-data-control)
- XDG foreign: cross-client surface identification

For a complete and up-to-date list, see [docs/wayland.md](./docs/wayland.md).

## Components

Otto consists of the main compositor and additional components:

| Component | Description |
|-----------|-------------|
| `otto` | Main compositor binary |
| `xdg-desktop-portal-otto` | XDG Desktop Portal backend for screen sharing |

See [docs/xdg-desktop-portal.md](./docs/xdg-desktop-portal.md) for details on the portal integration.

## How can I contribute?
You can contribute by testing the compositor, reporting bugs, by implementing new features or bring new ideas. Both this project and the LayersEngine are open to contributions. If you have any questions,  open an issue on the repository.

## Building Otto

### Prerequisites
You'll need to install the following dependencies (note, that those package
names may vary depending on your OS and linux distribution):
- `libwayland`
- `libxkbcommon`
- `libudev`
- `libinput`
- `libgbm`
- [`libseat`](https://git.sr.ht/~kennylevinsen/seatd)

If you want to enable X11 support (to run X11 applications within anvil),
then you'll need to install the following packages as well:
    - `xwayland`

## Configure Otto

Otto uses TOML configuration files. A complete example configuration is provided in `otto_config.example.toml` which you can copy and modify:

```bash
cp otto_config.example.toml otto_config.toml
```

### Backend-specific configuration

You can create backend-specific configuration files using the naming convention `otto_config.{backend}.toml`. For example:

- `otto_config.winit.toml` - Configuration for the winit backend
- `otto_config.udev.toml` - Configuration for the tty-udev/DRM backend

When running with a specific backend, Otto will automatically load the corresponding configuration file if it exists, falling back to `otto_config.toml` otherwise.

This allows you to maintain different display settings, keyboard shortcuts, or other preferences for each backend. For instance, you might want different `screen_scale` values or display resolutions when running in a window (winit/X11) versus on bare metal (tty-udev).

For detailed configuration options, see the [configuration documentation](./docs/configuration.md).

## Build and run

You can run it with cargo after having cloned this repository:

```
cd otto;

cargo run -- --{backend}
```

The currently available backends are:

- `--x11`: start anvil as an X11 client. This allows you to run the compositor inside an X11 session or any compositor supporting XWayland. Should be preferred over the winit backend where possible.
- `--winit`: start otto as a [Winit](https://github.com/tomaka/winit) application. This allows you to run it
  inside of an other X11 or Wayland session.
- `--tty-udev`: start otto in a tty with udev support. This is the "traditional" launch of a Wayland
  compositor. Note that this requires you to start otto as root if your system does not have logind
  available.


## Profiling

Otto includes built-in support for profiling using [puffin](https://github.com/EmbarkStudios/puffin). The profiler is enabled by default through the `profile` feature.

### Using the Profiler

1. **Run the compositor** - The puffin HTTP server starts automatically on port 8585:
   ```bash
   cargo run -- --winit
   ```

2. **Install puffin_viewer** (if you haven't already):
   ```bash
   cargo install puffin_viewer
   ```

3. **Connect to the profiler**:
   - Launch `puffin_viewer`
   - Connect to `127.0.0.1:8585`

The profiler will show frame timing, render performance, and other metrics to help identify performance bottlenecks.

**Note:** Make sure your `puffin_viewer` version matches the puffin version used by Otto (0.19.x requires puffin_viewer 0.22.0 or later).


### Credits
- Icon used: [WhiteSur Icon Theme](https://github.com/vinceliuice/WhiteSur-icon-theme)
- Font used: [Inter Font](https://rsms.me/inter/)
- Background used: [hdqwalls](https://hdqwalls.com/wallpaper/1152x864/macos-sonoma-orange-dark/)