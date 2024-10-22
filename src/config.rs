#[derive(Debug, Clone)]
pub struct Config {
    pub screen_scale: f64,
    pub cursor_theme: String,
    pub cursor_size: u32,
    pub natural_scroll: bool,
    pub terminal_bin: String,
    pub file_manager_bin: String,
    pub browser_bin: String,
    pub browser_args: Vec<String>,
    pub compositor_mode: String,
}
thread_local! {
    static CONFIG: Config = Config::init();
}
impl Config {
    pub fn with<R>(f: impl FnOnce(&Config) -> R) -> R {
        CONFIG.with(f)
    }
    fn init() -> Self {
        let config = Self {
            screen_scale: 2.0,
            cursor_theme: "Notwaita-Black".to_string(),
            cursor_size: 24,
            natural_scroll: true,
            terminal_bin: "kitty".to_string(),
            file_manager_bin: "dolphin".to_string(),
            browser_bin: "firefox".to_string(),
            browser_args: vec!["".to_string()],
            compositor_mode: "drm".to_string(),
        };
        let scaled_cursor_size = (config.cursor_size as f64) as u32;
        std::env::set_var("XCURSOR_SIZE", (scaled_cursor_size).to_string());
        std::env::set_var("XCURSOR_THEME", config.cursor_theme.clone());
        // std::env::set_var("GDK_DPI_SCALE", (config.screen_scale).to_string());
        config
    }
}
