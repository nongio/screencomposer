use skia_safe::{Canvas, Color, Color4f, Contains, Font, FontMgr, FontStyle, Paint, Point, Rect};
use std::collections::HashMap;

pub mod surface;

use super::menu::{Menu, MenuItem, MenuItemId};

/// Represents a toggleable menu item in the menu bar
#[derive(Clone)]
pub struct MenuBarItem {
    /// The label displayed in the menu bar
    pub label: String,
    /// The menu items for this menu
    pub items: Vec<MenuItem>,
    /// Whether this menu is currently open
    pub is_open: bool,
    /// The bounds of this label in the menu bar (for hit testing)
    pub bounds: Rect,
}

impl MenuBarItem {
    pub fn new(label: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            label: label.into(),
            items,
            is_open: false,
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}

/// A horizontal menu bar with toggleable labels
/// Each label controls a separate menu component
pub struct MenuBar {
    /// The menu bar items
    items: Vec<MenuBarItem>,
    /// Menus indexed by label
    menus: HashMap<String, Menu>,
    /// Height of the menu bar
    height: f32,
    /// Padding between items
    item_padding: f32,
    /// Left/right padding for the entire bar
    bar_padding: f32,
    /// Currently open menu label
    active_menu: Option<String>,
    /// Background color
    background_color: Color,
    /// Text color
    text_color: Color,
    /// Highlight color for hover
    _hover_color: Color,
    /// Active/open menu color
    active_color: Color,
    /// Font size
    font_size: f32,
    /// Callback for menu item clicks
    on_click: Option<Box<dyn Fn(&MenuItemId)>>,
}

impl MenuBar {
    /// Create a new menu bar
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            menus: HashMap::new(),
            height: 28.0,
            item_padding: 11.0,
            bar_padding: 8.0,
            active_menu: None,
            background_color: Color::from_rgb(240, 240, 240),
            text_color: Color::from_rgb(40, 40, 40),
            _hover_color: Color::from_argb(30, 0, 0, 0),
            active_color: Color::from_argb(50, 0, 0, 0),
            font_size: 13.0,
            on_click: None,
        }
    }

    /// Add a menu item to the menu bar
    pub fn add_item(&mut self, label: impl Into<String>, items: Vec<MenuItem>) -> &mut Self {
        let label_str = label.into();
        let item = MenuBarItem::new(label_str.clone(), items.clone());
        
        let menu = Menu::new(items);
        
        self.items.push(item);
        self.menus.insert(label_str, menu);
        self
    }

    /// Set the callback for menu item clicks
    pub fn set_on_click<F>(&mut self, callback: F)
    where
        F: Fn(&MenuItemId) + 'static,
    {
        self.on_click = Some(Box::new(callback));
    }

    /// Set the height of the menu bar
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Set the background color
    pub fn with_background(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    /// Set the text color
    pub fn with_text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    /// Get the height of the menu bar
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Toggle a menu by label
    pub fn toggle_menu(&mut self, label: &str) {
        if self.active_menu.as_deref() == Some(label) {
            // Close the currently open menu
            self.active_menu = None;
            if let Some(item) = self.items.iter_mut().find(|i| i.label == label) {
                item.is_open = false;
            }
        } else {
            // Close any open menu and open the new one
            if let Some(active) = &self.active_menu {
                if let Some(item) = self.items.iter_mut().find(|i| &i.label == active) {
                    item.is_open = false;
                }
            }
            self.active_menu = Some(label.to_string());
            if let Some(item) = self.items.iter_mut().find(|i| i.label == label) {
                item.is_open = true;
            }
        }
    }

    /// Switch to a specific menu (always opens, doesn't toggle)
    pub fn switch_to_menu(&mut self, label: &str) {
        // Close any currently open menu
        if let Some(active) = &self.active_menu {
            if let Some(item) = self.items.iter_mut().find(|i| &i.label == active) {
                item.is_open = false;
            }
        }
        // Open the new menu
        self.active_menu = Some(label.to_string());
        if let Some(item) = self.items.iter_mut().find(|i| i.label == label) {
            item.is_open = true;
        }
    }

    /// Close all menus
    pub fn close_all(&mut self) {
        self.active_menu = None;
        for item in &mut self.items {
            item.is_open = false;
        }
    }

    /// Check if a menu is open
    pub fn is_menu_open(&self, label: &str) -> bool {
        self.active_menu.as_deref() == Some(label)
    }

    /// Get the currently active menu
    pub fn active_menu(&self) -> Option<&str> {
        self.active_menu.as_deref()
    }

    /// Get a menu by label
    pub fn get_menu(&self, label: &str) -> Option<&Menu> {
        self.menus.get(label)
    }

    /// Get a mutable menu by label
    pub fn get_menu_mut(&mut self, label: &str) -> Option<&mut Menu> {
        self.menus.get_mut(label)
    }

    /// Get all menu labels
    pub fn get_menu_labels(&self) -> Vec<String> {
        self.items.iter().map(|item| item.label.clone()).collect()
    }

    /// Handle a click at the given position
    /// Returns (label, x_position) of the menu that was toggled, if any
    pub fn handle_click(&mut self, x: f32, y: f32) -> Option<(String, f32)> {
        // Check if click is within menu bar bounds
        if y < 0.0 || y > self.height {
            // Click outside menu bar - close all menus
            self.close_all();
            return None;
        }

        // Find which item was clicked
        for item in &self.items {
            if item.bounds.contains(Point::new(x, y)) {
                let label = item.label.clone();
                let x_pos = item.bounds.left;
                self.toggle_menu(&label);
                return Some((label, x_pos));
            }
        }

        // Click on empty area of menu bar - close all
        self.close_all();
        None
    }

    /// Handle hover at the given position
    /// If a menu is already open, automatically switches to the hovered menu
    /// Returns (label, x_position, state_changed) - state_changed is true if a menu was switched
    pub fn handle_hover(&mut self, x: f32, y: f32) -> Option<(String, f32, bool)> {
        if y < 0.0 || y > self.height {
            return None;
        }

        // Find which item is being hovered
        for item in &self.items {
            if item.bounds.contains(Point::new(x, y)) {
                let label = item.label.clone();
                let x_pos = item.bounds.left;
                
                // If a menu is already open, switch to the hovered menu if different
                if self.active_menu.is_some() {
                    let changed = !self.is_menu_open(&label);
                    if changed {
                        self.switch_to_menu(&label);
                    }
                    return Some((label, x_pos, changed));
                }
                
                // Return the hovered item (but don't open it if no menu is active)
                return Some((label, x_pos, false));
            }
        }

        None
    }

    /// Render the menu bar
    pub fn render(&mut self, canvas: &Canvas, _width: f32) {
        // Draw background
        canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
        // Set up font - use Inter with Semi-bold weight
        let font_mgr = FontMgr::new();
        let font_style = FontStyle::new(
            skia_safe::font_style::Weight::SEMI_BOLD,
            skia_safe::font_style::Width::NORMAL,
            skia_safe::font_style::Slant::Upright
        );
        let typeface = font_mgr
            .match_family_style("Inter", font_style)
            .unwrap_or_else(|| font_mgr.legacy_make_typeface(None, font_style).unwrap());
        let mut font = Font::from_typeface(typeface, self.font_size);
        font.set_subpixel(true);
        font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);

        // Render each item
        let mut x_offset = self.bar_padding;
        
        for item in &mut self.items {
            let text_paint = Paint::new(Color4f::from(self.text_color), None);
            
            // Measure text
            let (_, bounds) = font.measure_str(&item.label, Some(&text_paint));
            let text_width = bounds.width();
            let item_width = text_width + self.item_padding * 2.0;
            
            // Update item bounds for hit testing
            item.bounds = Rect::new(x_offset, 0.0, x_offset + item_width, self.height);
            
            // Draw background if active with rounded corners
            if item.is_open {
                let mut active_paint = Paint::new(Color4f::from(self.active_color), None);
                active_paint.set_anti_alias(true);
                let rrect = skia_safe::RRect::new_rect_xy(item.bounds, 4.0, 4.0); // 4px corner radius
                canvas.draw_rrect(rrect, &active_paint);
            }
            
            // Draw text centered in the item
            let text_x = x_offset + self.item_padding;
            let text_y = (self.height + self.font_size) / 2.0 - 2.0; // Rough vertical centering
            
            canvas.draw_str(&item.label, (text_x, text_y), &font, &text_paint);
            
            x_offset += item_width;
        }
    }

    /// Render a specific menu (should be called after render)
    /// This is separated so the menu can be rendered with appropriate positioning
    pub fn render_menu(&mut self, canvas: &Canvas, label: &str, x: f32, y: f32) {
        if let Some(_menu) = self.menus.get_mut(label) {
            // Save canvas state
            canvas.save();
            // Translate to menu position (typically below the menu bar item)
            canvas.translate((x, y));
            // Render menu
            // Note: Menu's render method needs to be public
            // menu.render(canvas);
            canvas.restore();
        }
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_bar_creation() {
        let menu_bar = MenuBar::new();
        assert_eq!(menu_bar.items.len(), 0);
        assert_eq!(menu_bar.active_menu, None);
    }

    #[test]
    fn test_add_item() {
        let mut menu_bar = MenuBar::new();
        menu_bar.add_item("File", vec![]);
        assert_eq!(menu_bar.items.len(), 1);
        assert_eq!(menu_bar.items[0].label, "File");
    }

    #[test]
    fn test_toggle_menu() {
        let mut menu_bar = MenuBar::new();
        menu_bar.add_item("File", vec![]);
        
        assert_eq!(menu_bar.active_menu, None);
        
        menu_bar.toggle_menu("File");
        assert_eq!(menu_bar.active_menu, Some("File".to_string()));
        assert!(menu_bar.is_menu_open("File"));
        
        menu_bar.toggle_menu("File");
        assert_eq!(menu_bar.active_menu, None);
        assert!(!menu_bar.is_menu_open("File"));
    }

    #[test]
    fn test_close_all() {
        let mut menu_bar = MenuBar::new();
        menu_bar.add_item("File", vec![]);
        menu_bar.add_item("Edit", vec![]);
        
        menu_bar.toggle_menu("File");
        assert!(menu_bar.is_menu_open("File"));
        
        menu_bar.close_all();
        assert!(!menu_bar.is_menu_open("File"));
        assert_eq!(menu_bar.active_menu, None);
    }
}
