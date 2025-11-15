**Keyboard Remap Overview**

ScreenComposer reads optional `[modifier_remap]` and `[key_remap]` tables from `sc_config.toml`.  
During startup these entries are parsed into two lookup tables (see `src/config/mod.rs`).  
When a key event arrives:

- Regular keycodes are rewritten according to the parsed `(Keysym -> Keysym)` pairs before Smithay consumes them (`src/state/mod.rs`, `src/input_handler.rs`).
- The compositor also rewrites the modifier bits (`ModifiersState` plus the serialized masks we forward to clients) based on `modifier_remap`. A recent fix ensures cycles such as swapping `logo` and `ctrl` keep the serialized state consistent, so your shortcuts continue to work reliably.

Because this logic runs inside the compositor, remaps apply to all Wayland and Xwayland clients connected to ScreenComposer, but other compositors or TTYs are unaffected. For global system-wide remaps you should still use tools such as keyd or libinput hwdb overrides.

**Configuring remaps in `sc_config.toml`**

Add the sections below to `sc_config.toml` (or the backend-specific override such as `sc_config.winit.toml`). Key names use the standard XKB keysym strings—case-insensitive and matching `xkbcommon`’s lookup logic.

```toml
# Swap the Super/Logo key with Control.
[modifier_remap]
logo = "ctrl"
ctrl = "logo"
alt = "shift"        # optional additional remaps

# Translate keys before they are forwarded to applications.
[key_remap]
BackSpace = "Delete"
Caps_Lock = "Escape"
F13 = "XF86AudioMute"
```

Guidelines:

1. Use the same modifier names that appear in Smithay (`ctrl`, `alt`, `shift`, `logo`, `caps`, `num`).  
2. Keysym strings must be valid XKB names (e.g., `Return`, `Escape`, `XF86AudioPlay`). You can list them with `xev`/`wev` or consult `xkbcommon-keysyms.h`.  
3. Restart the compositor after saving the file. On startup ScreenComposer will log warnings for malformed entries and continue with the valid ones.

With these tables in place, standard shortcuts—as well as compositor-level bindings—behave as if the hardware had been rewired, while remaining scoped to ScreenComposer sessions.

**Limitations to keep in mind**

- Remap tables are read once at compositor startup. You must restart ScreenComposer after editing `sc_config.toml`.
- Remapping is keysym-based. If the source or destination symbol is not present in the current keyboard layout, the entry is ignored.
- Key remaps only look at the base level of each key (level 0). Symbols accessible exclusively through Shift/AltGr cannot be mapped yet.