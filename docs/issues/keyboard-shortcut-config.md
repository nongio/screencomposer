# Keyboard Shortcut Configuration

## Summary
Introduce a configurable keyboard shortcuts map so default bindings live in `sc_config.toml` instead of being hard-coded. Support both simple built-in actions and parameterized commands while keeping the configuration readable.

## Tasks
- [x] Add shortcut config types and serde support (`ShortcutConfig`, `ShortcutTrigger`) under `src/config/shortcuts.rs`; extend `Config` to load a `BTreeMap<String, ShortcutConfig>` and document accepted modifier/key names.
- [x] Implement trigger parsing helpers that normalize modifiers and resolve keysyms, emitting warnings for invalid definitions and logging the final bindings.
- [x] Refactor `process_keyboard_shortcut` to resolve actions via the parsed config map, mapping built-in actions and `run` commands onto `KeyAction` variants, and gracefully handle duplicate triggers.
- [x] Refresh `sc_config.example.toml` with the map-style shortcut examples (mixing simple strings and inline tables) and document the schema in the configuration docs.
- [x] Add unit tests covering shortcut parsing, duplicate detection, invalid configs, and action resolution in the input handler.

## Phase 2
- [x] Introduce an `OpenDefaultApp` shortcut action that resolves a role (e.g. `terminal`, `browser`, `file_manager`) via the XDG MIME application database (`mimeapps.list`).
- [x] Build a small resolver that looks up default handlers for well-known MIME types and falls back to `xdg-open` when the role is unmapped.
- [x] Extend the shortcut parser to accept `action = { open_default = "browser" }` (or similar) and ensure the input handler routes it through the resolver.
- [x] Document the new action and its reliance on XDG defaults, including how users can override them.

## Notes
- Use `BTreeMap` for deterministic iteration when logging effective bindings.
- Prefer warning and skipping invalid entries rather than aborting startup.
- Log which shortcut takes precedence when duplicates occur (last definition wins).
