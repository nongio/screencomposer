Screenshare Phase 1 — Headless deterministic screenshots

Scope
- Add headless backend path and virtual output creation.
- Implement single-frame screenshot CLI: `sc screen screenshot --out <png> [--output <name>] [--region x,y,w,h] [--frame N]`.
- Ensure deterministic rendering (fixed simulation clock) for CI.

Acceptance
- Running headless with a virtual output produces a PNG identical across runs for the same frame.
- Non-headless builds unaffected.

Reference: ../screenshare.md
