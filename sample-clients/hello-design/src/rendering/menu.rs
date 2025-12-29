use skia_safe::{Canvas, Color4f, Font, FontMgr, Paint, PaintStyle, Rect, RRect};

#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub is_separator: bool,
}

impl MenuItem {
    pub fn new(label: &str, shortcut: Option<&str>) -> Self {
        Self {
            label: label.to_string(),
            shortcut: shortcut.map(|s| s.to_string()),
            is_separator: false,
        }
    }
    
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: None,
            is_separator: true,
        }
    }
}

pub struct MenuStyle {
    pub item_height: f32,
    pub separator_height: f32,
    pub padding_vertical: f32,
    pub padding_left: f32,
    pub padding_right: f32,
    pub highlight_h_padding: f32,
    pub font_size: f32,
    pub shortcut_font_size: f32,
}

impl Default for MenuStyle {
    fn default() -> Self {
        Self {
            item_height: 22.0,
            separator_height: 9.0,
            padding_vertical: 6.0,
            padding_left: 12.0,
            padding_right: 12.0,
            highlight_h_padding: 6.0,
            font_size: 13.5,
            shortcut_font_size: 13.0,
        }
    }
}

/// Draw a single menu item
/// 
/// # Arguments
/// * `canvas` - The Skia canvas to draw on
/// * `item` - The menu item to draw
/// * `y_position` - The Y position (top) where the item should be drawn
/// * `width` - The logical width of the menu
/// * `is_hovered` - Whether this item is currently hovered
/// * `style` - The menu style settings
/// * `menu_font` - Font for the menu item label
/// * `shortcut_font` - Font for the shortcut text
/// 
/// Returns the Y position after drawing this item
pub fn draw_menu_item(
    canvas: &Canvas,
    item: &MenuItem,
    y_position: f32,
    width: f32,
    is_hovered: bool,
    style: &MenuStyle,
    menu_font: &Font,
    shortcut_font: &Font,
) -> f32 {
    if item.is_separator {
        // Draw separator line
        let y = y_position + style.separator_height / 2.0;
        
        let mut separator_paint = Paint::new(Color4f::new(0.0, 0.0, 0.0, 0.1), None);
        separator_paint.set_anti_alias(true);
        separator_paint.set_stroke_width(1.0);
        separator_paint.set_style(PaintStyle::Stroke);
        
        canvas.draw_line(
            (style.padding_left, y),
            (width - style.padding_right, y),
            &separator_paint
        );
        
        y_position + style.separator_height
    } else {
        // Draw highlight background if hovered
        if is_hovered {
            let highlight_paint = Paint::new(Color4f::new(0.039, 0.51, 1.0, 0.75), None);
            let highlight_rect = RRect::new_rect_xy(
                Rect::from_xywh(
                    style.highlight_h_padding, 
                    y_position, 
                    width - 2.0 * style.highlight_h_padding, 
                    style.item_height
                ),
                5.0, 5.0,
            );
            canvas.draw_rrect(&highlight_rect, &highlight_paint);
        }
        
        // Calculate baseline position for text
        let baseline_y = y_position + style.item_height * 0.68;
        
        // Choose text color based on hover state
        let text_color = if is_hovered {
            Color4f::new(1.0, 1.0, 1.0, 1.0)  // White
        } else {
            Color4f::new(0.0, 0.0, 0.0, 0.85)  // Dark gray
        };
        
        let shortcut_color = if is_hovered {
            Color4f::new(1.0, 1.0, 1.0, 1.0)  // White
        } else {
            Color4f::new(0.0, 0.0, 0.0, 0.25)  // Light gray
        };
        
        let mut text_paint = Paint::new(text_color, None);
        text_paint.set_anti_alias(true);
        
        let mut shortcut_paint = Paint::new(shortcut_color, None);
        shortcut_paint.set_anti_alias(true);
        
        // Draw menu item label
        canvas.draw_str(&item.label, (style.padding_left, baseline_y), menu_font, &text_paint);
        
        // Draw shortcut if present (or placeholder for arrow)
        if let Some(ref shortcut) = item.shortcut {
            // Measure shortcut text to right-align it
            let (shortcut_width, _) = shortcut_font.measure_str(shortcut, Some(&shortcut_paint));
            let shortcut_x = width - style.padding_right - shortcut_width;
            canvas.draw_str(shortcut, (shortcut_x, baseline_y), shortcut_font, &shortcut_paint);
        }
        // TODO: Add arrow icon for submenu items
        
        y_position + style.item_height
    }
}

/// Draw a complete submenu
/// 
/// # Arguments
/// * `canvas` - The Skia canvas to draw on
/// * `items` - The menu items to draw
/// * `width` - The logical width of the menu
/// * `hovered_index` - The index of the currently hovered item (if any)
/// * `style` - The menu style settings
pub fn draw_submenu(
    canvas: &Canvas,
    items: &[MenuItem],
    width: f32,
    hovered_index: Option<usize>,
    style: &MenuStyle,
) {
    // Clear to transparent
    canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
    
    // Calculate total height
    let mut total_height = style.padding_vertical;
    for item in items.iter() {
        if item.is_separator {
            total_height += style.separator_height;
        } else {
            total_height += style.item_height;
        }
    }
    total_height += style.padding_vertical;
        
    // Setup fonts
    let font_mgr = FontMgr::new();
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
    
    let mut menu_font = Font::from_typeface(typeface.clone(), style.font_size);
    menu_font.set_subpixel(true);
    menu_font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
    
    let mut shortcut_font = Font::from_typeface(typeface, style.shortcut_font_size);
    shortcut_font.set_subpixel(true);
    shortcut_font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
    
    // Draw items
    let mut y = style.padding_vertical;
    let mut item_index = 0;
    
    for item in items.iter() {
        let is_hovered = if item.is_separator {
            false
        } else {
            let result = hovered_index == Some(item_index);
            item_index += 1;
            result
        };
        
        y = draw_menu_item(
            canvas,
            item,
            y,
            width,
            is_hovered,
            style,
            &menu_font,
            &shortcut_font,
        );
    }
}
