# Otto
A wayland compositor and stacking window manager,  built on top of [LayersEngine](https://github.com/nongio/layers);

The compositor is heavily inspired by MacOS, the goal is to learn and experiment with wayland, rust, skia and see how far can I go with it.

## :information_source: Disclamer
The project is not currently ready for a daily usage but any contributor curious to support is welcome. MANY things are hardcoded while trying to figure out the best way to implement them. Sudden crashes are expected, especially with the tty-udev backend which may result in a system freeze.

## :framed_picture: How does it look like?
<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/simple_desktop_with_dock.jpg?raw=true" alt="Basic desktop with Dock">
  <figcaption>Standard desktop with windows overlapping and Dock task manager. Windows shadow is added by the compositor.</figcaption>
</figure>


<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/application_switcher.jpg?raw=true" alt="Application switcher">
  <figcaption>Application switcher showcasing icons and application names, with background blur blending.</figcaption>
</figure>


<figure>
  <img src="https://github.com/nongio/otto/blob/main/assets/expose_windows.jpg?raw=true" alt="Expose windows">
  <figcaption>Expose windows, showing all open windows.</figcaption>
</figure>

## Is it usable?
Yes-ish, you can try it out, though it is not ready for daily usage. The project is still in the early stages of development, and many features are missing. The project is not yet packaged for any distribution, so you'll need to build it yourself.
Following a draft roadmap of features and improvements.

## Features / Roadmap

### Core
- [ ] login?
- [ ] enable multiple screens (maybe virtual?)
- [ ] fullscreen animation bug (try animated resize)
- [x] listen to animations in LayersEngine
- [ ] custom shape layers

### Basic window management
- [x] move windows
- [x] resize windows
- [x] window fullscreen/maximize
- [x] window scaling
- [x] animated drag and drop
- [x] minimize windows
- [ ] fix resize from edge-top
- [ ] 

### Applications switcher (hotkey: `alt + Tab`)
- [x] background search for application metadata and icons (following xdg specs)
- [x] close applications from application switcher
- [x] cycle between same app windows
- [x] Fullscreen app switcher
- [ ] 

### Expose windows (hotkey: `alt + f` gesture on udev: `three fingers swipe up`)
- [x] all windows (algorithm from gnome-shell/kde)
- [x] preview windows with name
- [ ] show all desktop

### Dock task manager
- [x] show running applications in their opening order
- [x] show minimized windows
- [x] show pinned applications
- [ ] show favorite locations
- [ ] refactor out of the compositor core

### Screenshare
- [x] XDG Desktop Portal backend (see [docs/xdg-desktop-portal.md](./docs/xdg-desktop-portal.md))
- [x] ScreenCast portal for PipeWire screen sharing
- [x] fullscreenshare with dmabuf zero-copy
- [ ] Window capture support (layers_element)
- [ ] screenshot support
- [ ] Permission dialog UI

### Miscellaneus
- [x] Theming dark/light
- [ ] Volume control widget
- [ ] Brightness control widget
- [ ] Keyboard light control widget
- [ ] notifications

### Natural scrolling
- [x] enable natural scrolling
- [x] 2 finger scrolling
- [ ] calculate scroll acceleration

### Workspace
- [x] dynamic background
- [x] multiple workspaces
- [x] workspace switcher
- [ ] animate create/delete workspace
- [ ] enable multiple screens
- [ ] screen rotation / resolution

### Config
- [x] centralized config manager
- [x] persistent config from text files
- [x] setup keybindings from config
- [ ] setup screen mode

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

## Is it open for contributions?
You can contribute by testing the compositor, reporting bugs, by implementing new features or bring new ideas. Both this project and the LayersEngine are open to contributions. If you have any questions,  open an issue on the repository.

## Build Dependencies
You'll need to install the following dependencies (note, that those package
names may vary depending on your OS and linux distribution):
- `libwayland`
- `libxkbcommon`

#### These are needed for the "Udev/DRM backend"

- `libudev`
- `libinput`
- `libgbm`
- [`libseat`](https://git.sr.ht/~kennylevinsen/seatd)

If you want to enable X11 support (to run X11 applications within anvil),
then you'll need to install the following packages as well:
    - `xwayland`

## Configuration

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

### Credits
- Icon used: [WhiteSur Icon Theme](https://github.com/vinceliuice/WhiteSur-icon-theme)
- Font used: [Inter Font](https://rsms.me/inter/)
- Background used: [hdqwalls](https://hdqwalls.com/wallpaper/1152x864/macos-sonoma-orange-dark/)