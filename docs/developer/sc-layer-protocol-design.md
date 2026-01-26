# Screen Composer Layer Protocol Design

## Overview

The `sc_layer_shell` protocol provides Wayland clients with powerful animation and compositing capabilities by exposing the lay-rs engine's features. This allows clients to create rich, GPU-accelerated UIs with declarative animations that run entirely on the compositor.

## Design Philosophy

### Core Principles

1. **Declarative API**: Clients declare what they want (end state + timing), compositor handles execution
2. **Transaction-based**: Group property changes that should animate together
3. **Server-side Animation**: All animation runs on the compositor, reducing IPC overhead
4. **Implicit Animations**: Property changes automatically animate when inside a transaction
5. **Spring Physics**: Natural, interruptible animations using spring dynamics

### Protocol Architecture

| Client API | Compositor State | lay-rs Backend |
|------------|------------------|----------------|
| sc_layer_surface_v1 | ScLayerSurface | Layer |
| sc_transaction_v1 | PendingTransaction | TransactionRef |
| sc_timing_function_v1 | TimingFunctionState | TimingFunction |
| - | Spring configuration | Spring |

## Recommended Protocol Structure

### 1. Transaction-Based Model

Property changes are grouped into transactions that execute atomically:

```xml
<interface name="sc_transaction_v1" version="1">
  <description>
    A transaction groups property changes that should animate together.
    Changes made within a transaction are batched and applied atomically.
  </description>
  
  <request name="begin">
    <description>Begin a new transaction context</description>
  </request>
  
  <request name="commit">
    <description>Commit all pending changes and start animations</description>
  </request>
  
  <request name="set_duration">
    <arg name="duration" type="fixed" summary="animation duration in seconds"/>
  </request>
  
  <request name="set_timing_function">
    <arg name="timing" type="object" interface="sc_timing_function_v1"/>
  </request>
  
  <request name="set_completion_callback">
    <description>Request notification when transaction completes</description>
  </request>
  
  <event name="completed">
    <description>Fired when transaction animation completes</description>
  </event>
</interface>
```

### 2. Timing Functions (Aligned with lay-rs)

```xml
<interface name="sc_timing_function_v1" version="1">
  <enum name="preset">
    <entry name="linear" value="0"/>
    <entry name="ease_in" value="1"/>
    <entry name="ease_out" value="2"/>
    <entry name="ease_in_out" value="3"/>
  </enum>
  
  <request name="set_bezier">
    <description>Custom cubic bezier curve</description>
    <arg name="c1x" type="fixed"/>
    <arg name="c1y" type="fixed"/>
    <arg name="c2x" type="fixed"/>
    <arg name="c2y" type="fixed"/>
  </request>
  
  <request name="set_preset">
    <arg name="preset" type="uint" enum="preset"/>
  </request>
  
  <request name="set_spring">
    <description>Spring-based timing with natural physics</description>
    <arg name="duration" type="fixed" summary="approximate duration"/>
    <arg name="bounce" type="fixed" summary="bounciness factor (0.0-1.0)"/>
    <arg name="velocity" type="fixed" summary="initial velocity (optional)"/>
  </request>
</interface>
```

### 3. Enhanced Layer Surface Properties

```xml
<interface name="sc_layer_surface_v1" version="1">
  <!-- Transform Properties (Animatable) -->
  <request name="set_position">
    <arg name="x" type="fixed"/>
    <arg name="y" type="fixed"/>
  </request>
  
  <request name="set_scale">
    <arg name="x" type="fixed"/>
    <arg name="y" type="fixed"/>
  </request>
  
  <request name="set_rotation">
    <description>Rotation in radians</description>
    <arg name="angle" type="fixed"/>
  </request>
  
  <request name="set_anchor_point">
    <description>Transform origin point (0.0-1.0 normalized)</description>
    <arg name="x" type="fixed"/>
    <arg name="y" type="fixed"/>
  </request>
  
  <!-- Appearance Properties (Animatable) -->
  <request name="set_opacity">
    <arg name="opacity" type="fixed" summary="0.0 to 1.0"/>
  </request>
  
  <request name="set_corner_radius">
    <arg name="radius" type="fixed"/>
  </request>
  
  <request name="set_background_color">
    <arg name="red" type="fixed" summary="0.0 to 1.0"/>
    <arg name="green" type="fixed" summary="0.0 to 1.0"/>
    <arg name="blue" type="fixed" summary="0.0 to 1.0"/>
    <arg name="alpha" type="fixed" summary="0.0 to 1.0"/>
  </request>
  
  <!-- Border Properties -->
  <request name="set_border">
    <arg name="width" type="fixed"/>
    <arg name="red" type="fixed"/>
    <arg name="green" type="fixed"/>
    <arg name="blue" type="fixed"/>
    <arg name="alpha" type="fixed"/>
  </request>
  
  <!-- Shadow Properties (Animatable) -->
  <request name="set_shadow">
    <arg name="opacity" type="fixed"/>
    <arg name="radius" type="fixed"/>
    <arg name="offset_x" type="fixed"/>
    <arg name="offset_y" type="fixed"/>
    <arg name="red" type="fixed"/>
    <arg name="green" type="fixed"/>
    <arg name="blue" type="fixed"/>
  </request>
  
  <!-- Hierarchy -->
  <request name="add_sublayer">
    <arg name="sublayer" type="object" interface="sc_layer_surface_v1"/>
  </request>
  
  <request name="insert_sublayer">
    <arg name="sublayer" type="object" interface="sc_layer_surface_v1"/>
    <arg name="index" type="int"/>
  </request>
  
  <request name="remove_from_superlayer"/>
  
  <!-- Filters -->
  <request name="set_compositing_filter">
    <arg name="filter" type="object" interface="sc_filter_v1" allow-null="true"/>
  </request>
</interface>
```

### 4. Compositing Filters

```xml
<interface name="sc_filter_v1" version="1">
  <enum name="type">
    <entry name="blur" value="0"/>
    <entry name="brightness" value="1"/>
    <entry name="contrast" value="2"/>
    <entry name="saturation" value="3"/>
    <entry name="multiply" value="4"/>
    <entry name="screen" value="5"/>
  </enum>
  
  <request name="set_type">
    <arg name="type" type="uint" enum="type"/>
  </request>
  
  <request name="set_parameter">
    <arg name="key" type="string"/>
    <arg name="value" type="fixed"/>
  </request>
</interface>
```

## Usage Examples

### Basic Animation

```c
// Client code example
sc_transaction_v1 *tx = sc_shell_begin_transaction(shell);
sc_transaction_set_duration(tx, wl_fixed_from_double(0.3));
sc_timing_function_v1 *timing = sc_shell_get_timing_function(shell);
sc_timing_function_set_preset(timing, SC_TIMING_EASE_OUT);
sc_transaction_set_timing_function(tx, timing);

// These changes will animate
sc_layer_surface_set_position(layer, 
    wl_fixed_from_int(100), 
    wl_fixed_from_int(200));
sc_layer_surface_set_opacity(layer, wl_fixed_from_double(0.5));

sc_transaction_commit(tx);
```

### Spring Animation

```c
sc_transaction_v1 *tx = sc_shell_begin_transaction(shell);
sc_timing_function_v1 *spring = sc_shell_get_timing_function(shell);
sc_timing_function_set_spring(spring,
    wl_fixed_from_double(0.5),  // duration
    wl_fixed_from_double(0.3),  // bounce
    wl_fixed_from_double(0.0)); // velocity
sc_transaction_set_timing_function(tx, spring);

sc_layer_surface_set_scale(layer, 
    wl_fixed_from_double(1.2), 
    wl_fixed_from_double(1.2));

sc_transaction_commit(tx);
```

### Gesture-Driven Animation

```c
// During gesture
sc_layer_surface_set_position(layer, x, y);  // Immediate

// On gesture end with velocity
sc_transaction_v1 *tx = sc_shell_begin_transaction(shell);
sc_timing_function_v1 *spring = sc_shell_get_timing_function(shell);
sc_timing_function_set_spring(spring,
    wl_fixed_from_double(0.3),
    wl_fixed_from_double(0.1),
    wl_fixed_from_double(velocity)); // Use gesture velocity!
sc_transaction_set_timing_function(tx, spring);

sc_layer_surface_set_position(layer, target_x, target_y);
sc_transaction_commit(tx);
```

## Implementation Mapping

### Compositor Side (Rust)

```rust
// When client commits transaction with timing
fn handle_transaction_commit(&mut self, tx: &ScTransaction) {
    let timing = tx.timing_function.map(|tf| {
        match tf.type {
            TimingType::Bezier(c1x, c1y, c2x, c2y) => {
                TimingFunction::Bezier(c1x, c1y, c2x, c2y)
            }
            TimingType::Spring { duration, bounce, velocity } => {
                let spring = Spring::with_duration_bounce_and_velocity(
                    duration, bounce, velocity
                );
                TimingFunction::Spring(spring)
            }
            TimingType::Linear => TimingFunction::Linear,
        }
    });
    
    let transition = Transition {
        delay: tx.delay,
        timing: timing.unwrap_or(TimingFunction::Linear),
    };
    
    // Apply all pending changes with animation
    let changes: Vec<_> = tx.pending_changes.iter()
        .map(|change| match change {
            PendingChange::Position(x, y) => 
                layer.change_position((*x, *y)),
            PendingChange::Scale(x, y) => 
                layer.change_scale((*x, *y)),
            PendingChange::Opacity(opacity) => 
                layer.change_opacity(*opacity),
            // ... etc
        })
        .collect();
    
    let animation = self.engine.add_animation_from_transition(&transition, true);
    let transactions = self.engine.schedule_changes(&changes, animation);
    
    // Track for completion callback
    if tx.wants_completion {
        if let Some(tr) = transactions.first() {
            tr.on_finish(move |_, _| {
                // Send completion event to client
                tx_object.completed();
            }, true);
        }
    }
}
```

## Advanced Features

### 1. Keyframe Animations

For complex animations:

```xml
<interface name="sc_keyframe_animation_v1" version="1">
  <request name="add_keyframe">
    <arg name="time" type="fixed" summary="0.0 to 1.0"/>
    <arg name="value" type="fixed"/>
  </request>
  
  <request name="set_timing_function">
    <arg name="from_keyframe" type="int"/>
    <arg name="to_keyframe" type="int"/>
    <arg name="timing" type="object" interface="sc_timing_function_v1"/>
  </request>
</interface>
```

### 2. Animation Groups

For complex choreography:

```xml
<interface name="sc_animation_group_v1" version="1">
  <request name="add_animation">
    <arg name="animation" type="object" interface="sc_transaction_v1"/>
    <arg name="start_time" type="fixed" summary="relative start time"/>
  </request>
</interface>
```

### 3. Gesture Tracking

For interactive animations:

```xml
<interface name="sc_gesture_recognizer_v1" version="1">
  <request name="link_to_property">
    <arg name="layer" type="object" interface="sc_layer_surface_v1"/>
    <arg name="property" type="string" summary="e.g., 'position.x'"/>
  </request>
  
  <event name="gesture_update">
    <arg name="progress" type="fixed"/>
    <arg name="velocity" type="fixed"/>
  </event>
</interface>
```

## Performance Considerations

1. **Server-side Animation**: All animation state lives in the compositor, reducing IPC
2. **Damage Tracking**: lay-rs already provides efficient damage regions
3. **GPU Acceleration**: Skia backend handles all rendering
4. **Transaction Batching**: Multiple property changes animate in sync
5. **Spring Interruption**: Springs can be interrupted mid-flight with new velocity

## Implementation Priority

### Phase 1: Core (MVP)
- Transaction-based property changes
- Basic timing functions (linear, bezier, spring)
- Transform properties (position, scale, rotation)
- Opacity

### Phase 2: Visual Effects
- Background colors
- Borders and corner radius
- Shadows
- Compositing filters

### Phase 3: Advanced
- Keyframe animations
- Animation groups
- Gesture recognizers
- 3D transforms

## References

- [lay-rs Engine API](https://github.com/nongio/layers)
- Compositor implementation examples in `src/workspaces/mod.rs` (spring animation usage)
- [Wayland Protocol Documentation](https://wayland.freedesktop.org/docs/html/)
