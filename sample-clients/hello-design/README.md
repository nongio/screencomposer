# Hello Design - Wayland Skia Client

A simple Wayland client demonstrating how to create surfaces with Skia-based rendering.

## Architecture

This example uses:
- **smithay-client-toolkit**: High-level Wayland client framework
- **wayland-egl**: Bridge between Wayland surfaces and EGL
- **khronos-egl**: EGL display/context management
- **skia-safe**: 2D graphics rendering with GPU acceleration
- **gl**: OpenGL function loading

## How It Works

1. **Wayland Connection**: Connects to the Wayland compositor using smithay-client-toolkit
2. **Window Creation**: Creates an XDG toplevel window
3. **EGL Setup**: 
   - Gets EGL display from Wayland display pointer
   - Creates OpenGL ES 2.0 context
   - Creates `WlEglSurface` bridging Wayland surface to EGL
   - Creates EGL window surface
4. **Skia Integration**:
   - Loads GL function pointers via EGL
   - Creates Skia GL interface
   - Creates Skia DirectContext for GPU-accelerated rendering
5. **Rendering Loop**:
   - Queries framebuffer info from GL
   - Wraps EGL surface as Skia backend render target
   - Uses Skia canvas API to draw (clear, shapes, text, etc.)
   - Flushes to GPU and swaps buffers

## Running

```bash
cargo build --release -p hello-design
./target/release/hello-design
```

The client will display a window with a rotating blue rectangle rendered using Skia.

## Next Steps

This foundation can be extended to:
1. Create multiple subsurfaces with independent Skia canvases
2. Position and layer subsurfaces
3. Handle input events (keyboard, mouse)
4. Implement more complex UI with the full Skia API
5. Add animations and transitions

## Key Code Pattern

The main pattern for Skia-on-Wayland surfaces:

```rust
// 1. Create Wayland surface
let wl_surface = compositor.create_surface(&qh);

// 2. Get Wayland display pointer for EGL
let display_ptr = conn.backend().display_ptr();

// 3. Initialize EGL with Wayland display
let egl = khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required()?;
let egl_display = egl.get_display(display_ptr as NativeDisplayType)?;

// 4. Create EGL context
let egl_context = egl.create_context(display, config, None, &context_attribs)?;

// 5. Create WlEglSurface bridging Wayland to EGL
let wl_egl_surface = wayland_egl::WlEglSurface::new(wl_surface.id(), width, height)?;

// 6. Create EGL surface
let egl_surface = egl.create_window_surface(display, config, wl_egl_surface.ptr(), None)?;

// 7. Make context current
egl.make_current(display, Some(egl_surface), Some(egl_surface), Some(egl_context))?;

// 8. Load GL and create Skia context
gl::load_with(|name| egl.get_proc_address(name).unwrap() as *const _);
let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
    egl.get_proc_address(name).unwrap() as *const _
})?;
let skia_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)?;

// 9. In render loop: wrap as Skia surface and draw
let backend_rt = skia_safe::gpu::backend_render_targets::make_gl(...);
let surface = skia_safe::gpu::surfaces::wrap_backend_render_target(
    &mut skia_context, &backend_rt, ...
)?;
let canvas = surface.canvas();
// ... draw with canvas ...
skia_context.flush_and_submit();
egl.swap_buffers(display, surface);
```

This pattern can be reused for each subsurface you want to create with its own Skia canvas.
