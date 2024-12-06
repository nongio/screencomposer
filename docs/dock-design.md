# Dock / Task manager component

The dock is a task manager that shows minimized windows and apps. It is a layer that is always on top of the screen.

## Features

### Taskbar
- list running applications
- list minimized windows
### Bookmarking
- list favourite application launchers
  - application launchers and running applications are mixed
- list shortcuts to folders

A dock element is:
- draggable
- clickable
- hoverable
- has a submenu associated with it


## Taskbar
The Dock shows a list of running applications. Each application has an icon and a label. The icon is the application icon and the label is the application name.

The icon and application name is retrieved from the application desktop file. The desktop file is a file that describes the application and is located in `/usr/share/applications/` following the [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html).

- Application icons and names needs to be loaded asynchronously. The loading is done in a separate thread to avoid blocking the main thread.

Few dependencies are required to load the application icons and names:
- xdgkit
- freedesktop-icons
- freedesktop-desktop-entry

## Bookmarking
File previews are shown in the dock.
Listing favourite places and files, requires the dock to be able to render files previews.
This task should be done in a separate application that communicates with the dock.

## Dock submenu
This feature could make the case for a separate application that communicates with the dock.

Should the compositor be responsible for the dock submenu?

## Configuration / Storage
The bookmaking and taskbar configuration needs to be persisted.

## Future considerations
A Wayland protocol to retrieve the application icon and name would be more efficient than reading the desktop file.
