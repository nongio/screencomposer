use smithay_client_toolkit::{
    compositor::{CompositorState, SurfaceData},
    reexports::client::{QueueHandle, protocol::wl_surface},
};
use wayland_client::{
    protocol::{wl_subcompositor, wl_subsurface},
    Dispatch,
};

use crate::rendering::{SkiaContext, SkiaSurface};

use super::MenuBar;

/// Surface manager for MenuBar component
pub struct MenuBarSurface {
    /// The Wayland surface
    pub wl_surface: wl_surface::WlSurface,
    /// The subsurface for positioning
    pub subsurface: wl_subsurface::WlSubsurface,
    /// Skia rendering context
    pub skia_context: SkiaContext,
    /// Skia surface for drawing
    pub skia_surface: SkiaSurface,
    /// The menu bar component
    pub menu_bar: MenuBar,
    /// Width of the surface
    pub width: i32,
    /// Height of the surface  
    pub height: i32,
    /// Buffer scale factor
    _buffer_scale: i32,
    /// Whether the surface needs redraw
    pub needs_redraw: bool,
    /// Display pointer for recreating surfaces
    display_ptr: *mut std::ffi::c_void,
}

impl MenuBarSurface {
    /// Create a new menu bar surface as a subsurface of the parent
    pub fn new<D>(
        parent_surface: &wl_surface::WlSurface,
        menu_bar: MenuBar,
        width: i32,
        compositor: &CompositorState,
        subcompositor: &wl_subcompositor::WlSubcompositor,
        qh: &QueueHandle<D>,
        display_ptr: *mut std::ffi::c_void,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        D: Dispatch<wl_surface::WlSurface, SurfaceData> + 
           Dispatch<wl_subsurface::WlSubsurface, ()> + 
           'static,
    {
        let height = menu_bar.height() as i32;
        
        // Create the Wayland surface
        let wl_surface = compositor.create_surface(qh);
        
        // Create subsurface
        let subsurface = subcompositor.get_subsurface(&wl_surface, parent_surface, qh, ());
        
        // Position at top of parent (0, 0)
        subsurface.set_position(0, 0);
        subsurface.set_desync();
        
        // Use 2x buffer for HiDPI rendering
        let buffer_scale = 2;
        wl_surface.set_buffer_scale(buffer_scale);
        
        // Create Skia context and surface
        let (skia_context, skia_surface) = SkiaContext::new(
            display_ptr,
            &wl_surface,
            width * buffer_scale,
            height * buffer_scale,
        )?;
        
        let mut menu_bar_surface = Self {
            wl_surface,
            subsurface,
            skia_context,
            skia_surface,
            menu_bar,
            width,
            height,
            _buffer_scale: buffer_scale,
            needs_redraw: true,
            display_ptr,
        };
        
        // Initial render
        menu_bar_surface.render();
        
        Ok(menu_bar_surface)
    }
    
    /// Render the menu bar
    pub fn render(&mut self) {
        self.skia_surface.draw(&mut self.skia_context, |canvas| {
            self.menu_bar.render(canvas, self.width as f32);
        });
        self.skia_surface.commit();
        self.needs_redraw = false;
    }
    
    /// Handle a click at the given position (in surface-local coordinates)
    /// Returns (label, x_position) of the menu that was toggled, if any
    pub fn handle_click(&mut self, x: f32, y: f32) -> Option<(String, f32)> {
        // Coordinates are already in logical space - no scaling needed
        let result = self.menu_bar.handle_click(x, y);
        if result.is_some() {
            self.needs_redraw = true;
            self.render();
        }
        result
    }
    
    /// Handle hover at the given position (in surface-local coordinates)
    /// Returns (label, x_position, changed) if hovering over a menubar item
    /// changed is true if the active menu was switched
    pub fn handle_hover(&mut self, x: f32, y: f32) -> Option<(String, f32, bool)> {
        // Coordinates are already in logical space - no scaling needed
        if let Some((label, x_pos, changed)) = self.menu_bar.handle_hover(x, y) {
            if changed {
                self.needs_redraw = true;
                self.render();
            }
            Some((label, x_pos, changed))
        } else {
            None
        }
    }
    
    /// Get the menu bar component
    pub fn menu_bar(&self) -> &MenuBar {
        &self.menu_bar
    }
    
    /// Get mutable menu bar component
    pub fn menu_bar_mut(&mut self) -> &mut MenuBar {
        &mut self.menu_bar
    }
    
    /// Get the surface
    pub fn surface(&self) -> &wl_surface::WlSurface {
        &self.wl_surface
    }
    
    /// Resize the surface
    pub fn resize(&mut self, width: i32) {
        if self.width != width {
            self.width = width;
            
            // Recreate Skia surface with new dimensions
            let buffer_scale = 2;
            
            if let Ok((new_ctx, new_surface)) = SkiaContext::new(
                self.display_ptr,
                &self.wl_surface,
                width * buffer_scale,
                self.height * buffer_scale,
            ) {
                self.skia_context = new_ctx;
                self.skia_surface = new_surface;
                self.needs_redraw = true;
                self.render();
            }
        }
    }
}
