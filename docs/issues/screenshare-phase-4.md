Screenshare Phase 4 — FrameTap and dmabuf path

Scope
- Add FrameTap hook after render and before swap in udev path.
- Provide dmabuf handles to consumers when available; fallback to RGBA map.

Acceptance
- FrameTap receives frames for selected output(s) without impacting visible FPS significantly (<5% regression).
- dmabuf path verified with zero-copy where supported.

Reference: ../screenshare.md
