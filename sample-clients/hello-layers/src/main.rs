use wayland_client::{
    protocol::{wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, QueueHandle,
};

use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// Include generated protocol code
mod sc_layer_protocol {
    use wayland_backend;
    use wayland_client;

    pub use wayland_client::protocol::{__interfaces::*, wl_surface};

    wayland_scanner::generate_interfaces!("../../protocols/sc-layer-v1.xml");
    wayland_scanner::generate_client_code!("../../protocols/sc-layer-v1.xml");
}

use sc_layer_protocol::{sc_layer_shell_v1, sc_layer_v1, sc_transaction_v1};

struct AppState {
    compositor: Option<wl_compositor::WlCompositor>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    sc_layer_shell: Option<sc_layer_shell_v1::ScLayerShellV1>,
    running: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        version.min(6),
                        qh,
                        (),
                    ));
                }
                "xdg_wm_base" => {
                    state.xdg_wm_base = Some(registry.bind::<xdg_wm_base::XdgWmBase, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    ));
                }
                "wl_shm" => {
                    state.shm =
                        Some(registry.bind::<wl_shm::WlShm, _, _>(name, version.min(1), qh, ()));
                }
                "sc_layer_shell_v1" => {
                    state.sc_layer_shell =
                        Some(registry.bind::<sc_layer_shell_v1::ScLayerShellV1, _, _>(
                            name,
                            version.min(1),
                            qh,
                            (),
                        ));
                    println!("Found sc_layer_shell_v1 interface!");
                }
                _ => {}
            }
        }
    }
}

// Empty dispatch impls for protocol objects we don't handle events for
impl Dispatch<wl_compositor::WlCompositor, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for AppState {
    fn event(
        _: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        _: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<sc_transaction_v1::ScTransactionV1, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &sc_transaction_v1::ScTransactionV1,
        _: sc_transaction_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<sc_layer_shell_v1::ScLayerShellV1, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &sc_layer_shell_v1::ScLayerShellV1,
        _: sc_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<sc_layer_v1::ScLayerV1, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &sc_layer_v1::ScLayerV1,
        _: sc_layer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn main() {
    println!("Hello Layers");
    println!("============");

    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");

    let mut state = AppState {
        compositor: None,
        xdg_wm_base: None,
        shm: None,
        sc_layer_shell: None,
        running: true,
    };

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qh, ());

    // Round-trip to get globals
    event_queue
        .roundtrip(&mut state)
        .expect("Failed initial roundtrip");

    if state.compositor.is_none() {
        eprintln!("Error: wl_compositor not available!");
        return;
    }

    if state.xdg_wm_base.is_none() {
        eprintln!("Error: xdg_wm_base not available!");
        return;
    }

    if state.sc_layer_shell.is_none() {
        eprintln!("Error: sc_layer_shell_v1 not available!");
        eprintln!("Make sure you're running this on ScreenComposer with the protocol enabled.");
        return;
    }

    if state.shm.is_none() {
        eprintln!("Error: wl_shm not available!");
        return;
    }

    println!("✓ Found all required protocols");

    // Create a window surface
    println!("\nCreating window...");
    let window_surface = {
        use std::os::unix::io::AsFd;

        let compositor = state.compositor.as_ref().unwrap();
        let xdg_wm_base = state.xdg_wm_base.as_ref().unwrap();
        let shm = state.shm.as_ref().unwrap();

        let window_surface = compositor.create_surface(&qh, ());
        let xdg_surface = xdg_wm_base.get_xdg_surface(&window_surface, &qh, ());
        let toplevel = xdg_surface.get_toplevel(&qh, ());

        toplevel.set_title("Hello Layers".to_string());
        toplevel.set_app_id("hello-layers".to_string());

        // Create a visible buffer (800x600 with semi-transparent background)
        let width = 800;
        let height = 600;
        let stride = width * 4; // ARGB8888
        let size = stride * height;

        // Create shared memory file
        let tmpfile = tempfile::tempfile().expect("Failed to create temp file");
        tmpfile
            .set_len(size as u64)
            .expect("Failed to set file size");

        // Map the file and fill with semi-transparent gray
        use std::io::{Seek, Write};
        use std::os::unix::fs::FileExt;
        let mut pixels = vec![0u8; size];
        for i in 0..(width * height) {
            let offset = i * 4;
            pixels[offset] = 40; // Blue
            pixels[offset + 1] = 40; // Green
            pixels[offset + 2] = 40; // Red
            pixels[offset + 3] = 128; // Alpha (semi-transparent)
        }
        tmpfile
            .write_all_at(&pixels, 0)
            .expect("Failed to write buffer");

        let pool = shm.create_pool(tmpfile.as_fd(), size as i32, &qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );

        // Attach buffer to make the surface valid
        window_surface.attach(Some(&buffer), 0, 0);
        window_surface.commit();
        window_surface
    };

    event_queue
        .roundtrip(&mut state)
        .expect("Failed to create window");

    println!("✓ Created window");

    // Now create layers to augment this window
    println!("\nCreating layers to augment window...");

    let (layer1, layer2, layer3) = {
        let sc_shell = state.sc_layer_shell.as_ref().unwrap();

        // Layer 1: Blue background layer
        let layer1 = sc_shell.get_layer(&window_surface, &qh, ());

        layer1.set_position(100.0, 100.0);
        layer1.set_size(400.0, 300.0);
        layer1.set_background_color(
            0.2, // red
            0.4, // green
            0.8, // blue
            1.0, // alpha
        );
        layer1.set_corner_radius(20.0);
        layer1.set_opacity(1.0);

        println!("✓ Layer 1: Blue rounded rectangle at (100, 100), size 400x300");

        // Layer 2: Red sublayer of layer1
        let layer2 = sc_shell.get_layer(&window_surface, &qh, ());

        layer2.set_position(50.0, 50.0);
        layer2.set_size(200.0, 150.0);
        layer2.set_background_color(
            0.9, // red
            0.2, // green
            0.2, // blue
            1.0, // alpha
        );
        layer2.set_corner_radius(10.0);
        layer2.set_opacity(0.8);

        println!("✓ Layer 2: Red rounded rectangle at (50, 50), size 200x150");

        // Add layer2 as sublayer of layer1
        layer1.add_sublayer(&layer2);
        println!("✓ Added Layer 2 as sublayer of Layer 1");

        // Layer 3: Green layer
        let layer3 = sc_shell.get_layer(&window_surface, &qh, ());

        layer3.set_position(600.0, 200.0);
        layer3.set_size(150.0, 150.0);
        layer3.set_background_color(
            0.2, // red
            0.8, // green
            0.2, // blue
            1.0, // alpha
        );
        layer3.set_corner_radius(75.0); // Make it circular
        layer3.set_opacity(1.0);
        layer3.set_hidden(0); // Initially visible

        println!("✓ Layer 3: Green circle at (600, 200), size 150x150");

        (layer1, layer2, layer3)
    };

    event_queue.roundtrip(&mut state).expect("Failed roundtrip");

    println!("\nLayers created! You should see:");
    println!("  - A window with a blue rounded rectangle");
    println!("  - A red rectangle inside the blue one");
    println!("  - A green circle to the right");
    println!("\nStarting animations in 2 seconds...");

    // Test animating multiple layers together
    std::thread::sleep(std::time::Duration::from_secs(2));
    println!("\nAnimating all 3 layers together:");
    println!("  - Layer 1: Moving down and changing opacity");
    println!("  - Layer 2: Scaling up");
    println!("  - Layer 3: Moving left and fading out");

    // Create single transaction for all animations
    {
        let sc_shell = state.sc_layer_shell.as_ref().unwrap();
        let transaction = sc_shell.begin_transaction(&qh, ());
        transaction.set_duration(1.0); // 1 second animation

        // Animate layer 1 - move down and fade
        layer1.set_position(100.0, 200.0);
        layer1.set_opacity(0.5);

        // Animate layer 2 - scale up
        layer2.set_size(300.0, 225.0);

        // Animate layer 3 - move left and fade out
        layer3.set_position(400.0, 200.0);
        layer3.set_opacity(0.3);

        transaction.commit();
    }
    event_queue.roundtrip(&mut state).expect("Failed roundtrip");

    std::thread::sleep(std::time::Duration::from_secs(2));
    println!("\nAnimating back to original positions...");

    // Animate back
    {
        let sc_shell = state.sc_layer_shell.as_ref().unwrap();
        let transaction = sc_shell.begin_transaction(&qh, ());
        transaction.set_duration(1.0); // 1 second animation

        // Animate layer 1 back
        layer1.set_position(100.0, 100.0);
        layer1.set_opacity(1.0);

        // Animate layer 2 back
        layer2.set_size(200.0, 150.0);

        // Animate layer 3 back
        layer3.set_position(600.0, 200.0);
        layer3.set_opacity(1.0);

        transaction.commit();
    }
    event_queue.roundtrip(&mut state).expect("Failed roundtrip");

    println!("\nTest complete! Keeping client running...");

    // Keep running to see the layers
    loop {
        event_queue
            .blocking_dispatch(&mut state)
            .expect("Event loop failed");
    }
}
