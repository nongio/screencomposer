# Dock Bookmarking Feature Plan

Goal: add configurable dock bookmarks sourced from desktop files, launching apps or opening running ones, without yet implementing submenus. Phase 2 will extend bookmarks with submenu support.

## Phase 0 – Foundations
- [x] Introduce a `dock` section in the compositor config (`Config` / `sc_config.toml`) with a `bookmarks` array containing desktop entry IDs and optional overrides (custom label, exec args, icon override, etc.).
- [x] Extend `Config` loading defaults and TOML parsing; include validation helpers that log when a bookmark entry cannot be resolved.
- [x] Update `sc_config.example.toml` (and docs) to illustrate the new schema.

## Phase 1 – Desktop Entry Resolution
- [x] Reuse or extend `ApplicationsInfo` to fetch metadata for a desktop ID on demand, returning `Application` (icon, name, exec command).
- [x] Add caching so bookmark resolution happens once per config load; refresh when the config is reloaded.
- [x] Decide how to fall back when a desktop entry is missing or malformed (e.g., show placeholder icon + label).

## Phase 2 – Dock Model & Rendering
- [x] Ensure the dock loads bookmarks on startup and keeps them in `DockModel.launchers` so UI updates preserve them.
- [x] In `DockView::render_elements_layers`, render bookmarks first, then running apps, then minimized windows. Distinguish bookmarks visually (pin badge, dim indicator) while keeping hover/magnification consistent.
- [x] Handle pointer interactions: clicking a bookmark should either focus an already-running instance (`Workspaces::focus_app`) or launch the application if it is absent.

## Phase 3 – Launch & Focus Logic
- [x] Add an app-launch helper that spawns the desktop entry command using the resolved `Exec` line and the current session environment (WAYLAND_DISPLAY, etc.).
- [x] Wire launch requests into the existing async runtime so they do not block the compositor.
- [x] Log launch failures clearly (missing binary, exit status) to aid debugging.

## Phase 4 – Runtime Integration & UX
- [ ] Support config reloads (if already possible) by diffing bookmark sets and updating the dock without restarting the compositor.
- [ ] Ensure magnification, animations, and accessibility (pointer events) work with mixed bookmark + app icons.
- [ ] Update docs (`docs/dock-design.md`, README, config examples) to explain how bookmarking is configured and behaves.

## Phase 5 – Phase 2 Prep (Submenus)
- [ ] Structure bookmark data as a dedicated struct so submenus/actions can be added later without breaking the model.
- [ ] Sketch requirements for submenu support (context menu invocation, action discovery via `Actions=` or custom provider) to guide the next iteration.
