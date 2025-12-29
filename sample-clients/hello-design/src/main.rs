mod rendering;

// Include generated protocol code
mod sc_layer_protocol {
    use wayland_backend;
    use wayland_client;

    pub use wayland_client::protocol::{__interfaces::*, wl_surface};

    wayland_scanner::generate_interfaces!("../../protocols/sc-layer-v1.xml");
    wayland_scanner::generate_client_code!("../../protocols/sc-layer-v1.xml");
}

use sc_layer_protocol::{sc_layer_shell_v1, sc_layer_v1, sc_transaction_v1};
use rendering::{SkiaContext, SkiaSurface};
use rendering::menu::{MenuItem, MenuStyle, draw_submenu};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_seat, delegate_shm,
    delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_seat, wl_surface, wl_subcompositor, wl_subsurface, wl_pointer},
    Connection, QueueHandle, Dispatch, WEnum,
};
use wayland_cursor::CursorTheme;

// MenuItem is now defined in rendering::menu module

fn get_menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("New Tab", Some("⌘T")),
        MenuItem::new("New Window", Some("⌘N")),
        MenuItem::new("New Incognito Window", Some("⇧⌘N")),
        MenuItem::new("Reopen Closed Tab", Some("⇧⌘T")),
        MenuItem::separator(),
        MenuItem::new("Open File...", Some("⌘O")),
        MenuItem::new("Open Location...", Some("⌘L")),
        MenuItem::separator(),
        MenuItem::new("Close Window", Some("⇧⌘W")),
        MenuItem::new("Close Tab", Some("⌘W")),
        MenuItem::new("Save Page As...", Some("⌘S")),
        MenuItem::separator(),
        MenuItem::new("Print...", Some("⌘P")),
    ]
}

struct LayerData {
    layer: Option<sc_layer_v1::ScLayerV1>,  // Optional - may not be available
    skia_surface: SkiaSurface,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

struct AppData {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    xdg_shell_state: XdgShell,
    sc_layer_shell: Option<sc_layer_shell_v1::ScLayerShellV1>,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    cursor_theme: Option<CursorTheme>,
    cursor_surface: Option<wl_surface::WlSurface>,
    
    window: Option<Window>,
    configured: bool,
    exit: bool,
    
    // Rendering state
    skia_context: Option<SkiaContext>,
    main_surface: Option<SkiaSurface>,
    
    // Layers
    layers: Vec<LayerData>,
    
    // Menu state
    hovered_item: Option<usize>,  // Index of currently hovered menu item
    menu_items: Vec<MenuItem>,
    pointer_on_submenu: bool,  // Track if pointer is on the submenu surface
    pending_hover_update: bool,  // Flag to throttle hover redraws
    
    // Flag to track if we need to create the layer after roundtrip
    needs_layer_creation: bool,
}

impl AppData {
    fn calculate_menu_height(items: &[MenuItem]) -> f32 {
        let style = MenuStyle::default();
        let mut height = style.padding_vertical;
        for item in items {
            if item.is_separator {
                height += style.separator_height;
            } else {
                height += style.item_height;
            }
        }
        height += style.padding_vertical;
        height
    }
    
    fn new(
        registry_state: RegistryState,
        seat_state: SeatState,
        output_state: OutputState,
        compositor_state: CompositorState,
        shm_state: Shm,
        xdg_shell_state: XdgShell,
    ) -> Self {
        Self {
            registry_state,
            seat_state,
            output_state,
            compositor_state,
            shm_state,
            xdg_shell_state,
            sc_layer_shell: None,
            subcompositor: None,
            keyboard: None,
            pointer: None,
            cursor_theme: None,
            cursor_surface: None,
            window: None,
            configured: false,
            exit: false,
            skia_context: None,
            main_surface: None,
            layers: Vec::new(),
            hovered_item: None,
            menu_items: Vec::new(),
            pointer_on_submenu: false,
            pending_hover_update: false,
            needs_layer_creation: false,
        }
    }

    fn init_rendering(&mut self, wl_surface: &wl_surface::WlSurface, wl_display_ptr: *mut std::ffi::c_void, width: i32, height: i32) -> Result<(), String> {
        let (ctx, surface) = SkiaContext::new(wl_display_ptr, wl_surface, width, height)?;
        self.skia_context = Some(ctx);
        self.main_surface = Some(surface);
        Ok(())
    }

    fn create_layer(&mut self, qh: &QueueHandle<Self>, width: i32, height: i32, x: f32, y: f32) -> Result<(), String> {
        let parent_surface = self.window.as_ref()
            .ok_or("No parent window")?
            .wl_surface();
        
        // Create base menu surface (static content)
        let menu_surface = self.compositor_state.create_surface(qh);
        menu_surface.set_buffer_scale(2);
        
        if let Some(subcompositor) = self.subcompositor.as_ref() {
            let subsurface = subcompositor.get_subsurface(&menu_surface, parent_surface, qh, ());
            subsurface.set_position(x as i32, y as i32);
            subsurface.set_desync();
        } else {
            eprintln!("Warning: No subcompositor, menu may not be positioned correctly");
        }
        
        let mut base_surface = {
            let ctx = self.skia_context.as_ref()
                .ok_or("Rendering not initialized")?;
            ctx.create_surface(menu_surface.clone(), width, height)?
        };

        // Initialize with transparent - will be drawn later
        if let Some(ctx) = self.skia_context.as_mut() {
            base_surface.draw(ctx, |canvas| {
                canvas.clear(skia_safe::Color::from_argb(0, 0, 0, 0));
            });
            base_surface.commit();
        }
        
        // Store base layer
        self.layers.push(LayerData {
            layer: None,
            skia_surface: base_surface,
            x,
            y,
            width: width as f32,
            height: height as f32,
        });
        
        // Create hover highlight surface (dynamic content)
        let hover_surface = self.compositor_state.create_surface(qh);
        hover_surface.set_buffer_scale(2);
        
        if let Some(subcompositor) = self.subcompositor.as_ref() {
            let hover_subsurface = subcompositor.get_subsurface(&hover_surface, parent_surface, qh, ());
            hover_subsurface.set_position(x as i32, y as i32);
            hover_subsurface.set_desync();
        }
        
        let mut hover_skia_surface = {
            let ctx = self.skia_context.as_ref()
                .ok_or("Rendering not initialized")?;
            ctx.create_surface(hover_surface.clone(), width, height)?
        };
        
        // Initialize transparent
        if let Some(ctx) = self.skia_context.as_mut() {
            hover_skia_surface.draw(ctx, |canvas| {
                canvas.clear(skia_safe::Color::from_argb(200, 255, 0, 0));
            });
            hover_skia_surface.commit();
        }
        
        // Store hover layer
        self.layers.push(LayerData {
            layer: None,
            skia_surface: hover_skia_surface,
            x,
            y,
            width: width as f32,
            height: height as f32,
        });
        
        Ok(())
    }
    
    fn create_layer_augmentation(&mut self, qh: &QueueHandle<Self>) {
        // Augment base menu layer (index 0)
        if let Some(layer_data) = self.layers.get(0) {
            let menu_surface = layer_data.skia_surface.wl_surface();
            let width = layer_data.width;
            let height = layer_data.height;
            
            if let Some(sc_layer_shell) = self.sc_layer_shell.as_ref() {
                let layer = sc_layer_shell.get_layer(menu_surface, qh, ());
                layer.set_position(0.0, 0.0);
                layer.set_size(width as f64, height as f64);
                layer.set_opacity(1.0);
                layer.set_background_color(1.0, 1.0, 1.0, 0.95);
                layer.set_corner_radius(8.0);
                layer.set_masks_to_bounds(1);
                layer.set_blend_mode(sc_layer_v1::BlendMode::BackgroundBlur);
                layer.set_shadow(0.3, 8.0, 0.0, 2.0, 0.0, 0.0, 0.0);
                
                if let Some(layer_data) = self.layers.get_mut(0) {
                    layer_data.layer = Some(layer);
                }
            }
        }
        
        // Augment hover layer (index 1) - no background, just overlay
        if let Some(layer_data) = self.layers.get(1) {
            let hover_surface = layer_data.skia_surface.wl_surface();
            let width = layer_data.width;
            let height = layer_data.height;
            
            if let Some(sc_layer_shell) = self.sc_layer_shell.as_ref() {
                let layer = sc_layer_shell.get_layer(hover_surface, qh, ());
                layer.set_position(0.0, 0.0);
                layer.set_size(width as f64, height as f64);
                layer.set_opacity(1.0);
                // No background color for hover layer, just transparent overlay
                
                if let Some(layer_data) = self.layers.get_mut(1) {
                    layer_data.layer = Some(layer);
                }
            }
        }
    }

    fn draw_submenu(&mut self, index: usize, menu_items: &[MenuItem]) {
        if index >= self.layers.len() {
            return;
        }
        
        let ctx = match self.skia_context.as_mut() {
            Some(ctx) => ctx,
            None => return,
        };
        
        let layer = &mut self.layers[index];
        let items = menu_items.to_vec();
        let surface_width = layer.skia_surface.width();
        let logical_width = surface_width as f32 / 2.0;

        layer.skia_surface.draw(ctx, |canvas| {
            canvas.scale((2.0, 2.0));
            let style = MenuStyle::default();
            // Draw static content without hover (None for hovered_item)
            draw_submenu(canvas, &items, logical_width, None, &style);
        });
        
        layer.skia_surface.commit();
    }
    
    fn draw_hover_highlight(&mut self, hover_layer_index: usize) {
        if hover_layer_index >= self.layers.len() {
            return;
        }
        
        let ctx = match self.skia_context.as_mut() {
            Some(ctx) => ctx,
            None => return,
        };
        
        let layer = &mut self.layers[hover_layer_index];
        let items = self.menu_items.clone();
        let surface_width = layer.skia_surface.width();
        let logical_width = surface_width as f32 / 2.0;
        let hovered_item = self.hovered_item;

        layer.skia_surface.draw(ctx, |canvas| {
            // Clear to transparent first
            canvas.clear(skia_safe::Color::from_argb(0, 0, 0, 0));
            canvas.scale((2.0, 2.0));
            
            // Draw only the hover highlight if there is one
            if let Some(item_idx) = hovered_item {
                let style = MenuStyle::default();
                
                // Create fonts matching the base layer
                let font_mgr = skia_safe::FontMgr::new();
                let font_style = skia_safe::FontStyle::new(
                    skia_safe::font_style::Weight::MEDIUM,
                    skia_safe::font_style::Width::NORMAL,
                    skia_safe::font_style::Slant::Upright
                );
                
                let typeface = font_mgr
                    .match_family_style("Inter", font_style)
                    .or_else(|| font_mgr.match_family_style("Inter UI", font_style))
                    .or_else(|| font_mgr.match_family_style("system-ui", font_style))
                    .unwrap_or_else(|| font_mgr.legacy_make_typeface(None, font_style).unwrap());
                
                let mut menu_font = skia_safe::Font::from_typeface(typeface.clone(), style.font_size);
                menu_font.set_subpixel(true);
                menu_font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
                
                let mut shortcut_font = skia_safe::Font::from_typeface(typeface, style.shortcut_font_size);
                shortcut_font.set_subpixel(true);
                shortcut_font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
                
                let mut y_pos = style.padding_vertical;
                let mut current_idx = 0;
                
                for item in &items {
                    if item.is_separator {
                        y_pos += style.separator_height;
                    } else {
                        if current_idx == item_idx {
                            // Use the menu module's draw_menu_item function for consistency
                            use rendering::menu::draw_menu_item;
                            draw_menu_item(canvas, item, y_pos, logical_width, true, &style, &menu_font, &shortcut_font);
                            break;
                        }
                        y_pos += style.item_height;
                        current_idx += 1;
                    }
                }
            }
        });
        
        layer.skia_surface.commit();
    }

    fn draw(&mut self, time: u32) {
        println!("[DRAW] Starting draw at time {}", time);
        let ctx = match self.skia_context.as_mut() {
            Some(ctx) => ctx,
            None => {
                println!("[DRAW] No skia context in draw");
                return;
            }
        };
        
        let surface = match self.main_surface.as_mut() {
            Some(s) => s,
            None => {
                println!("[DRAW] No main surface in draw");
                return;
            }
        };
        
        let width = surface.width();
        let height = surface.height();
        
        // println!("Drawing frame at time {} ({}x{})", time, width, height);
        
        surface.draw(ctx, |canvas| {
            // Clear background to #F6F6F6
            canvas.clear(skia_safe::Color::from_rgb(246, 246, 246));

            // Draw a rotating rectangle using compositor time
            let time_seconds = time as f32 / 1000.0;
            let angle = (time_seconds * 30.0) % 360.0; // 30 degrees per second

            let center_x = width as f32 / 2.0;
            let center_y = height as f32 / 2.0;

            canvas.save();
            canvas.translate((center_x, center_y));
            canvas.rotate(angle, None);
            
            let mut paint = skia_safe::Paint::default();
            paint.set_color(skia_safe::Color::from_rgb(100, 200, 255));
            paint.set_anti_alias(true);
            
            let rect = skia_safe::Rect::from_xywh(-100.0, -100.0, 200.0, 200.0);
            canvas.draw_rect(rect, &paint);
            
            canvas.restore();
        });
        
        println!("[DRAW] Finished drawing frame");
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
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        time: u32,
    ) {
        println!("[FRAME] Handler called at time {}", time);
        // Create layer augmentation after first roundtrip (compositor has processed commits)
        if self.needs_layer_creation {
            self.needs_layer_creation = false;
            self.create_layer_augmentation(qh);
        }
        
        // Process pending hover update if any
        if self.pending_hover_update {
            self.pending_hover_update = false;
            self.draw_hover_highlight(1);
        }
        
        self.draw(time);
        if let Some(main_surface) = &self.main_surface {
            main_surface.commit();
        }
        // Always request next frame
        println!("[FRAME] Requesting next frame callback");
        surface.frame(qh, surface.clone());
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
        _serial: u32,
    ) {
        // Update dimensions from configure event
        let (width_opt, height_opt) = configure.new_size;
        if let (Some(width), Some(height)) = (width_opt, height_opt) {
            let width = width.get() as i32;
            let height = height.get() as i32;
            
            println!("Configure event: resize to {}x{}", width, height);
            
            // Resize main surface if it exists
            if let Some(ref mut main_surface) = self.main_surface {
                main_surface.resize(width, height);
            }
        } else {
            println!("Configure event: no size specified");
        }
        
        if !self.configured {
            println!("[CONFIGURE] First configure, setting up rendering");
            self.configured = true;
            
            // Get dimensions
            let (width, height) = match configure.new_size {
                (Some(w), Some(h)) => (w.get() as i32, h.get() as i32),
                _ => (800, 600),
            };
            
            // Initialize rendering now that we have a surface
            let wl_surface = window.wl_surface();
            let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
            
            if let Err(e) = self.init_rendering(wl_surface, display_ptr, width, height) {
                eprintln!("Failed to initialize rendering: {}", e);
                return;
            }
            
            // Create menu items
            let menu_items = get_menu_items();
            self.menu_items = menu_items.clone();
            
            // Calculate submenu height
            let submenu_height = Self::calculate_menu_height(&menu_items);
            
            // Create base + hover layers at 2x resolution for crisp text
            if let Err(e) = self.create_layer(qh, 280 * 2, (submenu_height as i32) * 2, 50.0, 50.0) {
                eprintln!("Failed to create menu layers: {}", e);
            }
            
            // Draw first frame
            self.draw(0);
            
            // Draw static base menu (layer 0) once
            self.draw_submenu(0, &menu_items);
            // Initialize empty hover layer (layer 1)
            self.draw_hover_highlight(1);
            self.needs_layer_creation = true;
            
            // Request frame callback BEFORE commit
            let wl_surface = window.wl_surface();
            println!("[CONFIGURE] Requesting frame callback");
            wl_surface.frame(qh, wl_surface.clone());
            
            // Commit all surfaces
            if let Some(main_surface) = &self.main_surface {
                println!("[CONFIGURE] Committing main surface");
                main_surface.commit();
            }
        } else {
            // For subsequent configures, also request frame callback
            let wl_surface = window.wl_surface();
            println!("[CONFIGURE] Subsequent configure, requesting frame callback");
            wl_surface.frame(qh, wl_surface.clone());
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
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = Some(seat.get_pointer(qh, ()));
            
            // Initialize cursor theme
            let shm = self.shm_state.wl_shm();
            self.cursor_theme = CursorTheme::load_from_name(_conn, shm.clone(), "default", 24).ok();
            self.cursor_surface = Some(self.compositor_state.create_surface(qh));
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        _capability: Capability,
    ) {
    }

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
        event: <wl_keyboard::WlKeyboard as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event {
            // ESC key code is 1 (scancode)
            if key == 1 && key_state == WEnum::Value(wl_keyboard::KeyState::Pressed) {
                state.exit = true;
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &wl_pointer::WlPointer,
        event: <wl_pointer::WlPointer as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface, serial, .. } => {
                // Check if entering the menu layer surface
                if let Some(layer_data) = state.layers.get(1) {
                    if layer_data.skia_surface.wl_surface() == &surface {
                        state.pointer_on_submenu = true;
                        
                        // Set hand cursor
                        if let (Some(cursor_theme), Some(cursor_surface)) = (&mut state.cursor_theme, &state.cursor_surface) {
                            if let Some(cursor) = cursor_theme.get_cursor("pointer") {
                                let image = &cursor[0];
                                let (w, h) = image.dimensions();
                                let (hx, hy) = image.hotspot();
                                
                                cursor_surface.attach(Some(&image), 0, 0);
                                cursor_surface.damage_buffer(0, 0, w as i32, h as i32);
                                cursor_surface.commit();
                                
                                proxy.set_cursor(serial, Some(cursor_surface), hx as i32, hy as i32);
                            }
                        }
                    }
                }
            }
            wl_pointer::Event::Leave { surface, serial, .. } => {
                // Check if leaving the menu layer surface
                if let Some(layer_data) = state.layers.get(1) {
                    if layer_data.skia_surface.wl_surface() == &surface {
                        state.pointer_on_submenu = false;
                        
                        // Reset to default cursor
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
                        
                        if state.hovered_item.is_some() {
                            state.hovered_item = None;
                            // Mark pending update instead of drawing immediately
                            state.pending_hover_update = true;
                        }
                    }
                }
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                // Only process if pointer is on the submenu surface
                if state.pointer_on_submenu {
                    // Coordinates are now relative to the subsurface (0,0)
                    let rel_y = surface_y;
                    let style = MenuStyle::default();
                    
                    // Calculate which item is hovered
                    let mut y_pos = style.padding_vertical as f64;
                    let mut item_index = 0;
                    let mut found_item = None;
                    
                    for item in &state.menu_items {
                        if item.is_separator {
                            y_pos += style.separator_height as f64;
                        } else {
                            if rel_y >= y_pos && rel_y < y_pos + style.item_height as f64 {
                                found_item = Some(item_index);
                                break;
                            }
                            y_pos += style.item_height as f64;
                            item_index += 1;
                        }
                    }
                    
                    // Update hovered item if changed
                    if state.hovered_item != found_item {
                        state.hovered_item = found_item;
                        // Mark that we need to update hover, will be processed on next frame
                        state.pending_hover_update = true;
                    }
                }
            }
            _ => {}
        }
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(AppData);
delegate_output!(AppData);
delegate_shm!(AppData);
delegate_seat!(AppData);
delegate_xdg_shell!(AppData);
delegate_xdg_window!(AppData);
delegate_registry!(AppData);

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

impl Dispatch<sc_transaction_v1::ScTransactionV1, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &sc_transaction_v1::ScTransactionV1,
        _event: sc_transaction_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_subcompositor::WlSubcompositor, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_subcompositor::WlSubcompositor,
        _event: <wl_subcompositor::WlSubcompositor as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_subsurface::WlSubsurface, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_subsurface::WlSubsurface,
        _event: <wl_subsurface::WlSubsurface as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
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
    
    // Bind sc_layer_shell and subcompositor (optional - fallback if not available)
    let sc_layer_shell = globals.bind::<sc_layer_shell_v1::ScLayerShellV1, _, _>(&qh, 1..=1, ()).ok();
    let subcompositor = globals.bind::<wl_subcompositor::WlSubcompositor, _, _>(&qh, 1..=1, ()).ok();

    if sc_layer_shell.is_none() {
        eprintln!("Warning: sc_layer_shell_v1 not available, menu will not have shadow");
    }
    if subcompositor.is_none() {
        eprintln!("Warning: wl_subcompositor not available, menu positioning may not work");
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
    app_data.subcompositor = subcompositor;

    // Create window
    let surface = app_data.compositor_state.create_surface(&qh);
    let window = app_data.xdg_shell_state.create_window(
        surface,
        WindowDecorations::ServerDefault,
        &qh,
    );
    window.set_title("Hello Skia on Wayland");
    window.set_app_id("hello-design");
    window.set_min_size(Some((400, 300)));
    window.commit();

    app_data.window = Some(window);
    
    // Do an initial roundtrip to process the window creation
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
