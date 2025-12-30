# Simple Window Component

A minimal window component for the design system that creates a plain window with no decorations.

## Features

- Clean, minimal window without decorations
- Configurable size and background color
- Simple text rendering
- Easy to customize and extend

## Usage

```rust
use hello_design::components::simple_window::SimpleWindow;

// Create a simple window with default settings (400x300)
let window = SimpleWindow::default();

// Or create with custom dimensions
let window = SimpleWindow::new(800, 600)
    .with_title("My Window")
    .with_background(skia_safe::Color::from_rgb(255, 255, 255));

// Render the window content
window.render(canvas);
```

## API

### Constructor

- `SimpleWindow::new(width: i32, height: i32)` - Create a new window with specified dimensions
- `SimpleWindow::default()` - Create a window with default size (400x300)

### Builder Methods

- `with_title(title: impl Into<String>)` - Set the window title
- `with_background(color: skia_safe::Color)` - Set the background color

### Accessors

- `width() -> i32` - Get window width
- `height() -> i32` - Get window height
- `title() -> &str` - Get window title

### Rendering

- `render(&self, canvas: &skia_safe::Canvas)` - Render the window content

## Example

Run the complete example:

```bash
cargo run -p hello-design --example simple_window
```

This will open a simple window with centered text.

## Extending

The component can be easily extended to add:
- Custom rendering logic
- Event handling
- Interactive elements
- Animations
