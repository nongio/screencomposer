use std::fmt;

/// Identifier for a clicked menu item
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuItemId(String);

impl MenuItemId {
    /// Create a new menu item ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MenuItemId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for MenuItemId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for MenuItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A menu item with optional submenu
#[derive(Clone, Debug)]
pub enum MenuItem {
    /// Regular menu item with action
    Action {
        id: String,
        label: String,
        shortcut: Option<String>,
        enabled: bool,
    },
    /// Visual separator
    Separator,
    /// Item that opens a submenu
    Submenu {
        id: String,
        label: String,
        items: Vec<MenuItem>,
        enabled: bool,
    },
}

impl MenuItem {
    /// Create a new action item builder
    pub fn action(id: impl Into<String>, label: impl Into<String>) -> MenuItemBuilder {
        MenuItemBuilder::new(id.into(), label.into())
    }

    /// Create a new submenu builder
    pub fn submenu(id: impl Into<String>, label: impl Into<String>) -> SubmenuBuilder {
        SubmenuBuilder::new(id.into(), label.into())
    }

    /// Create a separator
    pub fn separator() -> Self {
        MenuItem::Separator
    }

    /// Get the ID of this menu item (if it has one)
    pub fn id(&self) -> Option<&str> {
        match self {
            MenuItem::Action { id, .. } => Some(id),
            MenuItem::Submenu { id, .. } => Some(id),
            MenuItem::Separator => None,
        }
    }

    /// Check if this is a separator
    pub fn is_separator(&self) -> bool {
        matches!(self, MenuItem::Separator)
    }

    /// Check if this is a submenu
    pub fn is_submenu(&self) -> bool {
        matches!(self, MenuItem::Submenu { .. })
    }

    /// Get the label of this item
    pub fn label(&self) -> Option<&str> {
        match self {
            MenuItem::Action { label, .. } => Some(label),
            MenuItem::Submenu { label, .. } => Some(label),
            MenuItem::Separator => None,
        }
    }

    /// Check if this item is enabled
    pub fn is_enabled(&self) -> bool {
        match self {
            MenuItem::Action { enabled, .. } => *enabled,
            MenuItem::Submenu { enabled, .. } => *enabled,
            MenuItem::Separator => false,
        }
    }
}

/// Builder for action menu items
pub struct MenuItemBuilder {
    id: String,
    label: String,
    shortcut: Option<String>,
    enabled: bool,
}

impl MenuItemBuilder {
    fn new(id: String, label: String) -> Self {
        Self {
            id,
            label,
            shortcut: None,
            enabled: true,
        }
    }

    /// Set the keyboard shortcut display text
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Set whether this item is enabled
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Build the menu item
    pub fn build(self) -> MenuItem {
        MenuItem::Action {
            id: self.id,
            label: self.label,
            shortcut: self.shortcut,
            enabled: self.enabled,
        }
    }
}

/// Builder for submenu items
pub struct SubmenuBuilder {
    id: String,
    label: String,
    items: Vec<MenuItem>,
    enabled: bool,
}

impl SubmenuBuilder {
    fn new(id: String, label: String) -> Self {
        Self {
            id,
            label,
            items: Vec::new(),
            enabled: true,
        }
    }

    /// Set the submenu items
    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self
    }

    /// Set whether this submenu is enabled
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Build the submenu item
    pub fn build(self) -> MenuItem {
        MenuItem::Submenu {
            id: self.id,
            label: self.label,
            items: self.items,
            enabled: self.enabled,
        }
    }
}

/// Menu visual style configuration
#[derive(Clone, Debug)]
pub struct MenuStyle {
    // Dimensions
    pub item_height: f32,
    pub separator_height: f32,
    pub padding_vertical: f32,
    pub padding_left: f32,
    pub padding_right: f32,
    pub highlight_h_padding: f32,
    pub corner_radius: f32,
    pub min_width: f32,

    // Typography
    pub font_size: f32,
    pub shortcut_font_size: f32,

    // Colors (RGBA 0.0-1.0)
    pub background_color: [f32; 4],
    pub item_text_color: [f32; 4],
    pub item_hover_background: [f32; 4],
    pub item_hover_text: [f32; 4],
    pub separator_color: [f32; 4],
    pub disabled_text_color: [f32; 4],

    // Protocol support
    pub sc_layer: bool, // Whether sc-layer protocol is available for background effects
}

impl Default for MenuStyle {
    fn default() -> Self {
        Self {
            item_height: 24.0,
            separator_height: 9.0,
            padding_vertical: 8.0,
            padding_left: 12.0,
            padding_right: 12.0,
            highlight_h_padding: 6.0,
            corner_radius: 10.0,
            min_width: 280.0,
            font_size: 13.5,
            shortcut_font_size: 13.0,
            background_color: [1.0, 1.0, 1.0, 1.0],
            item_text_color: [0.0, 0.0, 0.0, 0.85],
            item_hover_background: [0.039, 0.51, 1.0, 0.75],
            item_hover_text: [1.0, 1.0, 1.0, 1.0],
            separator_color: [0.0, 0.0, 0.0, 0.1],
            disabled_text_color: [0.0, 0.0, 0.0, 0.25],
            sc_layer: false,
        }
    }
}

impl MenuStyle {
    /// Calculate total menu height for a list of items
    pub fn calculate_menu_height(&self, items: &[MenuItem]) -> f32 {
        let mut height = self.padding_vertical;
        for item in items {
            if item.is_separator() {
                height += self.separator_height;
            } else {
                height += self.item_height;
            }
        }
        height += self.padding_vertical;
        height
    }

    /// Calculate menu width based on content
    pub fn calculate_menu_width(&self, _items: &[MenuItem]) -> f32 {
        // TODO: Measure text to get actual width
        // For now, use min_width
        self.min_width
    }
}

/// Position for menu placement
#[derive(Clone, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub anchor: Anchor,
    pub gravity: Gravity,
}

impl Position {
    /// Create a position at pointer location
    pub fn at_pointer(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            anchor: Anchor::TopLeft,
            gravity: Gravity::BottomRight,
        }
    }
}

/// Anchor point on parent surface
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

/// Gravity direction for menu positioning
#[derive(Clone, Copy, Debug)]
pub enum Gravity {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}
