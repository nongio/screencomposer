# Dock / Task manager component

The dock is a task manager that shows minimized windows and apps. It is a layer that is always on top of the screen.

## Features

### Taskbar
- list running applications
- list minimized windows

### Bookmarking
- list favourite application launchers
  - application launchers and running applications are mixed

A dock element is:
- clickable
- hoverable


## Taskbar
The Dock shows a list of running applications. Each application has an icon and a label. The icon is the application icon and the label is the application name.

The icon and application name is retrieved from the application desktop file. The desktop file is a file that describes the application and is located in `/usr/share/applications/` following the [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html).

- Application icons and names needs to be loaded asynchronously. The loading is done in a separate thread to avoid blocking the main thread.

Few dependencies are required to load the application icons and names:
- xdgkit
- freedesktop-icons
- freedesktop-desktop-entry

## Bookmarking
Bookmarks can now be declared in `sc_config.toml` under the `[dock]` table. Each entry supplies a desktop file id plus optional label/extra arguments, for example:

```toml
[dock]
bookmarks = [
  { desktop_id = "org.kde.dolphin.desktop" },
]
```
Configured launchers are preloaded into the dock and share the same icon/hover behaviour as running apps. Clicking a bookmark focuses an existing instance or launches the desktop entry if nothing is running.


## Dock submenu
This feature could make the case for a separate application that communicates with the dock.

Open questions:
Should the compositor be responsible for the dock submenu?

## Configuration / Storage
The bookmaking and taskbar configuration is stored in the `sc_config.toml` file under the `[dock]` table.

## Future considerations
A Wayland protocol to retrieve the application icon and name would be more efficient than reading the desktop file.

## Runtime behaviour

### Components
- `DockView` owns the layer hierarchy, state cache, and loose coupling to the rest of the compositor (`src/workspaces/dock/view.rs`).
- `DockModel` is the lightweight data model mirrored inside `DockView` (`src/workspaces/dock/model.rs`).
- `DockView` implements `Observer<WorkspacesModel>` so it receives workspace model snapshots (`src/workspaces/dock/view.rs`, `src/workspaces/mod.rs`).
- Pointer-facing logic lives in the `ViewInteractions` impl for `DockView` (`src/workspaces/dock/interactions.rs`).
- Rendering helpers live in `src/workspaces/dock/render.rs`.

### State flow
- `Workspaces` is the observable owner of global workspace state. On layout changes (`update_workspace_model`) or when windows are minimised/restored it clones `WorkspacesModel` and notifies observers (`src/workspaces/mod.rs`).
- At compositor start `Workspaces::new` registers the dock as an observer (`src/workspaces/mod.rs`).
- `DockView::notify` pushes workspace snapshots into an async channel and a throttled task inside `notification_handler` turns the latest snapshot into dock state every 500 ms. This keeps UI updates responsive without re-rendering on every transient event (`src/workspaces/dock/view.rs`).
- When a snapshot arrives, the dock resolves `Application` metadata through `ApplicationsInfo::get_app_info_by_id`, builds `DockModel { running_apps, minimized_windows, … }`, and calls `update_state`, which triggers `render_dock()` (`src/workspaces/dock/view.rs`).
- `render_dock()` recomputes icon sizes, (re)builds layers for app icons / miniwindows, and schedules removal animations for stale layers.

### Layer & layout
- The layer structure is created in `DockView::new`; `wrap_layer` pins the dock to the bottom edge, `view_layer` holds children, and `bar_layer`, `dock_apps_container`, `dock_handle`, and `dock_windows_container` host specific visuals (`src/workspaces/dock/view.rs`).
- `render_elements_layers` computes available icon width from the current dock width, applies size changes, and installs pointer callbacks on per-app layers. Old layers fade/scale out before removal to avoid layout jumps.
- `magnify_elements` drives the Mac-style magnification effect. It reads the current pointer focus position, computes a Gaussian (`magnify_function`) falloff, and schedules size changes through `Engine::schedule_changes` (`src/workspaces/dock/view.rs`).

### Interactions
- Pointer motion updates the magnification focus via `update_magnification_position`; leaving the dock resets it to the sentinel value so icons shrink back (`src/workspaces/dock/interactions.rs`, `src/workspaces/dock/view.rs`).
- Button release looks up the hovered layer. If the layer maps to an app, `Workspaces::focus_app` raises it and the compositor reassigns keyboard focus. If it maps to a minimised window, `Workspaces::unminimize_window` is invoked to restore it (`src/workspaces/dock/interactions.rs`, `src/workspaces/mod.rs`).
- `InputHandler::surface_under` delegates hit testing to `Workspaces::is_cursor_over_dock`, ensuring pointer focus enters the dock before regular windows (`src/input_handler.rs`, `src/workspaces/mod.rs`).

### Minimise / restore integration
- Minimising a window (`Workspaces::minimize_window`) appends the pair `(ObjectId, title)` to `WorkspacesModel.minimized_windows`, updates the dock state, and animates the corresponding `WindowView` into the dock drawer (`src/workspaces/mod.rs`).
- Restoring (`Workspaces::unminimize_window`) removes the entry, orchestrates the genie animation that puts the window back in the workspace, and collapses the dock drawer (`src/workspaces/mod.rs`).
- `DockView::add_window_element` and `remove_window_element` provide the bridge between dock layers and window views during these animations (`src/workspaces/dock/view.rs`).

### Visual details
- Icons render through Skia. `draw_app_icon` paints cached freedesktop icons when available, applies drop shadows, and only draws the indicator dot for running apps; fallbacks render a stroked rounded rect and optional picture (`src/workspaces/dock/render.rs`).
- Labels are built as balloon tooltips with blurred shadows and stay hidden until pointer hover (`src/workspaces/dock/render.rs`).
- The bar uses background blur, configurable colours from `theme_colors()`, and resizes dynamically with icon height (`src/workspaces/dock/view.rs`).

### Dependencies & configuration
- Scaling constants (icon size, genie effect span, screen scale) read from `Config`, so theme and animation tweaks propagate automatically (`src/workspaces/dock/view.rs`, `src/workspaces/dock/render.rs`).
- Application metadata resolution depends on the async `ApplicationsInfo` helper which queries freedesktop desktop entries (`src/workspaces/apps_info.rs`, `src/workspaces/dock/view.rs`).
