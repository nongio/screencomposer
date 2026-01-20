use hello_design::{
    components::{
        menu::{
            sc_layer_shell_v1, sc_layer_v1, Anchor, Gravity, Menu, MenuItem, MenuItemId, Position,
        },
        window::SimpleWindow,
    },
    rendering::{SkiaContext, SkiaSurface},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, SurfaceData},
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{pointer::PointerHandler, Capability, SeatHandler, SeatState},
    shell::{
        xdg::{
            popup::{PopupConfigure, PopupData, PopupHandler},
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell, XdgSurface,
        },
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::shell::client::{xdg_popup, xdg_surface};

fn create_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("file.new", "New Tab")
            .shortcut("⌘T")
            .build(),
        MenuItem::action("file.new_window", "New Window")
            .shortcut("⌘N")
            .build(),
        MenuItem::separator(),
        MenuItem::action("file.open", "Open File...")
            .shortcut("⌘O")
            .build(),
        MenuItem::separator(),
        MenuItem::submenu("export", "Export")
            .items(vec![
                MenuItem::action("export.pdf", "Export as PDF...").build(),
                MenuItem::action("export.html", "Export as HTML...").build(),
                MenuItem::submenu("export.image", "Export as Image")
                    .items(vec![
                        MenuItem::action("export.png", "PNG Format").build(),
                        MenuItem::action("export.jpeg", "JPEG Format").build(),
                        MenuItem::action("export.svg", "SVG Format").build(),
                    ])
                    .build(),
            ])
            .build(),
        MenuItem::separator(),
        MenuItem::action("file.close", "Close Tab")
            .shortcut("⌘W")
            .build(),
        MenuItem::action("file.quit", "Quit").shortcut("⌘Q").build(),
    ]
}

struct AppData {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    xdg_shell_state: XdgShell,
    sc_layer_shell_v1: Option<hello_design::components::menu::sc_layer_shell_v1::ScLayerShellV1>,

    window: Option<Window>,
    configured: bool,
    exit: bool,

    // Pointer state
    pointer: Option<wl_pointer::WlPointer>,
    pointer_x: f64,
    pointer_y: f64,

    // Rendering
    skia_context: Option<SkiaContext>,
    main_surface: Option<SkiaSurface>,
    display_ptr: Option<*mut std::ffi::c_void>,

    // Components
    simple_window: SimpleWindow,
    menu: Menu,
}

impl AppData {
    fn new(
        registry_state: RegistryState,
        seat_state: SeatState,
        output_state: OutputState,
        compositor_state: CompositorState,
        shm_state: Shm,
        xdg_shell_state: XdgShell,
        sc_layer_shell_v1: Option<
            hello_design::components::menu::sc_layer_shell_v1::ScLayerShellV1,
        >,
    ) -> Self {
        let simple_window = SimpleWindow::new(800, 600)
            .with_title("Simple Menu Example - Right click to open menu")
            .with_background(skia_safe::Color::from_rgb(240, 240, 245));

        let mut menu = Menu::new(create_menu_items());
        menu.set_on_click(|id: &MenuItemId| {
            println!("Menu item clicked: {}", id);
        });

        Self {
            registry_state,
            seat_state,
            output_state,
            compositor_state,
            shm_state,
            xdg_shell_state,
            sc_layer_shell_v1,
            window: None,
            configured: false,
            exit: false,
            pointer: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            skia_context: None,
            main_surface: None,
            display_ptr: None,
            simple_window,
            menu,
        }
    }

    fn render_window(&mut self) {
        if let Some(ref mut ctx) = self.skia_context {
            if let Some(ref mut main_surface) = self.main_surface {
                main_surface.draw(ctx, |canvas| {
                    // Draw the window background
                    self.simple_window.render(canvas);

                    // Draw instruction text
                    use skia_safe::{Font, FontMgr, Paint, TextBlob};
                    let font_mgr = FontMgr::new();
                    let typeface = font_mgr
                        .match_family_style("Inter", skia_safe::FontStyle::normal())
                        .or_else(|| font_mgr.match_family_style("", skia_safe::FontStyle::normal()))
                        .expect("Failed to load font");

                    let font = Font::from_typeface(typeface, 18.0);
                    let mut paint = Paint::default();
                    paint.set_color(skia_safe::Color::from_rgb(60, 60, 60));
                    paint.set_anti_alias(true);

                    let text = "Right-click anywhere to open the menu";
                    let blob = TextBlob::new(text, &font).expect("Failed to create text blob");
                    canvas.draw_text_blob(&blob, (50.0, 50.0), &paint);

                    // Show menu state
                    let status_text = if self.menu.is_visible() {
                        "Menu is open"
                    } else {
                        "Menu is closed"
                    };
                    let status_blob =
                        TextBlob::new(status_text, &font).expect("Failed to create text blob");
                    canvas.draw_text_blob(&status_blob, (50.0, 90.0), &paint);
                });
                main_surface.commit();
            }
        }
    }
}

impl CompositorHandler for AppData {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for AppData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for AppData {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        println!(
            "Window configure: configured={}, new_size={:?}, serial={}",
            self.configured, configure.new_size, serial
        );

        if !self.configured {
            self.configured = true;

            let (width, height) = match configure.new_size {
                (Some(w), Some(h)) => (w.get() as i32, h.get() as i32),
                _ => (self.simple_window.width(), self.simple_window.height()),
            };

            let wl_surface = window.wl_surface();
            let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;

            self.display_ptr = Some(display_ptr);

            let buffer_scale = 2;
            wl_surface.set_buffer_scale(buffer_scale);

            let (ctx, surface) = SkiaContext::new(
                display_ptr,
                wl_surface,
                width * buffer_scale,
                height * buffer_scale,
            )
            .expect("Failed to create Skia context");

            println!("Created Skia context and surface");

            self.skia_context = Some(ctx);
            self.main_surface = Some(surface);

            self.render_window();
        }
    }
}

impl PopupHandler for AppData {
    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        popup: &smithay_client_toolkit::shell::xdg::popup::Popup,
        configure: PopupConfigure,
    ) {
        self.menu.on_configure(popup, configure, qh, conn);
    }

    fn done(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _popup: &smithay_client_toolkit::shell::xdg::popup::Popup,
    ) {
        // Popup dismissed
    }
}

impl PointerHandler for AppData {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use smithay_client_toolkit::seat::pointer::PointerEventKind;

        for event in events {
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    self.menu
                        .on_pointer_enter(&event.surface, event.position.0, event.position.1);
                }
                PointerEventKind::Leave { .. } => {
                    self.menu.on_pointer_leave(&event.surface);
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_x = event.position.0;
                    self.pointer_y = event.position.1;
                    self.menu.on_pointer_motion(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                        qh,
                    );

                    // Check if we should open submenus
                    if let Some((parent_path, item_index)) = self.menu.should_open_submenu() {
                        if let Some(display_ptr) = self.display_ptr {
                            let _ = self.menu.open_submenu(
                                parent_path,
                                item_index,
                                &self.compositor_state,
                                &self.xdg_shell_state,
                                self.sc_layer_shell_v1.as_ref(),
                                qh,
                                display_ptr,
                            );
                        }
                    }

                    // Check if we should close submenus
                    if self.menu.should_close_submenus() {
                        self.menu.close_all_submenus();
                    }
                }
                PointerEventKind::Press { button, .. } => {
                    // Right click (button 273) to open menu
                    if *button == 273 && !self.menu.is_visible() {
                        if let (Some(window), Some(display_ptr)) = (&self.window, self.display_ptr)
                        {
                            let position = Position {
                                x: self.pointer_x as i32,
                                y: self.pointer_y as i32,
                                anchor: Anchor::TopLeft,
                                gravity: Gravity::BottomRight,
                            };

                            match self.menu.open_menu(
                                window,
                                position,
                                qh,
                                &self.compositor_state,
                                &self.xdg_shell_state,
                                self.sc_layer_shell_v1.as_ref(),
                                conn,
                                display_ptr,
                            ) {
                                Ok(_) => {
                                    println!(
                                        "Menu opened at ({}, {})",
                                        self.pointer_x, self.pointer_y
                                    );
                                    self.render_window();
                                }
                                Err(e) => eprintln!("Failed to open menu: {}", e),
                            }
                        }
                    } else {
                        // Handle clicks on menu items
                        self.menu
                            .on_pointer_button(*button, wl_pointer::ButtonState::Pressed);
                        self.render_window();
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    self.menu
                        .on_pointer_button(*button, wl_pointer::ButtonState::Released);
                }
                PointerEventKind::Axis { .. } => {}
            }
        }
    }
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(_qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
    }
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

// SC Layer protocol handlers
impl Dispatch<sc_layer_shell_v1::ScLayerShellV1, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &sc_layer_shell_v1::ScLayerShellV1,
        _event: sc_layer_shell_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<sc_layer_v1::ScLayerV1, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &sc_layer_v1::ScLayerV1,
        _event: sc_layer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(AppData: ignore wl_keyboard::WlKeyboard);

smithay_client_toolkit::delegate_compositor!(AppData);
smithay_client_toolkit::delegate_output!(AppData);
smithay_client_toolkit::delegate_shm!(AppData);
smithay_client_toolkit::delegate_seat!(AppData);
smithay_client_toolkit::delegate_xdg_shell!(AppData);
smithay_client_toolkit::delegate_xdg_window!(AppData);
smithay_client_toolkit::delegate_xdg_popup!(AppData);
smithay_client_toolkit::delegate_registry!(AppData);
smithay_client_toolkit::delegate_pointer!(AppData);

fn main() {
    println!("Starting simple menu example...");

    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
    let (globals, mut event_queue) =
        registry_queue_init(&conn).expect("Failed to initialize registry");
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_shell not available");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm not available");
    let sc_layer_shell_v1 = globals
        .bind::<sc_layer_shell_v1::ScLayerShellV1, _, _>(&qh, 1..=1, ())
        .ok();

    if sc_layer_shell_v1.is_none() {
        eprintln!(
            "Warning: sc_layer_shell_v1 not available, menu will not have blur/shadow effects"
        );
    }

    let mut app = AppData::new(
        RegistryState::new(&globals),
        SeatState::new(&globals, &qh),
        OutputState::new(&globals, &qh),
        compositor,
        shm,
        xdg_shell,
        sc_layer_shell_v1,
    );

    // Create the window
    let surface = app.compositor_state.create_surface(&qh);
    let window = app
        .xdg_shell_state
        .create_window(surface, WindowDecorations::RequestServer, &qh);

    window.set_title(app.simple_window.title());
    window.set_app_id("simple-menu-example");
    window.set_min_size(Some((400, 300)));
    window.commit();

    app.window = Some(window);

    println!("Window created, waiting for initial configure...");

    event_queue
        .roundtrip(&mut app)
        .expect("Failed to perform initial roundtrip");

    println!("Entering event loop...");
    println!("Right-click anywhere in the window to open the menu");

    loop {
        event_queue
            .blocking_dispatch(&mut app)
            .expect("Event queue dispatch failed");

        if app.exit {
            println!("Exiting...");
            break;
        }
    }
}
