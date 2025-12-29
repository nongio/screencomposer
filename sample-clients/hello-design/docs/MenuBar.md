# MenuBar Component

The `MenuBar` component provides a horizontal menu bar with toggleable labels, where each label controls showing/hiding a separate menu.

## Features

- **Horizontal Layout**: Menu items are arranged horizontally in a bar
- **Toggle Behavior**: Click a label to open its menu, click again to close
- **Mutual Exclusion**: Only one menu can be open at a time
- **Customizable Styling**: Configure colors, fonts, and spacing
- **Hit Testing**: Proper click and hover detection
- **Skia Rendering**: Hardware-accelerated rendering with Skia

## Usage

### Basic Example

```rust
use hello_design::components::menu::{MenuItem, MenuItemId};
use hello_design::components::menu_bar::MenuBar;

// Create a new menu bar
let mut menu_bar = MenuBar::new()
    .with_height(32.0)
    .with_background(Color::from_rgb(240, 240, 240))
    .with_text_color(Color::from_rgb(40, 40, 40));

// Add File menu
let file_items = vec![
    MenuItem::action("file.new", "New").shortcut("⌘N").build(),
    MenuItem::action("file.open", "Open...").shortcut("⌘O").build(),
    MenuItem::separator(),
    MenuItem::action("file.quit", "Quit").shortcut("⌘Q").build(),
];
menu_bar.add_item("File", file_items);

// Add Edit menu
let edit_items = vec![
    MenuItem::action("edit.undo", "Undo").shortcut("⌘Z").build(),
    MenuItem::action("edit.redo", "Redo").shortcut("⇧⌘Z").build(),
];
menu_bar.add_item("Edit", edit_items);

// Set up click handler
menu_bar.set_on_click(|id: &MenuItemId| {
    println!("Menu item clicked: {}", id);
});
```

### Rendering

```rust
// Render the menu bar
menu_bar.render(canvas, window_width);

// If you have an active menu, render it at the appropriate position
if let Some(active_label) = menu_bar.active_menu() {
    // Calculate position based on the menu item bounds
    let item_bounds = /* get bounds for active menu item */;
    menu_bar.render_menu(canvas, active_label, item_bounds.left, item_bounds.bottom);
}
```

### Handling Input

**Important**: The menu bar must be rendered at least once before hit testing will work, as the bounds for each item are calculated during rendering.

```rust
// First render to calculate bounds
menu_bar.render(canvas, window_width);

// Then handle mouse click
if mouse_clicked {
    if let Some(toggled_label) = menu_bar.handle_click(mouse_x, mouse_y) {
        println!("Toggled menu: {}", toggled_label);
    }
}

// Handle mouse hover (for visual feedback)
if let Some(hovered_label) = menu_bar.handle_hover(mouse_x, mouse_y) {
    println!("Hovering over: {}", hovered_label);
}
```

## API Reference

### MenuBar

#### Constructor

- `MenuBar::new()` - Create a new menu bar with default settings

#### Builder Methods

- `with_height(height: f32)` - Set the menu bar height (default: 32.0)
- `with_background(color: Color)` - Set the background color
- `with_text_color(color: Color)` - Set the text color

#### Methods

- `add_item(&mut self, label: impl Into<String>, items: Vec<MenuItem>)` - Add a menu to the bar
- `set_on_click<F>(&mut self, callback: F)` - Set the callback for menu item clicks
- `toggle_menu(&mut self, label: &str)` - Toggle a menu by its label
- `close_all(&mut self)` - Close all open menus
- `is_menu_open(&self, label: &str) -> bool` - Check if a menu is open
- `active_menu(&self) -> Option<&str>` - Get the currently active menu label
- `get_menu(&self, label: &str) -> Option<&Menu>` - Get a menu by label
- `get_menu_mut(&mut self, label: &str) -> Option<&mut Menu>` - Get a mutable menu by label
- `handle_click(&mut self, x: f32, y: f32) -> Option<String>` - Handle mouse click
- `handle_hover(&self, x: f32, y: f32) -> Option<&str>` - Handle mouse hover
- `render(&mut self, canvas: &Canvas, width: f32)` - Render the menu bar
- `render_menu(&mut self, canvas: &Canvas, label: &str, x: f32, y: f32)` - Render a specific menu
- `height(&self) -> f32` - Get the menu bar height

### MenuBarItem

Represents a single item in the menu bar.

#### Fields (public)

- `label: String` - The displayed label
- `items: Vec<MenuItem>` - The menu items for this menu
- `is_open: bool` - Whether this menu is currently open
- `bounds: Rect` - The hit-test bounds for this label

## Behavior

### Toggle Behavior

- Clicking a menu label toggles that menu open/closed
- Opening a menu automatically closes any other open menu
- Clicking outside the menu bar closes all menus
- Clicking in an empty area of the menu bar closes all menus

### Visual States

The MenuBar supports different visual states:

- **Normal**: Default appearance
- **Active**: When a menu is open, its label has a highlighted background
- **Hover**: (Future) Hovering over a label can show visual feedback

## Customization

### Colors

```rust
let menu_bar = MenuBar::new()
    .with_background(Color::from_rgb(240, 240, 240))  // Background color
    .with_text_color(Color::from_rgb(40, 40, 40));     // Text color
```

The MenuBar also has internal colors for:
- `hover_color` - Highlight color for hover (not yet used in rendering)
- `active_color` - Background color for active/open menu

### Spacing

You can customize spacing by modifying the internal fields:
- `item_padding` - Padding around each label (default: 16.0)
- `bar_padding` - Left/right padding for the entire bar (default: 8.0)

### Font

The font size can be customized via the `font_size` field (default: 14.0).

## Integration with Menu Component

The MenuBar manages multiple `Menu` components internally. Each menu is associated with a label and can be accessed via `get_menu()` or `get_menu_mut()`.

The MenuBar does not currently render the menus themselves - this allows for flexible positioning strategies (e.g., as subsurfaces, popups, or overlays).

## Future Enhancements

Potential improvements:

1. **Hover to Open**: When one menu is open, hovering over another label opens it
2. **Keyboard Navigation**: Arrow keys to navigate between menu labels
3. **Visual Hover State**: Render hover effect on labels
4. **Automatic Menu Rendering**: Built-in rendering of open menus with proper positioning
5. **Icons**: Support for icons alongside labels
6. **Separators**: Visual separators between menu groups
7. **Submenu Indicators**: Visual indicators when menus have submenus

## Example Application

See `examples/menu_bar.rs` for a complete working example demonstrating:
- Creating a MenuBar with multiple menus
- Setting up click handlers
- Toggling menus programmatically
- Checking menu state

Run the example with:

```bash
cargo run --example menu_bar
```
