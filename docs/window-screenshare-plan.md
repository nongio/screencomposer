# Window Screenshare Implementation Plan

This document outlines the plan to implement different sharing modes in ScreenComposer, particularly window-level and selection-based screensharing, inspired by Niri's comprehensive implementation.

## Current State (ScreenComposer)

Currently, ScreenComposer only supports **full output (monitor) screensharing**:
- Records entire outputs via their connector names
- D-Bus API: `RecordMonitor(connector: s, properties: a{sv})`
- Direct GPU blit from output framebuffer to PipeWire
- No window selection or region selection capabilities

## Target Capabilities

Three distinct sharing modes:

### 1. **Monitor Sharing** (Already Implemented)
- Share entire output by connector name
- Uses `StreamTarget::Output(WeakOutput)`

### 2. **Window Sharing** (Not Implemented)
- Share individual window with popups
- Uses `StreamTarget::Window { id: u64 }`
- Window selection via portal UI
- Dynamic resizing as window size changes
- Includes child windows/popups

### 3. **Dynamic/Portal-Controlled Sharing** (Not Implemented)
- Starts as "Nothing" until user selects via portal UI
- Can switch between outputs/windows/regions
- Used by xdg-desktop-portal for user-driven selection
- `StreamTarget::Nothing` → selected target

## Implementation Plan

### Phase 1: Core Infrastructure

#### 1.1 Extended StreamTarget Enum

**File:** `src/screenshare/mod.rs`

```rust
/// Target for a screencast stream.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StreamTarget {
    /// No target selected (for portal-initiated dynamic casts)
    Nothing,
    /// Share entire output
    Output { 
        connector: String,
    },
    /// Share specific window and its popups
    Window { 
        window_id: u64,
    },
    /// Share rectangular region of an output (future)
    Region {
        connector: String,
        rect: Rectangle<i32, Physical>,
    },
}

/// Identifiable target ID for communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamTargetId {
    Nothing,
    Output { connector: String },
    Window { id: u64 },
    Region { connector: String, x: i32, y: i32, width: i32, height: i32 },
}
```

#### 1.2 Window ID System

**Files:** `src/shell/element.rs`, `src/workspaces/window.rs`

Niri uses `MappedId` with atomic counter. We need similar:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static WINDOW_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

impl WindowId {
    pub fn unique() -> Self {
        Self(WINDOW_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
    }
    
    pub fn get(&self) -> u64 {
        self.0
    }
}
```

Assign to windows at creation:
- `WindowElement::new()` should assign a unique ID
- Store in `WindowElement` or wrapping struct
- Expose via public getter for screenshare queries

#### 1.3 Window Enumeration API

**File:** `src/workspaces/mod.rs`

Add methods to query windows:

```rust
impl Workspaces {
    /// Get all windows across all workspaces
    pub fn all_windows(&self) -> impl Iterator<Item = &WindowElement> {
        self.workspaces.values()
            .flat_map(|ws| ws.windows())
    }
    
    /// Find window by ID
    pub fn find_window(&self, id: WindowId) -> Option<&WindowElement> {
        self.all_windows().find(|w| w.id() == id)
    }
    
    /// Find window by ID with output location
    pub fn find_window_with_output(&self, id: WindowId) 
        -> Option<(&WindowElement, &Output)> {
        // Return window and the output it's currently on
    }
}
```

#### 1.4 Window Metadata for IPC

**File:** `src/workspaces/mod.rs` or new `src/ipc.rs`

Expose window list for portal selection:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub geometry: Rectangle<i32, Physical>,
    pub workspace: Option<String>,
    pub output: Option<String>,
}

impl Workspaces {
    pub fn list_windows(&self) -> Vec<WindowInfo> {
        self.all_windows()
            .map(|w| WindowInfo {
                id: w.id().get(),
                title: w.title().unwrap_or_default(),
                app_id: w.app_id().unwrap_or_default(),
                geometry: w.geometry(),
                // ... workspace/output info
            })
            .collect()
    }
}
```

### Phase 2: Window Rendering for Screencasting

#### 2.1 Window-Specific Render Path

**File:** `src/render.rs` or new `src/screenshare/window_render.rs`

Implement window-only rendering (similar to Niri's `render_for_screen_cast`):

```rust
/// Render a single window with its popups for screencasting
pub fn render_window_for_screencast(
    window: &WindowElement,
    renderer: &mut GlesRenderer,
    scale: Scale<f64>,
) -> Vec<OutputRenderElements> {
    let mut elements = Vec::new();
    
    // Get window bounding box including popups
    let bbox = window.bbox_with_popups();
    
    // Render window surface
    elements.extend(window.render_elements(renderer, bbox.loc, scale));
    
    // Render popups (subsurfaces, xdg_popups, etc.)
    elements.extend(window.render_popup_elements(renderer, scale));
    
    // Optionally render decorations/shadows if enabled
    
    elements
}
```

**Key considerations:**
- Must include all popups/child surfaces
- Handle subsurfaces correctly
- Respect window transformations
- Track damage per window, not per output

#### 2.2 Dynamic Size Tracking

Windows resize dynamically. Track changes:

```rust
pub struct WindowStream {
    window_id: WindowId,
    last_size: Size<i32, Physical>,
    output_scale: Scale<f64>,
}

impl WindowStream {
    pub fn check_size_change(&mut self, window: &WindowElement) 
        -> Option<Size<i32, Physical>> {
        let current = window.bbox_with_popups().size;
        if current != self.last_size {
            self.last_size = current;
            Some(current)
        } else {
            None
        }
    }
}
```

When size changes:
- Renegotiate PipeWire format via `pw_stream_update_params`
- Reallocate DMA-BUF buffers
- Update damage tracker

#### 2.3 Per-Window Damage Tracking

**File:** `src/screenshare/session_tap.rs` or new `src/screenshare/window_damage.rs`

Currently damage tracking is per-output. Need per-window:

```rust
pub struct WindowDamageTracker {
    window_id: WindowId,
    /// Damage accumulator in window-local coordinates
    accumulated_damage: Vec<Rectangle<i32, Physical>>,
}

impl WindowDamageTracker {
    pub fn damage_window(&mut self, regions: &[Rectangle<i32, Physical>]) {
        // Accumulate damage in window coordinate space
        self.accumulated_damage.extend(regions);
    }
    
    pub fn take_damage(&mut self) -> Vec<Rectangle<i32, Physical>> {
        std::mem::take(&mut self.accumulated_damage)
    }
}
```

Window damage sources:
- Window commits (`handle_commit` in shell handlers)
- Popup commits
- Subsurface updates
- Decoration changes

### Phase 3: D-Bus API Extensions

#### 3.1 RecordWindow Method

**File:** `src/screenshare/dbus_service.rs`

Add window recording method to Session interface:

```rust
#[zbus::interface(name = "org.screencomposer.ScreenCast.Session")]
impl Session {
    // ... existing methods ...
    
    /// Record a specific window
    async fn record_window(
        &mut self,
        properties: HashMap<String, zbus::zvariant::Value<'_>>,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        // Extract window_id from properties
        let window_id = properties
            .get("window-id")
            .and_then(|v| v.downcast_ref::<u64>())
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("window-id required".into()))?;
        
        // Create stream with StreamTarget::Window
        let stream_id = format!("stream_{}", uuid::Uuid::new_v4());
        let stream_path = format!(
            "/org/screencomposer/ScreenCast/stream/{}",
            stream_id
        );
        
        // Send command to compositor
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.command_sender.send(CompositorCommand::StartWindowRecording {
            session_id: self.session_id.clone(),
            window_id: *window_id,
            stream_id: stream_id.clone(),
            response_tx: tx,
        }).await?;
        
        let node_id = rx.await??;
        
        // Register stream object
        // ...
        
        Ok(stream_path.try_into().unwrap())
    }
}
```

#### 3.2 ListWindows Method

**File:** `src/screenshare/dbus_service.rs`

For portal window selection UI:

```rust
#[zbus::interface(name = "org.screencomposer.ScreenCast")]
impl ScreenCast {
    /// List available windows for selection
    async fn list_windows(&self) -> zbus::fdo::Result<Vec<WindowListEntry>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.command_sender.send(CompositorCommand::ListWindows {
            response_tx: tx,
        }).await?;
        
        Ok(rx.await?)
    }
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct WindowListEntry {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub thumbnail: Option<Vec<u8>>, // PNG thumbnail for UI
}
```

#### 3.3 SelectSource Method (Portal Dynamic Selection)

**File:** `src/screenshare/dbus_service.rs`

For portal-initiated selection flow:

```rust
impl Session {
    /// Select the recording source dynamically (called by portal after user selection)
    async fn select_source(
        &mut self,
        source_type: String, // "monitor", "window", "region"
        source_id: HashMap<String, zbus::zvariant::Value<'_>>,
    ) -> zbus::fdo::Result<()> {
        let target = match source_type.as_str() {
            "monitor" => {
                let connector = source_id.get("connector")
                    .and_then(|v| v.downcast_ref::<String>())
                    .ok_or(zbus::fdo::Error::InvalidArgs("connector required".into()))?;
                StreamTarget::Output { connector: connector.clone() }
            }
            "window" => {
                let id = source_id.get("window-id")
                    .and_then(|v| v.downcast_ref::<u64>())
                    .ok_or(zbus::fdo::Error::InvalidArgs("window-id required".into()))?;
                StreamTarget::Window { window_id: *id }
            }
            _ => return Err(zbus::fdo::Error::NotSupported("Unknown source type".into())),
        };
        
        // Update stream target
        self.command_sender.send(CompositorCommand::UpdateStreamTarget {
            session_id: self.session_id.clone(),
            target,
        }).await?;
        
        Ok(())
    }
}
```

### Phase 4: Compositor Command Handlers

#### 4.1 Extended CompositorCommand Enum

**File:** `src/screenshare/mod.rs`

```rust
#[derive(Debug)]
pub enum CompositorCommand {
    // ... existing variants ...
    
    /// List all windows for portal selection
    ListWindows {
        response_tx: tokio::sync::oneshot::Sender<Vec<WindowInfo>>,
    },
    
    /// Start recording a specific window
    StartWindowRecording {
        session_id: String,
        window_id: u64,
        stream_id: String,
        response_tx: tokio::sync::oneshot::Sender<Result<u32, String>>,
    },
    
    /// Update dynamic stream target (for portal selection)
    UpdateStreamTarget {
        session_id: String,
        stream_id: String,
        target: StreamTarget,
    },
}
```

#### 4.2 Command Handlers in Main Loop

**File:** Backend files (`src/udev.rs`, `src/winit.rs`, `src/x11.rs`)

```rust
fn handle_screenshare_command(
    state: &mut ScreenComposer<BackendData>,
    command: CompositorCommand,
) {
    match command {
        CompositorCommand::ListWindows { response_tx } => {
            let windows = state.workspaces.list_windows();
            let _ = response_tx.send(windows);
        }
        
        CompositorCommand::StartWindowRecording { 
            session_id, 
            window_id, 
            stream_id,
            response_tx 
        } => {
            // Find window
            let window = state.workspaces.find_window(WindowId(window_id));
            if window.is_none() {
                let _ = response_tx.send(Err("Window not found".into()));
                return;
            }
            
            // Create PipeWire stream for window
            let stream = create_window_stream(window.unwrap(), &state.backend);
            
            // Register frame tap for this window
            let tap_token = state.frame_tap_manager.register_window_tap(
                window_id,
                /* callback */
            );
            
            // Store in session
            // ...
            
            let _ = response_tx.send(Ok(stream.node_id()));
        }
        
        CompositorCommand::UpdateStreamTarget { 
            session_id, 
            stream_id, 
            target 
        } => {
            // Update existing stream's target
            if let Some(session) = state.screencast_sessions.get_mut(&session_id) {
                if let Some(stream) = session.streams.get_mut(&stream_id) {
                    stream.target = target;
                    // May need to resize/recreate PipeWire stream
                }
            }
        }
        
        // ...
    }
}
```

### Phase 5: Frame Capture Integration

#### 5.1 Window Frame Tap

**File:** `src/screenshare/frame_tap.rs`

Extend FrameTapManager to support window taps:

```rust
pub enum TapTarget {
    Output(OutputId),
    Window(WindowId),
}

pub struct FrameTapManager {
    output_taps: HashMap<OutputId, Vec<(FrameTapToken, FrameTap)>>,
    window_taps: HashMap<WindowId, Vec<(FrameTapToken, FrameTap)>>,
    next_token: usize,
}

impl FrameTapManager {
    pub fn register_window_tap(&mut self, window_id: WindowId, tap: FrameTap) 
        -> FrameTapToken {
        let token = FrameTapToken(self.next_token);
        self.next_token += 1;
        self.window_taps
            .entry(window_id)
            .or_insert_with(Vec::new)
            .push((token, tap));
        token
    }
    
    pub fn trigger_window_taps(&mut self, window_id: WindowId, frame: &FrameData) {
        if let Some(taps) = self.window_taps.get_mut(&window_id) {
            for (_, tap) in taps {
                tap.on_frame(frame);
            }
        }
    }
}
```

#### 5.2 Window Render Hook

**File:** `src/render.rs` or backend render loops

After rendering windows for output, trigger window-specific taps:

```rust
fn render_workspace(
    state: &mut ScreenComposer,
    output: &Output,
    renderer: &mut GlesRenderer,
) -> Vec<OutputRenderElements> {
    let elements = /* normal rendering */;
    
    // After rendering, trigger taps for visible windows
    for window in state.workspaces.windows_on_output(output) {
        if state.frame_tap_manager.has_window_tap(window.id()) {
            // Render just this window
            let window_elements = render_window_for_screencast(
                window, 
                renderer, 
                output.current_scale()
            );
            
            // Convert to frame data (dmabuf)
            let frame_data = capture_window_frame(
                renderer,
                &window_elements,
                window.bbox_with_popups(),
            );
            
            state.frame_tap_manager.trigger_window_taps(
                window.id(),
                &frame_data,
            );
        }
    }
    
    elements
}
```

**Performance consideration:** Only render windows that have active taps to avoid overhead.

### Phase 6: Portal Backend Integration

#### 6.1 Update Portal Backend

**File:** `components/xdg-desktop-portal-sc/src/screencast.rs`

The portal backend needs to:
1. Query available windows via `ListWindows`
2. Present selection UI (or delegate to compositor's built-in UI)
3. Call `RecordWindow` with selected window ID
4. Handle restore tokens for remembered selections

```rust
async fn select_sources(
    &self,
    handle: ObjectPath<'_>,
    request_handle: ObjectPath<'_>,
    options: SelectSourcesOptions,
) -> Result<SelectSourcesResult> {
    match options.types {
        AvailableSourceType::Monitor => {
            // Existing monitor selection
        }
        AvailableSourceType::Window => {
            // NEW: Window selection
            let windows = self.backend
                .list_windows()
                .await?;
            
            // Show selection dialog (could be compositor-side)
            let selected = show_window_selection_ui(windows).await?;
            
            // Store for Start call
            self.pending_selections.insert(handle, PendingSelection::Window {
                window_id: selected.id,
            });
        }
        // ...
    }
}

async fn start(
    &self,
    handle: ObjectPath<'_>,
    request_handle: ObjectPath<'_>,
    options: StartOptions,
) -> Result<StartResult> {
    let selection = self.pending_selections.remove(&handle)?;
    
    match selection {
        PendingSelection::Monitor { connector } => {
            self.backend.record_monitor(connector).await?
        }
        PendingSelection::Window { window_id } => {
            self.backend.record_window(window_id).await?
        }
    }
}
```

#### 6.2 Selection UI Options

Three approaches for window selection UI:

**Option A: Compositor Built-In (Like Niri)**
- Implement selection UI in compositor (similar to expose mode)
- Portal triggers compositor-side selection via D-Bus
- Compositor presents overlay with window thumbnails
- Returns selection to portal

**Option B: Portal-Side UI**
- Portal shows GTK dialog with window list
- Uses thumbnails from compositor
- Simpler but less integrated

**Option C: Hybrid**
- Portal requests compositor to show selection UI
- Compositor has optional built-in UI
- Fallback to portal UI if compositor doesn't support it

**Recommended:** Start with **Option B** (simpler), migrate to **Option A** later for better UX.

### Phase 7: Window Selection UI (Compositor-Side)

If implementing compositor-side selection (Option A):

#### 7.1 Window Selection View

**File:** `src/workspaces/window_selector.rs` (new)

```rust
pub struct WindowSelector {
    windows: Vec<SelectableWindow>,
    selected_index: usize,
    layout: WindowSelectorLayout,
}

struct SelectableWindow {
    id: WindowId,
    thumbnail: GlesTexture,
    title: String,
    app_id: String,
    geometry: Rectangle<i32, Physical>,
}

impl WindowSelector {
    pub fn new(
        windows: Vec<WindowInfo>,
        renderer: &mut GlesRenderer,
    ) -> Self {
        // Create thumbnails for each window
        let windows = windows.into_iter()
            .map(|info| {
                let thumbnail = render_window_thumbnail(
                    renderer, 
                    info.id,
                    Size::from((256, 256)),
                );
                SelectableWindow {
                    id: info.id,
                    thumbnail,
                    title: info.title,
                    app_id: info.app_id,
                    geometry: info.geometry,
                }
            })
            .collect();
        
        Self {
            windows,
            selected_index: 0,
            layout: WindowSelectorLayout::Grid { columns: 3 },
        }
    }
    
    pub fn render(&self, renderer: &mut GlesRenderer) 
        -> Vec<WindowSelectorElement> {
        // Grid layout of window thumbnails
        // Highlight selected window
        // Show title/app_id labels
    }
    
    pub fn select_next(&mut self) { /* ... */ }
    pub fn select_prev(&mut self) { /* ... */ }
    pub fn confirm_selection(&self) -> WindowId { /* ... */ }
}
```

#### 7.2 Integration with Input

**File:** `src/input_handler.rs`

```rust
impl ScreenComposer {
    fn handle_window_selection_input(&mut self, event: KeyEvent) {
        if let Some(selector) = &mut self.window_selector {
            match event.keysym {
                Keysym::Left | Keysym::h => selector.select_prev(),
                Keysym::Right | Keysym::l => selector.select_next(),
                Keysym::Return | Keysym::space => {
                    let window_id = selector.confirm_selection();
                    self.complete_window_selection(window_id);
                }
                Keysym::Escape => {
                    self.cancel_window_selection();
                }
                _ => {}
            }
        }
    }
}
```

### Phase 8: Testing & Validation

#### 8.1 Test Cases

**Unit Tests:**
- Window ID uniqueness
- Window enumeration
- Target serialization/deserialization

**Integration Tests:**
1. **Single Window Capture:**
   - Start recording a specific window
   - Verify only that window's content in PipeWire stream
   - Test with different window sizes

2. **Window With Popups:**
   - Open context menu or tooltip
   - Verify popup appears in stream
   - Test popup closing

3. **Window Resize:**
   - Start recording
   - Resize window
   - Verify stream updates correctly
   - Check PipeWire format negotiation

4. **Window Movement:**
   - Record window
   - Move to different workspace/output
   - Verify recording continues

5. **Window Close:**
   - Record window
   - Close window
   - Verify stream stops gracefully

6. **Multiple Window Streams:**
   - Record two different windows simultaneously
   - Verify independent streams
   - Check no cross-contamination

#### 8.2 Test Script

**File:** `scripts/test-window-share.sh`

```bash
#!/bin/bash
# Test window screenshare functionality

# Start compositor
cargo build --release
cargo run --release -- --winit &
COMPOSITOR_PID=$!
sleep 2

# Launch test windows
cargo run --example client-rainbow &
WINDOW1_PID=$!
sleep 1

# Query window list
dbus-send --session --print-reply \
  --dest=org.screencomposer.ScreenCast \
  /org/screencomposer/ScreenCast \
  org.screencomposer.ScreenCast.ListWindows

# Start recording first window
# (manual step or automated via portal)

# Verify PipeWire stream
pw-cli dump short | grep ScreenComposer

# Cleanup
kill $WINDOW1_PID $COMPOSITOR_PID
```

### Phase 9: Performance Optimizations

#### 9.1 Lazy Window Rendering

Only render windows for screencasting when:
- Window has changed (damage)
- Frame time threshold reached
- PipeWire requests a frame

```rust
struct WindowCastState {
    last_render_time: Duration,
    pending_damage: bool,
    min_frame_interval: Duration,
}

impl WindowCastState {
    fn should_render(&self, now: Duration) -> bool {
        self.pending_damage && 
        (now - self.last_render_time) >= self.min_frame_interval
    }
}
```

#### 9.2 Damage Coalescing

Batch multiple window commits into single frame:

```rust
impl WindowDamageTracker {
    fn coalesce_damage(&mut self) -> Vec<Rectangle<i32, Physical>> {
        // Merge overlapping rectangles
        // Return minimal set of damage regions
    }
}
```

#### 9.3 Buffer Reuse

Reuse PipeWire buffers when window size doesn't change:

```rust
struct WindowBufferPool {
    size: Size<i32, Physical>,
    buffers: Vec<Dmabuf>,
    free_list: Vec<usize>,
}
```

## Implementation Timeline

### Milestone 1: Foundation (1 week)
- [ ] Window ID system
- [ ] Window enumeration API
- [ ] StreamTarget enum extension
- [ ] Basic window metadata

### Milestone 2: Rendering (1 week)
- [ ] Window-specific render path
- [ ] Per-window damage tracking
- [ ] Window size change detection
- [ ] Popup handling

### Milestone 3: D-Bus API (1 week)
- [ ] RecordWindow method
- [ ] ListWindows method
- [ ] Command handlers
- [ ] Integration with existing sessions

### Milestone 4: Frame Capture (1 week)
- [ ] Window frame tap registration
- [ ] Render hook integration
- [ ] PipeWire stream for windows
- [ ] Buffer management

### Milestone 5: Portal Integration (1 week)
- [ ] Update portal backend
- [ ] Window selection UI (basic)
- [ ] Testing with OBS/Chrome
- [ ] Restore token support

### Milestone 6: Polish (1 week)
- [ ] Performance optimizations
- [ ] Error handling
- [ ] Documentation
- [ ] Test coverage

**Total Estimated Time:** 6 weeks

## Future Enhancements

### Region Selection
- Similar to Niri's screenshot selection UI
- Freeform rectangular selection on an output
- `StreamTarget::Region { connector, rect }`
- Interactive selection with mouse/keyboard

### Virtual Output Sharing
- Share off-screen virtual workspace
- Useful for presentations
- Independent resolution from physical outputs

### Picture-in-Picture Window
- Compositor-provided PiP window showing active share
- Visual feedback for user
- Quick stop/pause controls

### Cursor Options
- Hidden (current default)
- Embedded (render in stream)
- Metadata (separate cursor updates)

### Window Filters
- Exclude specific windows (e.g., password managers)
- Blur/redact sensitive content
- Automatic detection of sensitive apps

## References

- **Niri Implementation:**
  - `/home/riccardo/dev/niri/src/dbus/mutter_screen_cast.rs` - D-Bus API
  - `/home/riccardo/dev/niri/src/niri.rs` - Render paths (`render_windows_for_screen_cast`)
  - `/home/riccardo/dev/niri/src/pw_utils.rs` - PipeWire integration

- **ScreenComposer Current:**
  - `docs/screenshare.md` - Current architecture
  - `src/screenshare/` - Existing implementation
  - `components/xdg-desktop-portal-sc/` - Portal backend

- **Specifications:**
  - [xdg-desktop-portal ScreenCast](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html)
  - [PipeWire Documentation](https://docs.pipewire.org/)
  - [Mutter ScreenCast D-Bus API](https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-screen-cast.c)
