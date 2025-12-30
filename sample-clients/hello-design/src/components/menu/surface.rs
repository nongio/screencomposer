use smithay_client_toolkit::{
    compositor::{CompositorState, SurfaceData},
    reexports::client::{
        Connection, Dispatch, QueueHandle, protocol::{wl_callback, wl_pointer, wl_surface}
    },
    shell::{
        WaylandSurface, xdg::{
            XdgPositioner, XdgShell, XdgSurface, popup::{self, Popup, PopupConfigure, PopupData}, window::Window
        }
    },
};
use wayland_client::{Proxy, protocol::wl_keyboard};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_popup};
use std::collections::HashMap;

use crate::rendering::{SkiaContext, SkiaSurface};

use super::{
    data::{Anchor, Gravity, MenuItem, MenuItemId, MenuStyle, Position},
    drawing::draw_menu,
    MenuError,
};

/// Main menu component
pub struct Menu {
    // Menu data
    items: Vec<MenuItem>,
    style: MenuStyle,

    // Root menu surface (created on show)
    root: Option<MenuSurface>,

    // Interaction state
    hovered_item: Option<usize>,
    pointer_x: f64,
    pointer_y: f64,
    prev_pointer_x: f64,
    prev_pointer_y: f64,

    // Submenu tracking
    submenus: HashMap<usize, MenuSurface>,
    hovering_submenu: bool, // Track if pointer is over a submenu surface

    // Click handler
    on_click: Option<Box<dyn Fn(&MenuItemId)>>,
}

impl Menu {
    /// Create a new menu
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            items,
            style: MenuStyle::default(),
            root: None,
            hovered_item: None,
            pointer_x: 0.0,
            pointer_y: 0.0,
            prev_pointer_x: 0.0,
            prev_pointer_y: 0.0,
            submenus: HashMap::new(),
            hovering_submenu: false,
            on_click: None,
        }
    }

    /// Set the click handler
    pub fn set_on_click<F>(&mut self, handler: F)
    where
        F: Fn(&MenuItemId) + 'static,
    {
        self.on_click = Some(Box::new(handler));
    }

    /// Set custom styling
    pub fn set_style(&mut self, style: MenuStyle) {
        self.style = style;
    }

    /// Show the menu at the specified position
    pub fn open_menu<D>(
        &mut self,
        parent_window: &Window,
        position: Position,
        qh: &QueueHandle<D>,
        compositor: &CompositorState,
        xdg_shell: &XdgShell,
        _conn: &Connection,
        display_ptr: *mut std::ffi::c_void,
    ) -> Result<(), MenuError>
    where
        D: wayland_client::Dispatch<wl_surface::WlSurface, SurfaceData> + 
           wayland_client::Dispatch<xdg_surface::XdgSurface, PopupData> +
           wayland_client::Dispatch<xdg_popup::XdgPopup, PopupData> +
           'static,
    {
        // Create root menu surface
        let width = self.style.calculate_menu_width(&self.items);
        let height = self.style.calculate_menu_height(&self.items);

        let wl_surface = compositor.create_surface(qh);

        // Create positioner
        let positioner = create_positioner(
            xdg_shell,
            qh,
            &self.items,
            &position,
            &self.style,
        )?;

        let parent_xdg = parent_window.xdg_surface();
        // Create popup with parent XdgSurface
        let popup = Popup::from_surface(
            Some(parent_xdg),
            &positioner,
            qh,
            wl_surface.clone(),
            xdg_shell,
        )
        .map_err(|_| MenuError::SurfaceCreationFailed)?;;

        // Get popup surface for XDG operations
        popup.xdg_surface().set_window_geometry(0, 0, width as i32, height as i32);

        // Use 2x buffer for HiDPI rendering
        let buffer_scale = 2;
        wl_surface.set_buffer_scale(buffer_scale);

        // Create Skia context and surface for rendering
        let (skia_context, skia_surface) = SkiaContext::new(
            display_ptr,
            &wl_surface,
            (width as i32) * buffer_scale,
            (height as i32) * buffer_scale,
        ).map_err(|_| MenuError::SurfaceCreationFailed)?;

        let menu_surface = MenuSurface {
            wl_surface: wl_surface.clone(),
            popup: Some(popup),
            skia_context,
            skia_surface,
            items: self.items.clone(),
            width: width as i32,
            height: height as i32,
            hovered_item: None,
            needs_redraw: false,
            configured: false,
            frame_callback: None,
            open_submenus: HashMap::new(),
        };

        // Commit the surface to trigger configure event from compositor
        println!("Committing menu surface to trigger configure event");
        wl_surface.commit();
        
        // Don't render yet - wait for configure event
        // Rendering will happen in the configure handler when configured = true
        
        self.root = Some(menu_surface);
        
        println!("Menu show() completed successfully");

        Ok(())
    }

    /// Hide the menu
    pub fn hide(&mut self) {
        // Close all submenus first
        self.close_all_submenus();
        
        if let Some(mut root) = self.root.take() {
            root.destroy();
        }
        
        // Reset all interaction state
        self.hovered_item = None;
        self.pointer_x = 0.0;
        self.pointer_y = 0.0;
        self.prev_pointer_x = 0.0;
        self.prev_pointer_y = 0.0;
        self.hovering_submenu = false;
    }

    /// Check if menu is visible
    pub fn is_visible(&self) -> bool {
        self.root.is_some()
    }

    /// Handle pointer enter event
    pub fn on_pointer_enter(&mut self, surface: &wl_surface::WlSurface, x: f64, y: f64) {
        if let Some(root) = &mut self.root {
            if &root.wl_surface == surface {
                self.pointer_x = x;
                self.pointer_y = y;
                self.update_hover();
            }
        }
    }

    /// Handle pointer motion event
    pub fn on_pointer_motion(&mut self, surface: &wl_surface::WlSurface, x: f64, y: f64) {
        self.prev_pointer_x = self.pointer_x;
        self.prev_pointer_y = self.pointer_y;
        self.pointer_x = x;
        self.pointer_y = y;
        
        // Check if this is the root surface
        if self.root.as_ref().map_or(false, |r| &r.wl_surface == surface) {
            self.hovering_submenu = false;
            self.update_hover();
            
            // Close level 2 submenus that don't match the hovered item
            if let Some(root) = &mut self.root {
                let hovered_idx = root.hovered_item;
                for (idx, submenu) in root.open_submenus.iter_mut() {
                    if Some(*idx) != hovered_idx {
                        // Not hovered, close it (but keep in HashMap)
                        submenu.close_all_submenus_recursive();
                        if let Some(popup) = submenu.popup.take() {
                            popup.xdg_popup().destroy();
                            popup.wl_surface().destroy();
                            submenu.configured = false;
                        }
                        submenu.hovered_item = None;
                    }
                }
            }
        } else {
            // Use recursive helper to find and handle the surface
            self.hovering_submenu = false;
            if let Some(root) = &mut self.root {
                let mut active_path = Vec::new();
                if root.handle_pointer_motion_recursive(surface, y, &self.style, &mut active_path) {
                    self.hovering_submenu = true;
                    // Only close inactive submenus if we're not hovering a submenu item that's about to open
                    // The submenu opening logic will handle this
                    // Don't call close_inactive_submenus here - it interferes with submenu creation
                }
            }
        }
    }

    /// Handle pointer leave event
    pub fn on_pointer_leave(&mut self, surface: &wl_surface::WlSurface) {
        // Only clear hover if leaving ALL menu surfaces (not just moving between them)
        // Check if the surface is the root or any submenu
        let is_menu_surface = self.root.as_ref().map_or(false, |r| &r.wl_surface == surface)
            || self.submenus.values().any(|s| &s.wl_surface == surface);
        
        if is_menu_surface && self.hovered_item.is_some() {
            // Don't clear immediately - the triangle logic will handle it
            // Only clear hover and redraw if needed
            self.hovered_item = None;
            self.set_need_render();
        }
    }

    /// Handle pointer button event
    pub fn on_pointer_button(&mut self, button: u32, state: wl_pointer::ButtonState) {
        if button == 272 && state == wl_pointer::ButtonState::Pressed {
            // Left click - check root and all submenus recursively
            if let Some(ref handler) = self.on_click {
                if let Some(root) = &self.root {
                    if root.handle_pointer_button_recursive(handler) {
                        // An item was clicked, close the menu
                        self.hide();
                    }
                }
            }
        }
    }

    /// Handle keyboard key event
    pub fn on_keyboard_key(&mut self, key: u32, state: wl_keyboard::KeyState) {
        if state == wl_keyboard::KeyState::Pressed {
            match key {
                1 => {
                    // Escape - close menu
                    self.hide();
                }
                103 => {
                    // Up arrow
                    self.navigate_up();
                }
                108 => {
                    // Down arrow
                    self.navigate_down();
                }
                28 => {
                    // Enter - activate item
                    if let Some(hover_idx) = self.hovered_item {
                        if let Some(item) = self.items.get(hover_idx) {
                            if !item.is_separator() && !item.is_submenu() && item.is_enabled() {
                                if let Some(ref handler) = self.on_click {
                                    if let Some(id) = item.id() {
                                        handler(&MenuItemId::from(id));
                                    }
                                }
                                self.hide();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Handle configure event for a specific popup
    pub fn on_configure(&mut self, popup: &Popup, configure: PopupConfigure) {
        let popup_surface = popup.wl_surface();
        println!("Menu configure for popup surface: {:?}", popup_surface.id());
        
        // Check if it's the root menu
        if let Some(root) = &mut self.root {
            if root.handle_configure_recursive(&popup_surface.id(), configure.serial, &self.style, 0) {
                return;
            }
        }
        
        println!("WARNING: Configure event for unknown popup surface!");
    }

    /// Handle frame callback
    // pub fn on_frame(&mut self, qh: &QueueHandle<impl wayland_client::Dispatch<wayland_client::protocol::wl_callback::WlCallback, ()>>) {
        
    //     let needs_redraw = if let Some(root) = &mut self.root {
    //         root.frame_callback = None;
    //         root.needs_redraw
    //     } else {
    //         false
    //     };

    //     if needs_redraw {
    //         if let Some(root) = &mut self.root {
    //             root.render(&self.style);
    //             root.needs_redraw = false;
    //         }
    //     }
    // }

    /// Request a frame callback if not already pending
    // fn request_frame(&mut self, qh: &QueueHandle<impl wayland_client::Dispatch<wayland_client::protocol::wl_callback::WlCallback, ()> + 'static>) {
    //     if let Some(root) = &mut self.root {
    //         if root.frame_callback.is_none() && root.configured {
    //             let data = ();
    //             let callback = root.wl_surface.frame(qh, data);
    //             root.frame_callback = Some(callback);
    //         }
    //     }
    // }

    /// Mark surface for redraw
    fn set_need_render(&mut self) {
        if let Some(root) = &mut self.root {
            root.needs_redraw = true;
            root.render(&self.style);
            root.needs_redraw = false;
        }
    }

    /// Update hover state based on pointer position
    fn update_hover(&mut self) {
        let new_hover = self.item_at_position(self.pointer_y as f32);
        if let Some(root) = &mut self.root {
            if new_hover != root.hovered_item {
                root.hovered_item = new_hover;
                self.hovered_item = new_hover; // Sync the top-level field too
                self.set_need_render();
            }
        }
    }

    /// Check if we should open a submenu and return its path and index
    /// Returns (parent_path, item_idx) where parent_path is the indices to navigate to the parent menu
    pub fn should_open_submenu(&self) -> Option<(Vec<usize>, usize)> {
        // Check root menu first
        if let Some(root) = &self.root {
            if let Some(result) = root.check_should_open_submenu_recursive(&[]) {
                return Some(result);
            }
        }
        
        None
    }

    /// Check if we should close submenus
    pub fn should_close_submenus(&self) -> bool {
        // Count configured submenus
        let configured_count = self.submenus.values().filter(|s| s.configured).count();
        
        // Never close if we have no configured submenus
        if configured_count == 0 {
            return false;
        }
        
        // Never close if we're hovering over a submenu surface
        if self.hovering_submenu {
            return false;
        }
        
        // Check triangle/safe zone - if mouse is moving toward any open submenu, don't close
        if self.is_moving_toward_submenus() {
            return false;
        }
        
        match self.hovered_item {
            Some(idx) => {
                // Don't close if hovering the parent item of a configured submenu
                if self.submenus.get(&idx).map_or(false, |s| s.configured) {
                    return false;
                }
                
                // Don't close if hovering another submenu item (we'll switch submenus instead)
                if let Some(item) = self.items.get(idx) {
                    if item.is_submenu() {
                        return false;
                    }
                }
                
                // Close if hovering a regular action item
                true
            }
            None => {
                // If not hovering anything in the root menu, only close if we're
                // also not in the submenu and not moving toward it
                // (triangle logic already checked above)
                true
            }
        }
    }
    
    /// Check if mouse is moving toward any open submenu (triangle/safe zone logic)
    fn is_moving_toward_submenus(&self) -> bool {
        // Get root menu width to calculate submenu absolute positions
        let root_width = self.style.calculate_menu_width(&self.items);
        
        // Check each configured submenu
        for (item_idx, submenu) in self.submenus.iter() {
            // Skip unconfigured (closed) submenus
            if !submenu.configured {
                continue;
            }
            
            // Calculate submenu position (positioned to the right of root menu)
            let submenu_left = root_width;
            let submenu_right = root_width + self.style.calculate_menu_width(&submenu.items);
            
            // Calculate Y position of this submenu (aligned with its parent item)
            let mut submenu_top = self.style.padding_vertical;
            for (idx, item) in self.items.iter().enumerate() {
                if idx == *item_idx {
                    break;
                }
                if item.is_separator() {
                    submenu_top += self.style.separator_height;
                } else {
                    submenu_top += self.style.item_height;
                }
            }
            let submenu_bottom = submenu_top + self.style.calculate_menu_height(&submenu.items);
            
            // Check if moving toward this submenu using triangle algorithm
            if self.is_moving_toward_rect(
                submenu_left as f64,
                submenu_top as f64,
                submenu_right as f64,
                submenu_bottom as f64,
            ) {
                return true;
            }
        }
        
        false
    }
    
    /// Triangle/safe zone algorithm: check if mouse trajectory is toward a rectangle
    /// This creates a triangle from the current mouse position to the two corners
    /// of the rectangle edge closest to the mouse, and checks if the movement
    /// vector points within that triangle
    fn is_moving_toward_rect(&self, rect_left: f64, rect_top: f64, rect_right: f64, rect_bottom: f64) -> bool {
        // Movement vector
        let dx = self.pointer_x - self.prev_pointer_x;
        let dy = self.pointer_y - self.prev_pointer_y;
        
        // If mouse hasn't moved significantly, keep submenu open
        if dx.abs() < 0.1 && dy.abs() < 0.1 {
            return true;
        }
        
        // Determine which edge of the rectangle is closest to the mouse
        // and use its two corners to form the triangle
        let (corner1_x, corner1_y, corner2_x, corner2_y) = if self.pointer_x < rect_left {
            // Mouse is to the left - use left edge corners
            (rect_left, rect_top, rect_left, rect_bottom)
        } else if self.pointer_x > rect_right {
            // Mouse is to the right - use right edge corners
            (rect_right, rect_top, rect_right, rect_bottom)
        } else {
            // Mouse is horizontally within bounds - use top or bottom edge
            if self.pointer_y < rect_top {
                (rect_left, rect_top, rect_right, rect_top)
            } else {
                (rect_left, rect_bottom, rect_right, rect_bottom)
            }
        };
        
        // Vectors from current position to the two corners
        let to_corner1_x = corner1_x - self.pointer_x;
        let to_corner1_y = corner1_y - self.pointer_y;
        let to_corner2_x = corner2_x - self.pointer_x;
        let to_corner2_y = corner2_y - self.pointer_y;
        
        // Use cross product to check if movement vector is between the two corner vectors
        // If the cross products have opposite signs, the movement is within the triangle
        let cross1 = dx * to_corner1_y - dy * to_corner1_x;
        let cross2 = dx * to_corner2_y - dy * to_corner2_x;
        
        // Movement is toward rectangle if it's between the two corner vectors
        cross1 * cross2 <= 0.0
    }

    /// Find which item is at the given Y position
    fn item_at_position(&self, y: f32) -> Option<usize> {
        let mut current_y = self.style.padding_vertical;
        let mut item_index = 0;

        for item in &self.items {
            if item.is_separator() {
                current_y += self.style.separator_height;
                item_index += 1; // Still increment to track position in items array
            } else {
                let item_bottom = current_y + self.style.item_height;
                if y >= current_y && y < item_bottom {
                    return Some(item_index); // Returns actual index in items array
                }
                current_y = item_bottom;
                item_index += 1;
            }
        }

        None
    }

    /// Navigate to previous item
    fn navigate_up(&mut self) {
        let current = self.hovered_item.unwrap_or(0);
        if current > 0 {
            self.hovered_item = Some(current - 1);
            self.set_need_render();
        }
    }

    /// Navigate to next item
    fn navigate_down(&mut self) {
        let max_index = self.items.iter().filter(|i| !i.is_separator()).count();
        let current = self.hovered_item.unwrap_or(0);
        if current + 1 < max_index {
            self.hovered_item = Some(current + 1);
            self.set_need_render();
        }
    }

    /// Get the root surface (if visible)
    pub fn root_surface(&self) -> Option<&wl_surface::WlSurface> {
        self.root.as_ref().map(|r| &r.wl_surface)
    }

    /// Check if a surface belongs to this menu
    pub fn owns_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        // Check root surface
        if self.root.as_ref().map_or(false, |r| &r.wl_surface == surface) {
            return true;
        }
        
        // Use recursive helper to check all submenu levels
        if let Some(root) = &self.root {
            for submenu in root.open_submenus.values() {
                if submenu.owns_surface_recursive(surface) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Close all open submenus
    pub fn close_all_submenus(&mut self) {
        println!("Closing all submenus (count: {})", self.submenus.len());
        
        // Use recursive helper to close all nested submenus
        if let Some(root) = &mut self.root {
            root.close_all_submenus_recursive();
        }
        
        // Reset submenu-related state
        self.hovering_submenu = false;
    }

    /// Open a submenu at the given item index
    /// parent_path: indices to navigate to the parent menu (empty for root)
    pub fn open_submenu<D>(
        &mut self,
        parent_path: Vec<usize>,
        item_index: usize,
        compositor: &CompositorState,
        xdg_shell: &XdgShell,
        qh: &QueueHandle<D>,
        display_ptr: *mut std::ffi::c_void,
    ) -> Result<(), MenuError>
    where
        D: wayland_client::Dispatch<wl_surface::WlSurface, SurfaceData> + 
           wayland_client::Dispatch<xdg_surface::XdgSurface, PopupData> +
           wayland_client::Dispatch<xdg_popup::XdgPopup, PopupData> +
           'static,
    {
        // Navigate to the parent menu using the path
        let mut current = self.root.as_ref().ok_or(MenuError::SurfaceCreationFailed)?;
        
        for &idx in &parent_path {
            current = current.open_submenus.get(&idx).ok_or(MenuError::SurfaceCreationFailed)?;
        }
        
        // Get the parent menu items and width
        let parent_items = current.items.clone();
        let parent_width = current.width;
        let parent_popup = current.popup.as_ref().ok_or(MenuError::SurfaceCreationFailed)?;
        
        // Get the submenu items
        let (submenu_items, item_y_position) = if let Some(MenuItem::Submenu { items, .. }) = parent_items.get(item_index) {
            // Calculate Y position of this item
            let mut y = self.style.padding_vertical;
            for (idx, item) in parent_items.iter().enumerate() {
                if idx == item_index {
                    break;
                }
                if item.is_separator() {
                    y += self.style.separator_height;
                } else {
                    y += self.style.item_height;
                }
            }
            (items.clone(), y as i32)
        } else {
            return Ok(()); // Not a submenu
        };

        // First, check if submenu already exists (peek without mut borrow)
        // Navigate to parent again for checking
        let mut check_current = self.root.as_ref().ok_or(MenuError::SurfaceCreationFailed)?;
        for &idx in &parent_path {
            check_current = check_current.open_submenus.get(&idx).ok_or(MenuError::SurfaceCreationFailed)?;
        }
        
        let already_exists = check_current.open_submenus.get(&item_index)
            .map(|s| (s.configured, s.popup.is_some()));

        // If exists with popup and is configured, nothing to do
        // If exists with popup but not configured, we need to wait for configure
        // If exists without popup, we need to create a new one (fall through)
        if let Some((is_configured, has_popup)) = already_exists {
            println!("[open_submenu] Path {:?}, item {} already exists: configured={}, has_popup={}", parent_path, item_index, is_configured, has_popup);
            if has_popup {
                if is_configured {
                    // Already open and configured, nothing to do
                    println!("[open_submenu] Already configured, returning");
                    return Ok(());
                } else {
                    // Popup exists but waiting for configure - nothing to do
                    println!("[open_submenu] Waiting for configure, returning");
                    return Ok(());
                }
            }
            // If we get here, submenu exists but popup was destroyed
            // Fall through to create a new popup
            println!("[open_submenu] Submenu exists but no popup, creating new one");
        }
        
        // Create a new submenu surface
        println!("Creating a popup menu at index {} with path {:?}", item_index, parent_path);
        // Create submenu surface
        let submenu_width = self.style.calculate_menu_width(&submenu_items) as i32;
        let submenu_height = self.style.calculate_menu_height(&submenu_items) as i32;

        let wl_surface = compositor.create_surface(qh);
        wl_surface.set_buffer_scale(2); // HiDPI support

        // Create positioner
        let positioner = XdgPositioner::new(xdg_shell)
            .map_err(|_| MenuError::SurfaceCreationFailed)?;
        positioner.set_size(submenu_width, submenu_height);
        positioner.set_anchor_rect(
            parent_width - 8, // Position at right edge of parent menu
            item_y_position,
            1,
                self.style.item_height as i32,
            );
            
            use wayland_protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity};
            positioner.set_anchor(Anchor::TopRight);
            positioner.set_gravity(Gravity::BottomRight);

            // Use the parent popup we got earlier
            let popup = Popup::from_surface(
                Some(parent_popup.xdg_surface()),
                &positioner,
                qh,
                wl_surface.clone(),
                xdg_shell,
            )
            .map_err(|_| MenuError::SurfaceCreationFailed)?;

            // Create Skia context and surface
            let (skia_context, skia_surface) = SkiaContext::new(
                display_ptr,
                &wl_surface,
                (submenu_width * 2) as i32,
                (submenu_height * 2) as i32,
            ).map_err(|_| MenuError::SurfaceCreationFailed)?;

            // Don't draw yet - wait for configure event first
            let submenu_surface = MenuSurface {
                wl_surface: wl_surface.clone(),
                popup: Some(popup),
                skia_context,
                skia_surface,
                items: submenu_items,
                width: submenu_width,
                height: submenu_height,
                hovered_item: None,
                needs_redraw: true,
                configured: false,
                frame_callback: None,
                open_submenus: HashMap::new(),
            };

            // Navigate to parent menu and insert the submenu BEFORE committing
            // (this prevents race condition where events check before submenu is inserted)
            let mut insert_current = self.root.as_mut().ok_or(MenuError::SurfaceCreationFailed)?;
            for &idx in &parent_path {
                insert_current = insert_current.open_submenus.get_mut(&idx).ok_or(MenuError::SurfaceCreationFailed)?;
            }
            let old_entry = insert_current.open_submenus.insert(item_index, submenu_surface);
            println!("[open_submenu] Inserted new submenu at path {:?}, item {}. Replaced existing: {}", parent_path, item_index, old_entry.is_some());

            // Commit to trigger configure (after insertion to avoid race condition)
            wl_surface.commit();

        Ok(())
    }
}

/// Represents a single menu surface
struct MenuSurface {
    wl_surface: wl_surface::WlSurface,
    popup: Option<Popup>,
    skia_context: SkiaContext,
    skia_surface: SkiaSurface,
    items: Vec<MenuItem>,
    width: i32,
    height: i32,
    hovered_item: Option<usize>,
    needs_redraw: bool,
    configured: bool,
    frame_callback: Option<wl_callback::WlCallback>,
    open_submenus: HashMap<usize, MenuSurface>,
}

impl MenuSurface {
    /// Recursively handle configure event for this surface or any of its submenus
    fn handle_configure_recursive(&mut self, popup_surface_id: &wayland_client::backend::ObjectId, serial: u32, style: &MenuStyle, depth: usize) -> bool {
        // Check if this is the surface we're looking for
        if let Some(popup) = self.popup.as_ref() {
            if &popup.wl_surface().id() == popup_surface_id {
                println!("Configuring menu at depth {}", depth);
                popup.xdg_surface().ack_configure(serial);
                if !self.configured {
                    self.configured = true;
                }
                self.render(style);
                return true;
            }
        }
        
        // Recursively check all submenus
        for (idx, submenu) in self.open_submenus.iter_mut() {
            if submenu.handle_configure_recursive(popup_surface_id, serial, style, depth + 1) {
                return true;
            }
        }
        
        false
    }
    
    /// Recursively check if this surface or any submenu has a hovered submenu item that should be opened
    /// Returns (parent_index_in_THIS_menu, item_index_to_open)
    fn check_should_open_submenu_recursive(&self, parent_path: &[usize]) -> Option<(Vec<usize>, usize)> {
        // Check if this surface has a hovered submenu item
        if let Some(item_idx) = self.hovered_item {
            if let Some(item) = self.items.get(item_idx) {
                if item.is_submenu() {
                    // Only open if submenu doesn't exist yet or doesn't have a popup
                    // (if popup exists, it's just waiting for configure event)
                    if !self.open_submenus.contains_key(&item_idx) {
                        println!("[check_should_open] Path {:?}, item {} not in open_submenus - SHOULD OPEN", parent_path, item_idx);
                        return Some((parent_path.to_vec(), item_idx));
                    }
                    if let Some(submenu) = self.open_submenus.get(&item_idx) {
                        let has_popup = submenu.popup.is_some();
                        println!("[check_should_open] Path {:?}, item {} exists, has_popup={}, configured={}", parent_path, item_idx, has_popup, submenu.configured);
                        if submenu.popup.is_none() {
                            println!("[check_should_open] Path {:?}, item {} has no popup - SHOULD OPEN", parent_path, item_idx);
                            return Some((parent_path.to_vec(), item_idx));
                        }
                    }
                }
            }
        }
        
        // Recursively check all configured submenus
        for (idx, submenu) in &self.open_submenus {
            if submenu.configured {
                let mut new_path = parent_path.to_vec();
                new_path.push(*idx);
                if let Some(result) = submenu.check_should_open_submenu_recursive(&new_path) {
                    return Some(result);
                }
            }
        }
        
        None
    }
    
    /// Recursively close all submenus of this surface
    fn close_all_submenus_recursive(&mut self) {
        for (_, submenu) in self.open_submenus.iter_mut() {
            // First recursively close nested submenus
            submenu.close_all_submenus_recursive();
            
            // Then close this submenu
            if let Some(popup) = submenu.popup.take() {
                popup.xdg_popup().destroy();
                popup.wl_surface().destroy();
                submenu.configured = false;
            }
            submenu.hovered_item = None;
        }
    }
    
    /// Recursively handle pointer motion for this surface or any submenu
    /// Returns true if the surface was found and handled
    fn handle_pointer_motion_recursive(&mut self, surface: &wl_surface::WlSurface, y: f64, style: &MenuStyle, active_path: &mut Vec<usize>) -> bool {
        // Check if this is the surface we're looking for
        if self.configured && &self.wl_surface == surface {
            // Update this surface's hover state
            let new_hover = self.item_at_position(y as f32, style);
            if self.hovered_item != new_hover {
                self.hovered_item = new_hover;
                self.needs_redraw = true;
                self.render(style);
                
                // Close submenus that don't match the currently hovered item
                let hovered_idx = new_hover;
                for (sub_idx, sub_submenu) in self.open_submenus.iter_mut() {
                    if Some(*sub_idx) != hovered_idx {
                        sub_submenu.close_all_submenus_recursive();
                        if let Some(popup) = sub_submenu.popup.take() {
                            popup.xdg_popup().destroy();
                            popup.wl_surface().destroy();
                            sub_submenu.configured = false;
                        }
                        sub_submenu.hovered_item = None;
                    }
                }
            }
            return true;
        }
        
        // Recursively check all configured submenus
        for (idx, submenu) in self.open_submenus.iter_mut() {
            if submenu.configured {
                active_path.push(*idx);
                if submenu.handle_pointer_motion_recursive(surface, y, style, active_path) {
                    return true;
                }
                active_path.pop();
            }
        }
        
        false
    }
    
    /// Close all submenus except those in the active path
    fn close_inactive_submenus(&mut self, active_path: &[usize]) {
        if active_path.is_empty() {
            // No active path, close everything
            self.close_all_submenus_recursive();
            return;
        }
        
        let current_idx = active_path[0];
        let remaining_path = &active_path[1..];
        
        // First, recurse into the active submenu
        if let Some(submenu) = self.open_submenus.get_mut(&current_idx) {
            submenu.close_inactive_submenus(remaining_path);
        }
        
        // Then close inactive submenus (but keep in HashMap)
        println!("[close_inactive_submenus] Active path: {:?}, current_idx: {}, open_submenus keys: {:?}", active_path, current_idx, self.open_submenus.keys().collect::<Vec<_>>());
        for (idx, submenu) in self.open_submenus.iter_mut() {
            if *idx != current_idx {
                // Not in active path, close it (but keep in HashMap)
                println!("[close_inactive_submenus] Closing submenu {}, not in active path", idx);
                submenu.close_all_submenus_recursive();
                if let Some(popup) = submenu.popup.take() {
                    popup.xdg_popup().destroy();
                    popup.wl_surface().destroy();
                    submenu.configured = false;
                }
                submenu.hovered_item = None;
            } else {
                println!("[close_inactive_submenus] Keeping submenu {}, in active path", idx);
            }
        }
    }
    
    /// Recursively check if this surface or any submenu owns the given surface
    fn owns_surface_recursive(&self, surface: &wl_surface::WlSurface) -> bool {
        // Check this surface
        if self.configured && &self.wl_surface == surface {
            return true;
        }
        
        // Recursively check all submenus
        for submenu in self.open_submenus.values() {
            if submenu.owns_surface_recursive(surface) {
                return true;
            }
        }
        
        false
    }
    
    /// Recursively handle pointer button click - returns true if an item was clicked
    fn handle_pointer_button_recursive<F>(&self, handler: &F) -> bool
    where
        F: Fn(&MenuItemId)
    {
        // Check if this surface has a clicked item
        if let Some(hover_idx) = self.hovered_item {
            if let Some(item) = self.items.get(hover_idx) {
                if !item.is_separator() && !item.is_submenu() && item.is_enabled() {
                    // Clicked an action item
                    if let Some(id) = item.id() {
                        handler(&MenuItemId::from(id));
                        return true;
                    }
                }
            }
        }
        
        // Recursively check submenus
        for submenu in self.open_submenus.values() {
            if submenu.configured && submenu.handle_pointer_button_recursive(handler) {
                return true;
            }
        }
        
        false
    }
    
    fn render(&mut self, style: &MenuStyle) {
        self.skia_surface.draw(&mut self.skia_context, |canvas| {
            draw_menu(
                canvas,
                &self.items,
                self.width as f32,
                self.hovered_item,
                style,
            );
        });
        self.skia_surface.commit();
    }
    
    /// Find which item is at the given Y position
    fn item_at_position(&self, y: f32, style: &MenuStyle) -> Option<usize> {
        let mut current_y = style.padding_vertical;
        let mut item_index = 0;

        for item in &self.items {
            if item.is_separator() {
                current_y += style.separator_height;
            } else {
                let item_bottom = current_y + style.item_height;
                if y >= current_y && y < item_bottom {
                    return Some(item_index); // Returns actual index in items array
                }
                current_y = item_bottom;
            }
            item_index += 1;
        }

        None
    }
    
    fn destroy(&mut self) {
        // frame_callback will be dropped automatically
        // Popup and surface will be destroyed automatically
    }
}

/// Create an XDG positioner for menu placement
fn create_positioner<D>(
    xdg_shell: &XdgShell,
    qh: &QueueHandle<D>,
    items: &[MenuItem],
    position: &Position,
    style: &MenuStyle,
) -> Result<XdgPositioner, MenuError>
where
    D: 'static,
{
    use wayland_protocols::xdg::shell::client::xdg_positioner::{Anchor as WlAnchor, Gravity as WlGravity, ConstraintAdjustment};
    
    let width = style.calculate_menu_width(items) as i32;
    let height = style.calculate_menu_height(items) as i32;

    // Create the positioner
    let positioner = XdgPositioner::new(xdg_shell)
        .map_err(|_| MenuError::SurfaceCreationFailed)?;

    // Set size of the popup
    positioner.set_size(width, height);
    
    // Set anchor rectangle (1x1 point at the specified position)
    positioner.set_anchor_rect(position.x, position.y, 1, 1);

    // Convert our Anchor enum to protocol Anchor
    let anchor = match position.anchor {
        Anchor::TopLeft => WlAnchor::TopLeft,
        Anchor::Top => WlAnchor::Top,
        Anchor::TopRight => WlAnchor::TopRight,
        Anchor::Right => WlAnchor::Right,
        Anchor::BottomRight => WlAnchor::BottomRight,
        Anchor::Bottom => WlAnchor::Bottom,
        Anchor::BottomLeft => WlAnchor::BottomLeft,
        Anchor::Left => WlAnchor::Left,
    };

    // Convert our Gravity enum to protocol Gravity
    let gravity = match position.gravity {
        Gravity::TopLeft => WlGravity::TopLeft,
        Gravity::Top => WlGravity::Top,
        Gravity::TopRight => WlGravity::TopRight,
        Gravity::Right => WlGravity::Right,
        Gravity::BottomRight => WlGravity::BottomRight,
        Gravity::Bottom => WlGravity::Bottom,
        Gravity::BottomLeft => WlGravity::BottomLeft,
        Gravity::Left => WlGravity::Left,
    };

    positioner.set_anchor(anchor);
    positioner.set_gravity(gravity);

    // Allow compositor to adjust position if it would be off-screen
    positioner.set_constraint_adjustment(
        ConstraintAdjustment::FlipX
            | ConstraintAdjustment::FlipY
            | ConstraintAdjustment::SlideX
            | ConstraintAdjustment::SlideY
    );

    Ok(positioner)
}
