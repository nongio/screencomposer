use hello_design::{rendering::{SkiaContext, SkiaSurface}, components::window::SimpleWindow};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell, XdgSurface,
        },
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    Connection, QueueHandle, globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface}
};

struct AppData {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    xdg_shell_state: XdgShell,
    
    window: Option<Window>,
    configured: bool,
    exit: bool,
    
    // Rendering
    skia_context: Option<SkiaContext>,
    main_surface: Option<SkiaSurface>,
    
    // Simple window component
    simple_window: SimpleWindow,
}

impl AppData {
    fn new(
        registry_state: RegistryState,
        seat_state: SeatState,
        output_state: OutputState,
        compositor_state: CompositorState,
        shm_state: Shm,
        xdg_shell_state: XdgShell,
    ) -> Self {
        let simple_window = SimpleWindow::new(800, 600)
            .with_title("Simple Window Example")
            .with_background(skia_safe::Color::from_rgb(255, 255, 255));
        
        Self {
            registry_state,
            seat_state,
            output_state,
            compositor_state,
            shm_state,
            xdg_shell_state,
            window: None,
            configured: false,
            exit: false,
            skia_context: None,
            main_surface: None,
            simple_window,
        }
    }
}

impl CompositorHandler for AppData {
    fn scale_factor_changed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &wl_surface::WlSurface, _new_factor: i32) {}
    fn frame(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &wl_surface::WlSurface, _time: u32) {}
    fn transform_changed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &wl_surface::WlSurface, _new_transform: wl_output::Transform) {}
    fn surface_enter(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &wl_surface::WlSurface, _output: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &wl_surface::WlSurface, _output: &wl_output::WlOutput) {}
}

impl OutputHandler for AppData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl WindowHandler for AppData {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        println!("Window configure: configured={}, new_size={:?}, serial={}", 
                 self.configured, configure.new_size, serial);
                
        if !self.configured {
            self.configured = true;
            
            let (width, height) = match configure.new_size {
                (Some(w), Some(h)) => (w.get() as i32, h.get() as i32),
                _ => (self.simple_window.width(), self.simple_window.height()),
            };
            
            println!("Using dimensions: {}x{}", width, height);
            
            // Initialize rendering
            let wl_surface = window.wl_surface();
            let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
            
            // Use 2x buffer for HiDPI rendering (matches the 2x scale in surface.rs)
            let buffer_scale = 2;
            wl_surface.set_buffer_scale(buffer_scale);
            println!("Set buffer scale to {}", buffer_scale);
            
            let (ctx, surface) = SkiaContext::new(display_ptr, wl_surface, width * buffer_scale, height * buffer_scale)
                .expect("Failed to create Skia context");
            
            println!("Created Skia context and surface");
            
            self.skia_context = Some(ctx);
            self.main_surface = Some(surface);

            // Initial render using the simple window component
            if let Some(ref mut ctx) = self.skia_context {
                if let Some(ref mut main_surface) = self.main_surface {
                    println!("Starting initial render...");
                    main_surface.draw(ctx, |canvas| {
                        self.simple_window.render(canvas);
                    });
                    println!("Committing surface...");
                    main_surface.commit();
                    println!("Surface committed");
                }
            }
        }
    }
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat, _capability: Capability) {}
    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat, _capability: Capability) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}
}

impl ShmHandler for AppData {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

wayland_client::delegate_noop!(AppData: ignore wl_keyboard::WlKeyboard);
wayland_client::delegate_noop!(AppData: ignore wl_pointer::WlPointer);

smithay_client_toolkit::delegate_compositor!(AppData);
smithay_client_toolkit::delegate_output!(AppData);
smithay_client_toolkit::delegate_shm!(AppData);
smithay_client_toolkit::delegate_seat!(AppData);
smithay_client_toolkit::delegate_xdg_shell!(AppData);
smithay_client_toolkit::delegate_xdg_window!(AppData);
smithay_client_toolkit::delegate_registry!(AppData);

fn main() {
    println!("Starting simple window example...");

    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
    let (globals, mut event_queue) = registry_queue_init(&conn).expect("Failed to initialize registry");
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_shell not available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm not available");

    let mut app = AppData::new(
        RegistryState::new(&globals),
        SeatState::new(&globals, &qh),
        OutputState::new(&globals, &qh),
        compositor,
        shm,
        xdg_shell,
    );

    // Create the window with no decorations
    let surface = app.compositor_state.create_surface(&qh);
    let window = app.xdg_shell_state.create_window(
        surface,
        WindowDecorations::None,
        &qh,
    );
    
    window.set_title(app.simple_window.title());
    window.set_app_id("simple-window-example");
    window.set_min_size(Some((400, 300)));
    window.commit();
    
    app.window = Some(window);

    println!("Window created, waiting for initial configure...");
    
    // Perform initial roundtrip to receive the configure event
    event_queue.roundtrip(&mut app).expect("Failed to perform initial roundtrip");

    println!("Entering event loop...");

    loop {
        event_queue.blocking_dispatch(&mut app).expect("Event queue dispatch failed");
        
        if app.exit {
            println!("Exiting...");
            break;
        }
    }
}
