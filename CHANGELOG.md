# Changelog

All notable changes to this project will be documented in this file.

## [0.11.0] - 2026-01-20

### 🚀 Features

- Bump up smithay
- Initial support for foreign toplevel protocol
- Apps-manager component init
- Initial protocol clients sample clients and system design
- Add window-specific popup visibility control
- Improve application info loading and icon fallback
- Update sc-layer protocol implementation
- Add session startup scripts
- *(portal)* Add compositor watchdog for health monitoring
- *(compositor)* Track and apply layer shell exclusive zones
- Add configurable icon_theme option
- Add wlr-foreign-toplevel-management protocol support
- Support monitor resolution and refresh rate from config

### 🐛 Bug Fixes

- Buffer exaaustion for slow clients for screenshare
- Upgrade smitahy, chrome viewport crash
- Skip dock/workspace selector animations for non-current workspaces
- Prevent window jump when dragging maximized windows
- Reposition window during top/left edge resize
- Use requested size for touch resize positioning
- Dock rendering
- Better AGENT.md
- Workspace + sclayer early init
- Dock scaling + config
- Ux style + ux improvement
- Update puffin_http to 0.16 for compatibility with puffin 0.19

### 🚜 Refactor

- Improve expose gesture handling and API

### 📚 Documentation

- Review doc files
- Add profiling section to README
- Add foreign toplevel management documentation
- Add dock migration strategy to foreign-toplevel

### 🎨 Styling

- UI refinements for dock, expose mode, and app switcher

### ⚙️ Miscellaneous Tasks

- Initial protocol implementation layer protocol
- Rendering metrics calculation

## [0.11.0] - 2026-01-20

### 🚀 Features

- Bump up smithay
- Initial support for foreign toplevel protocol
- Apps-manager component init
- Initial protocol clients sample clients and system design
- Add window-specific popup visibility control
- Improve application info loading and icon fallback
- Update sc-layer protocol implementation
- Add session startup scripts
- *(portal)* Add compositor watchdog for health monitoring
- *(compositor)* Track and apply layer shell exclusive zones
- Add configurable icon_theme option
- Add wlr-foreign-toplevel-management protocol support
- Support monitor resolution and refresh rate from config
- Xdg-desktop-portal for screencomposer
- Screenshare fullscreen
- Session script for dbus and keyring
- Script for aumated testing

### 🐛 Bug Fixes

- Buffer exaaustion for slow clients for screenshare
- Upgrade smitahy, chrome viewport crash
- Skip dock/workspace selector animations for non-current workspaces
- Prevent window jump when dragging maximized windows
- Reposition window during top/left edge resize
- Use requested size for touch resize positioning
- Dock rendering
- Better AGENT.md
- Workspace + sclayer early init
- Dock scaling + config
- Ux style + ux improvement
- Update puffin_http to 0.16 for compatibility with puffin 0.19
- Agent instructions + CLAUDE.md symlink
- Agents.md
- Cap screenshare framerate at 60fps for Chrome/WebRTC compatibility
- Improve display mode refresh rate fallback logic in udev backend

### 🚜 Refactor

- Improve expose gesture handling and API

### 📚 Documentation

- Review doc files
- Add profiling section to README
- Add foreign toplevel management documentation
- Add dock migration strategy to foreign-toplevel
- Update screenshare
- Document framerate compatibility issue and fix in screenshare.md

### 🎨 Styling

- UI refinements for dock, expose mode, and app switcher

### ⚙️ Miscellaneous Tasks

- Initial protocol implementation layer protocol
- Rendering metrics calculation
- Remove unused deps
- Bump minimum Rust version to 1.85.0
- Update Rust toolchain to 1.85.0 in GitHub Actions
- Add libpipewire-0.3-dev to CI system dependencies
- Use ubuntu-24.04 for clippy to match pipewire 0.9 requirements

## [0.9.0] - 2025-12-08

### 🚀 Features

- Theme colors, text styles + config
- Multiple workspaces
- Gate perf counters behind feature flag
- Enable debugger feature in default build
- Add scene snapshot functionality
- Make keyboard shortcuts configurable
- Allow remapping modifiers and keysyms
- Toggle maximize window
- Display config
- Sample-clients for submenus
- First implementation of wlr layers
- Enable swipe workspace gesture
- Direct scanout for fullscreen windows in udev backend

### 🐛 Bug Fixes

- Texture loading
- Improve workspace layout and sizing
- Add allow unsafe_code attribute for font initialization
- Workspace rendering
- Dock + app switch theme
- Keyboard mappings
- Dock rendering colors
- Interaction bugs in dock
- Expose show all
- Prevent dragging fullscreen surfaces
- Workspace selector preview size
- Minimize windows
- Delete fullscreen workspace
- Reset focus on minimize window
- Genie effect glitches
- On undo window drag/drop restore expose window sorting
- When moving windows between workspaces ensure the expose is uptodate
- Workspace move indexing
- Clean logs
- Opening appswitch should exist expose mode
- Popup surface commit / update
- Popups rendering
- Keyboard focus when switching workspaces
- Crash on wlr delete
- Expose overlay opacity on first open

### 🚜 Refactor

- Split state in multiple files
- Refactor and consolidate workspaces
- Handle all workspace elements in rendering pipeline

### 📚 Documentation

- Docs
- AGENTS docs for expose feature
- Wlr layer shell 1.0

### ⚡ Performance

- Enable image caching for better performance

### ⚙️ Miscellaneous Tasks

- Use rust 1.82.0
- Fps_ticker as custom feature
- [**breaking**] Multiple workspaces
- Simplify renderer code
- Refactor workspaces data flow, dock, app_switcher
- Run rustfmt on workspace modules
- Cleanup inative gpu logs

### Update

- Refactor transitions

## [0.2.0] - 2024-10-26

### 🐛 Bug Fixes

- Fix linter warnings
- Fix app switcher view
- Fix compile issues for xwayland
- Fix binpacking window size
- Fix window position rendering
- Fix clippy warning
- Fix compilation skia_renderer
- Fix udev
- Fix state
- Fix x11
- Fix xdg shell
- Fix grabs
- Fix input_handler
- Fix compilation errors
- Fix warnings
- Fix skia version
- Fix smithay version and clippy warnings
- Fix raise multiple windows order

### 🚜 Refactor

- Refactor input handling
- Refactor scene_element
- Refactor and optmisation of update loop
- Refactor workspace views + interactive views
- Refactor quit appswitcher app logic
- Refactor workspace views name and pointer events
- Refactor workspace, dock, add minimize windows stub
- Refactor app switcher
- Refactor window selector
- Refactor windows positioning
- Refactor scene damage tracking
- Refactor dock + animations

### 📚 Documentation

- Dock view stub
- Dock minimize animation fix

### ⚙️ Miscellaneous Tasks

- Fix build
- Remove msrv job

<!-- generated by git-cliff -->
