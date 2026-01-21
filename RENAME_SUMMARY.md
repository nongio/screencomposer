# Project Rename Summary: screen-composer → otto

## Overview
Successfully renamed the project from "screen-composer" to "otto" across all files, documentation, and code.

## Files Renamed

### Configuration Files
- `sc_config.toml` → `otto_config.toml`
- `sc_config.example.toml` → `otto_config.example.toml`
- `sc_config.winit.toml` → `otto_config.winit.toml`

### Directories
- `components/xdg-desktop-portal-sc/` → `components/xdg-desktop-portal-otto/`
- `wlcs_screencomposer/` → `wlcs_otto/`

## Code Changes

### Package Names (Cargo.toml)
- Main package: `screen-composer` → `otto`
- Portal package: `xdg-desktop-portal-screencomposer` → `xdg-desktop-portal-otto`

### Rust Code
- Struct: `ScreenComposer<T>` → `Otto<T>`
- Crate references: `screen_composer::` → `otto::`
- Module references: `xdg_desktop_portal_screencomposer` → `xdg_desktop_portal_otto`

### D-Bus Interfaces
- `org.screencomposer.ScreenCast` → `org.otto.ScreenCast`
- `org.screencomposer.Compositor` → `org.otto.Compositor`

### Environment Variables
- `XDG_CURRENT_DESKTOP=screencomposer` → `XDG_CURRENT_DESKTOP=otto`

### PipeWire Stream Names
- `screencomposer-screencast` → `otto-screencast`

## Documentation Updates

### Files Updated
- README.md
- AGENTS.md
- CLAUDE.md
- CHANGELOG.md
- ROADMAP.md
- All files in `docs/`
- Component README files

### Protocol Files (NOT Changed)
- `protocols/sc-layer-v1.xml` - kept as-is per user request

## Build Verification

All main components build successfully:
- ✅ `otto` (main compositor binary)
- ✅ `xdg-desktop-portal-otto` (portal backend)
- ✅ `apps-manager`

Generated binaries:
- `target/debug/otto` (464M)
- `target/debug/xdg-desktop-portal-otto` (61M)
- `target/debug/apps-manager` (13M)

## Statistics
- Total files modified: 86
- All Rust source files in `src/` updated
- All component source files updated
- All markdown documentation updated
- All shell scripts updated

## Known Issues
- Sample client `hello-layers` has unrelated build error (pre-existing)

## Commands to Use New Names

```sh
# Run the compositor
cargo run -- --winit

# Build the portal
cargo build -p xdg-desktop-portal-otto

# Run apps manager
cargo run -p apps-manager

# Use new config
cp otto_config.example.toml otto_config.toml
```

