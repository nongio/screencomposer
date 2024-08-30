#[derive(Debug)]
pub struct Config {
    pub screen_scale: f64,
    pub cursor_theme: String,
    pub cursor_size: u32,
    pub terminal_bin: String,
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
            cursor_size: 32,
            terminal_bin: "kitty".to_string(),
        };
        let scaled_cursor_size = (config.cursor_size as f64) as u32;
        std::env::set_var("XCURSOR_SIZE", (scaled_cursor_size).to_string());
        std::env::set_var("XCURSOR_THEME", config.cursor_theme.clone());
        // std::env::set_var("GDK_DPI_SCALE", (config.screen_scale).to_string());
        config
    }
}
