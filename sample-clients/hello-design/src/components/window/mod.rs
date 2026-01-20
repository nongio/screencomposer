/// Simple window component with no decorations
/// Just opens a plain window with basic rendering capabilities
pub struct SimpleWindow {
    width: i32,
    height: i32,
    title: String,
    background_color: skia_safe::Color,
}

impl SimpleWindow {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            title: "Simple Window".to_string(),
            background_color: skia_safe::Color::from_rgb(245, 245, 245),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_background(mut self, color: skia_safe::Color) -> Self {
        self.background_color = color;
        self
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Render the simple window content
    pub fn render(&self, canvas: &skia_safe::Canvas) {
        // Clear with background color
        canvas.clear(self.background_color);

        // Draw a simple centered text
        let paint = skia_safe::Paint::new(skia_safe::Color4f::new(0.2, 0.2, 0.2, 1.0), None);

        let font_mgr = skia_safe::FontMgr::new();
        let typeface = font_mgr
            .match_family_style("sans-serif", skia_safe::FontStyle::normal())
            .unwrap_or_else(|| {
                font_mgr
                    .legacy_make_typeface(None, skia_safe::FontStyle::normal())
                    .unwrap()
            });

        let font = skia_safe::Font::from_typeface(typeface, 24.0);

        let text = "Simple Window Component";
        let (_, bounds) = font.measure_str(text, Some(&paint));

        let x = (self.width as f32 - bounds.width()) / 2.0;
        let y = (self.height as f32) / 2.0;

        canvas.draw_str(text, (x, y), &font, &paint);
    }
}

impl Default for SimpleWindow {
    fn default() -> Self {
        Self::new(400, 300)
    }
}
