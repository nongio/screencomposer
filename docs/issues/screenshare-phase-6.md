Screenshare Phase 6 — CLI polish and dynamic target

Scope
- Add `sc screen set-target` and `block-window` commands.
- Hot-switch cast target (output <-> window) without resetting session.
- Add automated tests and goldens for CI.

Acceptance
- Switching target updates the live stream within ~1s without restart.
- Tests pass in CI (headless deterministic path).

Reference: ../screenshare.md
