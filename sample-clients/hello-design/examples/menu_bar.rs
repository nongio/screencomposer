/// Example demonstrating the MenuBar component in a window
/// 
/// This shows how to create a horizontal menu bar with toggleable menu items.
/// Click on menu labels to toggle them open/closed.

use hello_design::{rendering::{SkiaContext, SkiaSurface}, components::menu::{MenuItem, MenuItemId, Position, Anchor, Gravity, sc_layer_shell_v1, sc_layer_v1}, components::menu_bar::{MenuBar, surface::MenuBarSurface}};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    globals::GlobalData,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        xdg::{
            popup::{PopupConfigure, PopupHandler},
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    Connection, QueueHandle, globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_subcompositor, wl_surface, wl_subsurface}
};
use skia_safe::Color;

struct AppData {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    xdg_shell_state: XdgShell,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    sc_layer_shell_v1: Option<sc_layer_shell_v1::ScLayerShellV1>,
    
    window: Option<Window>,
    configured: bool,
    exit: bool,
    
    // Rendering
    skia_context: Option<SkiaContext>,
    main_surface: Option<SkiaSurface>,
    display_ptr: Option<*mut std::ffi::c_void>,
    
    // MenuBar component surface
    menu_bar_surface: Option<MenuBarSurface>,
    pointer_x: f64,
    pointer_y: f64,
    pointer: Option<wl_pointer::WlPointer>,
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
        sc_layer_shell_v1: Option<sc_layer_shell_v1::ScLayerShellV1>,
    ) -> Self {
        Self {
            registry_state,
            seat_state,
            output_state,
            compositor_state,
            shm_state,
            xdg_shell_state,
            subcompositor: None,
            sc_layer_shell_v1,
            window: None,
            configured: false,
            exit: false,
            skia_context: None,
            main_surface: None,
            display_ptr: None,
            menu_bar_surface: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            pointer: None,
            current_surface: None,
        }
    }
    
    fn render(&mut self) {
        if let Some(ref mut ctx) = self.skia_context {
            if let Some(ref mut main_surface) = self.main_surface {
                main_surface.draw(ctx, |canvas| {
                    // Clear background
                    canvas.clear(Color::from_rgb(255, 255, 255));
                    
                    // Draw some instructional text
                    let paint = skia_safe::Paint::new(skia_safe::Color4f::new(0.2, 0.2, 0.2, 1.0), None);
                    let font_mgr = skia_safe::FontMgr::new();
                    let typeface = font_mgr
                        .match_family_style("sans-serif", skia_safe::FontStyle::normal())
                        .unwrap_or_else(|| font_mgr.legacy_make_typeface(None, skia_safe::FontStyle::normal()).unwrap());
                    
                    let font = skia_safe::Font::from_typeface(&typeface, 18.0);
                    let text = "Click on the menu labels above to toggle menus";
                    
                    let x = 20.0;
                    let y = 100.0;
                    
                    canvas.draw_str(text, (x, y), &font, &paint);
                    
                    // Show current state
                    let state_text = if let Some(ref menu_bar_surface) = self.menu_bar_surface {
                        if let Some(active) = menu_bar_surface.menu_bar().active_menu() {
                            format!("Active menu: {}", active)
                        } else {
                            "No menu open".to_string()
                        }
                    } else {
                        "MenuBar not initialized".to_string()
                    };
                    
                    let small_font = skia_safe::Font::from_typeface(&typeface, 14.0);
                    let state_x = 20.0;
                    let state_y = 140.0;
                    
                    canvas.draw_str(&state_text, (state_x, state_y), &small_font, &paint);
                });
                main_surface.commit();
            }
        }
    }
}

impl CompositorHandler for AppData {
    fn scale_factor_changed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &wl_surface::WlSurface, _new_factor: i32) {}
    
    fn frame(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _time: u32) {
        // Route frame callback to active menu
        if let Some(ref mut menu_bar_surface) = self.menu_bar_surface {
            if let Some(active_label) = menu_bar_surface.menu_bar().active_menu() {
                let active_label = active_label.to_string();
                if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&active_label) {
                    menu.on_frame_callback(surface, qh);
                }
            }
        }
    }
    
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
        _serial: u32,
    ) {
        if !self.configured {
            self.configured = true;
            
            let (width, height) = match configure.new_size {
                (Some(w), Some(h)) => (w.get() as i32, h.get() as i32),
                _ => (800, 400),
            };
            
            // Initialize rendering
            let wl_surface = window.wl_surface();
            let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
            self.display_ptr = Some(display_ptr);
            
            let buffer_scale = 2;
            wl_surface.set_buffer_scale(buffer_scale);
            
            let (ctx, surface) = SkiaContext::new(display_ptr, wl_surface, width * buffer_scale, height * buffer_scale)
                .expect("Failed to create Skia context");
            
            self.skia_context = Some(ctx);
            self.main_surface = Some(surface);

            // Create MenuBar component
            let mut menu_bar = MenuBar::new()
                .with_height(28.0)
                .with_background(Color::from_rgb(245, 245, 245))
                .with_text_color(Color::from_rgb(30, 30, 30));
            
            menu_bar.add_item("File", vec![
                MenuItem::action("file.new", "New Tab").shortcut("⌘T").build(),
                MenuItem::action("file.new_window", "New Window").shortcut("⌘N").build(),
                MenuItem::separator(),
                MenuItem::action("file.open", "Open File...").shortcut("⌘O").build(),
                MenuItem::action("file.close", "Close Tab").shortcut("⌘W").build(),
            ]);
            
            menu_bar.add_item("Edit", vec![
                MenuItem::action("edit.undo", "Undo").shortcut("⌘Z").build(),
                MenuItem::action("edit.redo", "Redo").shortcut("⇧⌘Z").build(),
                MenuItem::separator(),
                MenuItem::action("edit.cut", "Cut").shortcut("⌘X").build(),
                MenuItem::action("edit.copy", "Copy").shortcut("⌘C").build(),
                MenuItem::action("edit.paste", "Paste").shortcut("⌘V").build(),
            ]);
            
            menu_bar.add_item("View", vec![
                MenuItem::action("view.fullscreen", "Toggle Fullscreen").shortcut("F11").build(),
                MenuItem::separator(),
                MenuItem::action("view.zoom_in", "Zoom In").shortcut("⌘+").build(),
                MenuItem::action("view.zoom_out", "Zoom Out").shortcut("⌘-").build(),
            ]);
            
            menu_bar.add_item("Help", vec![
                MenuItem::action("help.docs", "Documentation").build(),
                MenuItem::action("help.about", "About").build(),
            ]);
            
            menu_bar.set_on_click(|id: &MenuItemId| {
                println!("Menu item clicked: {}", id);
            });

            // Create MenuBarSurface as a subsurface
            if let Some(ref subcompositor) = self.subcompositor {
                match MenuBarSurface::new(
                    wl_surface,
                    menu_bar,
                    width,
                    &self.compositor_state,
                    subcompositor,
                    qh,
                    display_ptr,
                ) {
                    Ok(menu_bar_surface) => {
                        self.menu_bar_surface = Some(menu_bar_surface);
                        println!("MenuBar subsurface created successfully");
                    }
                    Err(e) => eprintln!("Failed to create MenuBar subsurface: {}", e),
                }
            }

            // Initial render
            self.render();
        }
    }
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = Some(seat.get_pointer(qh, ()));
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

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

wayland_client::delegate_noop!(AppData: ignore wl_subsurface::WlSubsurface);

impl wayland_client::Dispatch<wl_keyboard::WlKeyboard, ()> for AppData {
    fn event(
        app_data: &mut Self,
        _keyboard: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Leave { .. } => {
                // Window lost keyboard focus - close all menus
                if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                    let all_labels = menu_bar_surface.menu_bar().get_menu_labels();
                    for menu_label in &all_labels {
                        if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(menu_label) {
                            if menu.is_visible() {
                                menu.hide();
                            }
                        }
                    }
                    // Clear the active menu state
                    menu_bar_surface.menu_bar_mut().close_all();
                }
            }
            _ => {}
        }
    }
}

impl wayland_client::Dispatch<wl_pointer::WlPointer, ()> for AppData {
    fn event(
        app_data: &mut Self,
        _pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_pointer::Event;
        
        match event {
            Event::Enter { surface, surface_x, surface_y, .. } => {
                app_data.pointer_x = surface_x;
                app_data.pointer_y = surface_y;
                app_data.current_surface = Some(surface.clone());
                
                // Check if entering a menu surface
                if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                    if let Some(active_label) = menu_bar_surface.menu_bar().active_menu() {
                        let active_label = active_label.to_string();
                        if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&active_label) {
                            menu.on_pointer_enter(&surface, surface_x, surface_y);
                        }
                    }
                }
            }
            Event::Leave { surface, .. } => {
                app_data.current_surface = None;
                // Check if leaving a menu surface
                if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                    if let Some(active_label) = menu_bar_surface.menu_bar().active_menu() {
                        let active_label = active_label.to_string();
                        if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&active_label) {
                            menu.on_pointer_leave(&surface);
                        }
                    }
                }
            }
            Event::Motion { surface_x, surface_y, .. } => {
                app_data.pointer_x = surface_x;
                app_data.pointer_y = surface_y;
                
                // Forward motion to active menu ONLY if we're on the menu surface
                if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                    if let Some(active_label) = menu_bar_surface.menu_bar().active_menu() {
                        let active_label_clone = active_label.to_string();
                        if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&active_label_clone) {
                            // Check if current surface belongs to this menu (root or any submenu)
                            let is_menu_surface = app_data.current_surface.as_ref()
                                .map(|s| menu.owns_surface(s))
                                .unwrap_or(false);
                            
                            if is_menu_surface {
                                if let Some(current_surf) = app_data.current_surface.as_ref() {
                                    menu.on_pointer_motion(current_surf, surface_x, surface_y, qh);
                                    
                                    // Check if we should open submenus
                                    if let Some((parent_path, item_index)) = menu.should_open_submenu() {
                                        if let Some(display_ptr) = app_data.display_ptr {
                                            let _ = menu.open_submenu(
                                                parent_path,
                                                item_index,
                                                &app_data.compositor_state,
                                                &app_data.xdg_shell_state,
                                                app_data.sc_layer_shell_v1.as_ref(),
                                                qh,
                                                display_ptr,
                                            );
                                        }
                                    }
                                    
                                    // Check if we should close submenus
                                    if menu.should_close_submenus() {
                                        menu.close_all_submenus();
                                    }
                                }
                            }
                        }
                    }
                }
                    
                // Also handle hover for menu bar (only if on menubar surface)
                let x = surface_x as f32;
                let y = surface_y as f32;
                
                // Check if we're on the menubar subsurface
                let on_menubar = app_data.current_surface.as_ref()
                    .and_then(|surf| app_data.menu_bar_surface.as_ref()
                        .map(|mbs| surf == mbs.surface()))
                    .unwrap_or(false);
                
                if on_menubar {
                    if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                    if let Some((label, x_pos, changed)) = menu_bar_surface.handle_hover(x, y) {
                        // If the menu changed (switched from one to another), update the open menu
                        if changed {
                            if let (Some(ref window), Some(display_ptr)) = (&app_data.window, app_data.display_ptr) {
                                // Capture height before getting mutable reference
                                let menu_y = menu_bar_surface.height as f32;
                                
                                // Get all menu labels to close all menus
                                let all_labels = menu_bar_surface.menu_bar().get_menu_labels();
                                
                                // Hide all menus first
                                for menu_label in &all_labels {
                                    if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(menu_label) {
                                        if menu.is_visible() {
                                            menu.hide();
                                        }
                                    }
                                }
                                
                                // Open the new menu at the correct position
                                if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&label) {
                                    let position = hello_design::components::menu::Position {
                                        x: x_pos as i32,
                                        y: menu_y as i32 + 2,
                                        anchor: hello_design::components::menu::Anchor::TopLeft,
                                        gravity: hello_design::components::menu::Gravity::BottomRight,
                                    };
                                    
                                    let _ = menu.open_menu(
                                        window,
                                        position,
                                        qh,
                                        &app_data.compositor_state,
                                        &app_data.xdg_shell_state,
                                        app_data.sc_layer_shell_v1.as_ref(),
                                        conn,
                                        display_ptr,
                                    );
                                }
                            }
                        }
                    }
                    }
                }
            }
            Event::Button { button, state: button_state, .. } => {
                // Forward button events to active menu first
                if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                    if let Some(active_label) = menu_bar_surface.menu_bar().active_menu() {
                        let active_label = active_label.to_string();
                        if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&active_label) {
                            if let wayland_client::WEnum::Value(state) = button_state {
                                menu.on_pointer_button(button, state);
                            }
                        }
                    }
                }
                
                if button == 0x110 && matches!(button_state, wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)) {
                    let x = app_data.pointer_x as f32;
                    let y = app_data.pointer_y as f32;
                    
                    if let Some(ref mut menu_bar_surface) = app_data.menu_bar_surface {
                        if let Some((label, x_pos)) = menu_bar_surface.handle_click(x, y) {
                            // Get all menu labels first to avoid borrow issues
                            let all_labels = menu_bar_surface.menu_bar().get_menu_labels();
                            
                            // Close all other menus
                            for other_label in &all_labels {
                                if other_label != &label {
                                    if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(other_label) {
                                        menu.hide();
                                    }
                                }
                            }
                            
                            // Check if menu is active AFTER the toggle
                            let is_active = menu_bar_surface.menu_bar().active_menu().is_some();
                            
                            // Open the clicked menu if it's now active, otherwise close it
                            if is_active {
                                if let (Some(window), Some(display_ptr)) = (&app_data.window, app_data.display_ptr) {
                                    if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&label) {
                                        let position = Position {
                                            x: x_pos as i32,
                                            y: 30,
                                            anchor: Anchor::TopLeft,
                                            gravity: Gravity::BottomRight,
                                        };
                                        
                                        let _ = menu.open_menu(
                                            window,
                                            position,
                                            qh,
                                            &app_data.compositor_state,
                                            &app_data.xdg_shell_state,
                                            app_data.sc_layer_shell_v1.as_ref(),
                                            conn,
                                            display_ptr,
                                        );
                                    }
                                }
                            } else {
                                // Menu was toggled off, close it
                                if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&label) {
                                    menu.hide();
                                }
                            }
                            
                            // Don't call app_data.render() here - the MenuBarSurface has already rendered itself
                            // and calling render() on the main window might conflict with EGL context
                        }
                    }
                }
            }
            _ => {}
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
        // Forward to menus
        if let Some(ref mut menu_bar_surface) = self.menu_bar_surface {
            if let Some(active) = menu_bar_surface.menu_bar().active_menu() {
                let active = active.to_string();
                if let Some(menu) = menu_bar_surface.menu_bar_mut().get_menu_mut(&active) {
                    
                    menu.on_configure(popup, configure, qh, conn);
                }
            }
        }
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

smithay_client_toolkit::delegate_compositor!(AppData);
smithay_client_toolkit::delegate_output!(AppData);
smithay_client_toolkit::delegate_shm!(AppData);
smithay_client_toolkit::delegate_seat!(AppData);
smithay_client_toolkit::delegate_xdg_shell!(AppData);
smithay_client_toolkit::delegate_xdg_window!(AppData);
smithay_client_toolkit::delegate_registry!(AppData);
smithay_client_toolkit::delegate_xdg_popup!(AppData);

// Manually implement Dispatch for wl_subcompositor
impl wayland_client::Dispatch<wl_subcompositor::WlSubcompositor, GlobalData> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_subcompositor::WlSubcompositor,
        _event: wl_subcompositor::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

// Dispatch for sc-layer protocol
impl wayland_client::Dispatch<sc_layer_shell_v1::ScLayerShellV1, ()> for AppData {
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

impl wayland_client::Dispatch<sc_layer_v1::ScLayerV1, ()> for AppData {
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

// Note: Frame callbacks are handled by CompositorHandler
// We implement the frame callback handling in CompositorHandler trait

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MenuBar Component Example");
    println!("=========================");
    println!("Click on menu labels to toggle menus open/closed");
    println!();
    
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();
    
    let registry_state = RegistryState::new(&globals);
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);
    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let shm_state = Shm::bind(&globals, &qh)?;
    let xdg_shell_state = XdgShell::bind(&globals, &qh)?;
    
    // Bind subcompositor
    let subcompositor: wl_subcompositor::WlSubcompositor = globals.bind(&qh, 1..=1, GlobalData)?;
    
    // Bind sc_layer_shell_v1 for blur/shadow effects
    let sc_layer_shell_v1 = globals.bind::<sc_layer_shell_v1::ScLayerShellV1, _, _>(&qh, 1..=1, ()).ok();
    
    if sc_layer_shell_v1.is_none() {
        eprintln!("Warning: sc_layer_shell_v1 not available, menus will not have blur/shadow effects");
    }
    
    let mut app_data = AppData::new(
        registry_state,
        seat_state,
        output_state,
        compositor_state,
        shm_state,
        xdg_shell_state,
        sc_layer_shell_v1,
    );
    
    app_data.subcompositor = Some(subcompositor);
    
    let surface = app_data.compositor_state.create_surface(&qh);
    let window = app_data.xdg_shell_state.create_window(
        surface,
        WindowDecorations::ServerDefault,
        &qh,
    );
    
    window.set_title("MenuBar Example");
    window.set_app_id("menubar-example");
    window.commit();
    
    app_data.window = Some(window);
    
    while !app_data.exit {
        event_queue.blocking_dispatch(&mut app_data)?;
    }
    
    Ok(())
}
