# ScreenComposer
A wayland compositor and stacking window manager,  built on top of [LayersEngine](https://github.com/nongio/layers);

The compositor is heavily inspired by MacOS, the goal is to learn and experiment with wayland, rust, skia and see how far can I go with it.

## :information_source: Disclamer
The project is not currently ready for a daily usage but any contributor curious to support is welcome. MANY things are hardcoded while trying to figure out the best way to implement them. Sudden crashes are expected, especially with the tty-udev backend which may result in a system freeze.

## :framed_picture: How does it look like?
<figure>
  <img src="https://github.com/nongio/screencomposer/blob/main/assets/simple_desktop_with_dock.jpg?raw=true" alt="Basic desktop with Dock">
  <figcaption>Standard desktop with windows overlapping and Dock task manager. Windows shadow is added by the compositor.</figcaption>
</figure>


<figure>
  <img src="https://github.com/nongio/screencomposer/blob/main/assets/application_switcher.jpg?raw=true" alt="Application switcher">
  <figcaption>Application switcher showcasing icons and application names, with background blur blending.</figcaption>
</figure>


<figure>
  <img src="https://github.com/nongio/screencomposer/blob/main/assets/expose_windows.jpg?raw=true" alt="Expose windows">
  <figcaption>Expose windows, showing all open windows.</figcaption>
</figure>

## Is it usable?
Yes-ish, you can try it out, though it is not ready for daily usage. The project is still in the early stages of development, and many features are missing. The project is not yet packaged for any distribution, so you'll need to build it yourself.
Following a draft roadmap of features and improvements.

## Features / Roadmap

### Basic window management
- [x] move windows
- [x] resize windows
- [x] window fullscreen/maximize
- [x] window scaling
- [x] animated drag and drop
- [ ] minimize windows
### Applications switcher (hotkey: `alt + Tab`)
- [x] background search for application metadata and icons (following xdg specs)
- [x] close applications from application switcher
- [x] cycle between same app windows
### Expose windows (hotkey: `alt + f` gesture on udev: `three fingers swipe up`)
- [x] all windows (algorithm from gnome-shell/kde)
- [x] preview windows with name

### Dock task manager
- [x] show running applications in their opening order
- [ ] show minimized windows
- [ ] show pinned applications
- [ ] show favorite locations

### Screenshare
- [ ] enable screenshare xdg-portal

### Miscellaneus
- [ ] Volume control widget
- [ ] Brightness control widget
- [ ] Keyboard light control widget
- [ ] Theming dark/light

### Natural scrolling
- [x] enable natural scrolling
- [x] 2 finger scrolling
- [ ] calculate scroll acceleration

### Workspace
- [ ] dynamic background
- [ ] enable multiple screens
- [ ] screen rotation / resolution
- [ ] multiple workspaces
- [ ] workspace switcher

### Config
- [ ] centralized config manager
- [ ] persistent config from text files
- [ ] setup keybindings from config
- [ ] setup screen mode

### Hardcoded Hotkeys
- show desktop: `alt + d`
- show all windows: `alt + f`
- cycle current app windows: `alt + r`
- application switcher: `alt + tab`
- open a terminal: `alt + shift + enter`
- open a file browser: `alt + shift + space`
- open a browser: `alt + shift + b`

If you want to customise the binaries triggered by the hotkeys, you can edit the `src/config.rs` file.

If you want to change the hotkeys, take a look at: `src/input_handler.rs#L1401`

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

## Build and run

You can run it with cargo after having cloned this repository:

```
cd screen-composer;

cargo run -- --{backend}
```

The currently available backends are:

- `--x11`: start anvil as an X11 client. This allows you to run the compositor inside an X11 session or any compositor supporting XWayland. Should be preferred over the winit backend where possible.
- `--winit`: start screen-composer as a [Winit](https://github.com/tomaka/winit) application. This allows you to run it
  inside of an other X11 or Wayland session.
- `--tty-udev`: start screen-composer in a tty with udev support. This is the "traditional" launch of a Wayland
  compositor. Note that this requires you to start screen-composer as root if your system does not have logind
  available.

### Credits
- Icon used: [WhiteSur Icon Theme](https://github.com/vinceliuice/WhiteSur-icon-theme)
- Font used: [Inter Font](https://rsms.me/inter/)
- Background used: [hdqwalls](https://hdqwalls.com/wallpaper/1152x864/macos-sonoma-orange-dark/)