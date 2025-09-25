Screenshare Phase 2 — zwlr_screencopy_v1 server

Scope
- Implement screencopy capture_output and capture_output_region.
- Prefer dmabuf export; fallback to shm readback.
- Cursor inclusion flag; basic damage/throttling.

Acceptance
- wf-recorder/obs via xdg-desktop-portal-wlr can capture a monitor.
- If dmabuf unsupported, shm path still works.

Reference: ../screenshare.md
