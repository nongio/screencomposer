mod rendering;
mod wayland_handlers;
mod components;

use components::menu::{Menu, MenuItem, MenuItemId, Position, sc_layer_shell_v1, sc_layer_v1};
use rendering::{SkiaContext, SkiaSurface};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        xdg::{
            popup::{PopupConfigure, PopupHandler},
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell, XdgSurface,
        },
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, globals::registry_queue_init, 
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface}
};
use wayland_cursor::CursorTheme;

fn get_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("file.new", "New Tab").shortcut("⌘T").build(),
        MenuItem::action("file.new_window", "New Window").shortcut("⌘N").build(),
        MenuItem::action("file.new_incognito", "New Incognito Window").shortcut("⇧⌘N").build(),
        MenuItem::separator(),
        MenuItem::action("file.open", "Open File...").shortcut("⌘O").build(),
        MenuItem::separator(),
        MenuItem::submenu("export", "Export")
            .items(vec![
                MenuItem::action("export.pdf", "Export as PDF...").build(),
                MenuItem::action("export.html", "Export as HTML...").build(),
                MenuItem::submenu("export.image", "Export as Image")
                    .items(vec![
                        MenuItem::action("export.png", "PNG Format").build(),
                        MenuItem::action("export.jpeg", "JPEG Format").build(),
                    ])
                    .build(),
            ])
            .build(),
        MenuItem::separator(),
        MenuItem::action("file.close_tab", "Close Tab").shortcut("⌘W").build(),
        MenuItem::action("file.print", "Print...").shortcut("⌘P").build(),
    ]
}

struct AppData {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    xdg_shell_state: XdgShell,
    sc_layer_shell: Option<sc_layer_shell_v1::ScLayerShellV1>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    cursor_theme: Option<CursorTheme>,
    cursor_surface: Option<wl_surface::WlSurface>,
    
    window: Option<Window>,
    configured: bool,
    exit: bool,
    
    // Rendering
    skia_context: Option<SkiaContext>,
    main_surface: Option<SkiaSurface>,
    display_ptr: Option<*mut std::ffi::c_void>,
    
    // Menu component
    menu: Menu,
    pointer_x: f64,
    pointer_y: f64,
    current_surface: Option<wl_surface::WlSurface>,
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
        let mut menu = Menu::new(get_menu_items());
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
            sc_layer_shell: None,
            keyboard: None,
            pointer: None,
            cursor_theme: None,
            cursor_surface: None,
            window: None,
            configured: false,
            exit: false,
            skia_context: None,
            main_surface: None,
            display_ptr: None,
            menu,
            pointer_x: 0.0,
            pointer_y: 0.0,
            current_surface: None,
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
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        println!("Window configure: configured={}, new_size={:?}, serial={}", self.configured, configure.new_size, serial);
        
        // Always acknowledge the configure event
        use smithay_client_toolkit::shell::WaylandSurface;
        window.wl_surface(); // Make sure it's committed
        window.xdg_surface().ack_configure(serial);
        println!("Acknowledged configure with serial {}", serial);
        
        if !self.configured {
            self.configured = true;
            
            let (width, height) = match configure.new_size {
                (Some(w), Some(h)) => (w.get() as i32, h.get() as i32),
                _ => (800, 600),
            };
            
            // Initialize rendering
            let wl_surface = window.wl_surface();
            let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
            
            self.display_ptr = Some(display_ptr);
            
            // Use 2x buffer for HiDPI rendering
            let buffer_scale = 2;
            wl_surface.set_buffer_scale(buffer_scale);
            
            let (ctx, surface) = SkiaContext::new(display_ptr, wl_surface, width * buffer_scale, height * buffer_scale)
                .expect("Failed to create Skia context");
            
            println!("Created Skia context and surface: {}x{} (buffer scale {})", width, height, buffer_scale);
            
            self.skia_context = Some(ctx);
            self.main_surface = Some(surface);

            // Draw simple background
            if let Some(ref mut ctx) = self.skia_context {
                if let Some(ref mut main_surface) = self.main_surface {
                    main_surface.draw(ctx, |canvas| {
                        canvas.clear(skia_safe::Color::from_rgb(246, 246, 246));
                        
                        // Draw text instruction
                        use skia_safe::{Font, FontMgr, Paint};
                        let font_mgr = FontMgr::new();
                        
                        // Use Inter font
                        let font_style = skia_safe::FontStyle::new(
                            skia_safe::font_style::Weight::NORMAL,
                            skia_safe::font_style::Width::NORMAL,
                            skia_safe::font_style::Slant::Upright
                        );
                        let typeface = font_mgr
                            .match_family_style("Inter", font_style)
                            .or_else(|| font_mgr.match_family_style("Inter UI", font_style))
                            .or_else(|| font_mgr.match_family_style("system-ui", font_style))
                            .unwrap_or_else(|| font_mgr.legacy_make_typeface(None, font_style).unwrap());
                        
                        let mut font = Font::from_typeface(typeface, 16.0);
                        font.set_subpixel(true);
                        font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
                        
                        let mut paint = Paint::default();
                        paint.set_color(skia_safe::Color::from_rgb(60, 60, 60));
                        paint.set_anti_alias(true);
                        
                        canvas.draw_str("Right-click to open menu", (20.0, 30.0), &font, &paint);
                    });
                    main_surface.commit();
                    println!("Initial surface committed");
                }
            }
        }
    }
}

impl PopupHandler for AppData {
    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        popup: &smithay_client_toolkit::shell::xdg::popup::Popup,
        config: PopupConfigure,
    ) {
        println!("Popup configure: {:?}", config);
        self.menu.on_configure(popup, config, qh, conn);
        // TODO: Implement menu frame handling
        // self.menu.on_frame(qh);
    }
    
    fn done(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _popup: &smithay_client_toolkit::shell::xdg::popup::Popup,
    ) {
        // Popup dismissed by compositor
        self.menu.hide();
    }
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = Some(seat.get_pointer(qh, ()));
            
            let shm = self.shm_state.wl_shm();
            self.cursor_theme = CursorTheme::load_from_name(conn, shm.clone(), "default", 24).ok();
            self.cursor_surface = Some(self.compositor_state.create_surface(qh));
        }
    }

    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat, _capability: Capability) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}
}

impl ShmHandler for AppData {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event {
            if let wayland_client::WEnum::Value(state_val) = key_state {
                state.menu.on_keyboard_key(key, state_val);
            }
            
            // ESC to exit
            if key == 1 && matches!(key_state, wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed)) {
                state.exit = true;
            }
        }
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface, serial, surface_x, surface_y, .. } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                state.current_surface = Some(surface.clone());
                
                // Set cursor
                if let (Some(cursor_theme), Some(cursor_surface)) = (&mut state.cursor_theme, &state.cursor_surface) {
                    if let Some(cursor) = cursor_theme.get_cursor("default") {
                        let image = &cursor[0];
                        let (w, h) = image.dimensions();
                        let (hx, hy) = image.hotspot();
                        
                        cursor_surface.attach(Some(&image), 0, 0);
                        cursor_surface.damage_buffer(0, 0, w as i32, h as i32);
                        cursor_surface.commit();
                        
                        proxy.set_cursor(serial, Some(cursor_surface), hx as i32, hy as i32);
                    }
                }
                
                // Check if entering menu surface
                if state.menu.owns_surface(&surface) {
                    state.menu.on_pointer_enter(&surface, surface_x, surface_y);
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if state.menu.owns_surface(&surface) {
                    state.menu.on_pointer_leave(&surface);
                }
                // Clear current surface if leaving
                if state.current_surface.as_ref() == Some(&surface) {
                    state.current_surface = None;
                }
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                
                // Only process menu motion if we're over a menu surface
                if let Some(ref current_surface) = state.current_surface {
                    if state.menu.owns_surface(current_surface) {
                        state.menu.on_pointer_motion(current_surface, surface_x, surface_y, qh);
                    }
                }
                
                // Check if we should open a submenu
                if let Some((parent_path, submenu_idx)) = state.menu.should_open_submenu() {
                    println!("Opening submenu at index {} (parent path: {:?})", submenu_idx, parent_path);
                    if let Some(display_ptr) = state.display_ptr {
                        let _ = state.menu.open_submenu(
                            parent_path,
                            submenu_idx,
                            &state.compositor_state,
                            &state.xdg_shell_state,
                            state.sc_layer_shell.as_ref(),
                            qh,
                            display_ptr,
                        );
                    }
                }
                
                // Check if we should close submenus
                if state.menu.should_close_submenus() {
                    state.menu.close_all_submenus();
                }
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                println!("Pointer button: button={}, state={:?}", button, btn_state);
                
                // Right click to open menu
                if button == 273 && matches!(btn_state, wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)) {
                    println!("Right-click detected!");
                    if !state.menu.is_visible() {
                        println!("Menu not visible, showing...");
                        if let Some(window) = &state.window {
                            let position = Position::at_pointer(
                                state.pointer_x as i32,
                                state.pointer_y as i32,
                            );
                            
                            if let Some(display_ptr) = state.display_ptr {
                                if let Err(e) = state.menu.open_menu(
                                    window,
                                    position,
                                    qh,
                                    &state.compositor_state,
                                    &state.xdg_shell_state,
                                    state.sc_layer_shell.as_ref(),
                                    _conn,
                                    display_ptr,
                                ) {
                                    eprintln!("Failed to show menu: {}", e);
                                }
                            }
                        }
                    } else {
                        println!("Menu already visible, hiding...");
                        state.menu.hide();
                    }
                }
                
                if let wayland_client::WEnum::Value(state_val) = btn_state {
                    state.menu.on_pointer_button(button, state_val);
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let xdg_shell_state = XdgShell::bind(&globals, &qh)?;
    let shm_state = Shm::bind(&globals, &qh)?;
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);
    let registry_state = RegistryState::new(&globals);
    
    // Bind sc_layer_shell (optional - graceful degradation if not available)
    let sc_layer_shell = globals.bind::<sc_layer_shell_v1::ScLayerShellV1, _, _>(&qh, 1..=1, ()).ok();
    
    if sc_layer_shell.is_none() {
        eprintln!("Warning: sc_layer_shell_v1 not available, menu will not have blur/shadow effects");
    }

    let mut app_data = AppData::new(
        registry_state,
        seat_state,
        output_state,
        compositor_state,
        shm_state,
        xdg_shell_state,
    );
    
    app_data.sc_layer_shell = sc_layer_shell;

    // Create window
    let surface = app_data.compositor_state.create_surface(&qh);
    let window = app_data.xdg_shell_state.create_window(
        surface,
        WindowDecorations::ServerDefault,
        &qh,
    );
    window.set_title("Menu Component Demo");
    window.set_app_id("hello-design");
    window.set_min_size(Some((400, 300)));
    window.commit();

    app_data.window = Some(window);
    
    event_queue.roundtrip(&mut app_data)?;

    // Event loop
    loop {
        event_queue.blocking_dispatch(&mut app_data)?;
        
        if app_data.exit {
            break;
        }
    }
    
    Ok(())
}
