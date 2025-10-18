use serde::{Deserialize, Serialize};

pub mod default_apps;
pub mod shortcuts;

use shortcuts::{build_bindings, ShortcutBinding, ShortcutMap};
use toml::map::Entry;
use tracing::warn;

use crate::theme::ThemeScheme;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub font_family: String,
    pub genie_scale: f64,
    pub genie_span: f64,
    pub keyboard_repeat_delay: i32,
    pub keyboard_repeat_rate: i32,
    pub theme_scheme: ThemeScheme,
    pub background_image: String,
    pub locales: Vec<String>,
    #[serde(default = "shortcuts::default_shortcut_map")]
    pub keyboard_shortcuts: ShortcutMap,
    #[serde(skip)]
    #[serde(default)]
    shortcut_bindings: Vec<ShortcutBinding>,
}
thread_local! {
    static CONFIG: Config = Config::init();
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Self {
            screen_scale: 2.0,
            cursor_theme: "Notwaita-Black".to_string(),
            cursor_size: 24,
            natural_scroll: true,
            terminal_bin: "kitty".to_string(),
            file_manager_bin: "dolphin".to_string(),
            browser_bin: "firefox".to_string(),
            browser_args: vec!["".to_string()],
            compositor_mode: "drm".to_string(),
            font_family: "Inter".to_string(),
            genie_scale: 0.5,
            genie_span: 10.0,
            keyboard_repeat_delay: 300,
            keyboard_repeat_rate: 30,
            theme_scheme: ThemeScheme::Light,
            background_image: "./resources/background.jpg".to_string(),
            locales: vec!["en".to_string()],
            keyboard_shortcuts: shortcuts::default_shortcut_map(),
            shortcut_bindings: Vec::new(),
        };
        config.rebuild_shortcut_bindings();
        config
    }
}
impl Config {
    pub fn with<R>(f: impl FnOnce(&Config) -> R) -> R {
        CONFIG.with(f)
    }
    fn init() -> Self {
        let mut merged =
            toml::Value::try_from(Self::default()).expect("default config is always valid toml");

        if let Ok(content) = std::fs::read_to_string("sc_config.toml") {
            match content.parse::<toml::Value>() {
                Ok(value) => merge_value(&mut merged, value),
                Err(err) => warn!("Failed to parse sc_config.toml: {err}"),
            }
        }

        if let Some(backend) = std::env::var("SCREEN_COMPOSER_BACKEND").ok() {
            for candidate in backend_override_candidates(&backend) {
                if let Ok(content) = std::fs::read_to_string(&candidate) {
                    match content.parse::<toml::Value>() {
                        Ok(value) => {
                            merge_value(&mut merged, value);
                            break;
                        }
                        Err(err) => {
                            warn!("Failed to parse {candidate}: {err}");
                        }
                    }
                }
            }
        }

        let mut config: Config = merged.try_into().unwrap_or_else(|err| {
            warn!("Falling back to default config due to invalid overrides: {err}");
            Self::default()
        });

        config.rebuild_shortcut_bindings();
        let scaled_cursor_size = (config.cursor_size as f64) as u32;
        std::env::set_var("XCURSOR_SIZE", (scaled_cursor_size).to_string());
        std::env::set_var("XCURSOR_THEME", config.cursor_theme.clone());
        // std::env::set_var("GDK_DPI_SCALE", (config.screen_scale).to_string());
        config
    }

    fn rebuild_shortcut_bindings(&mut self) {
        self.shortcut_bindings = build_bindings(&self.keyboard_shortcuts);
    }

    pub fn shortcut_bindings(&self) -> &[ShortcutBinding] {
        &self.shortcut_bindings
    }
}

fn merge_value(base: &mut toml::Value, overrides: toml::Value) {
    match (base, overrides) {
        (toml::Value::Table(base_map), toml::Value::Table(override_map)) => {
            for (key, override_value) in override_map {
                match base_map.entry(key) {
                    Entry::Occupied(mut entry) => merge_value(entry.get_mut(), override_value),
                    Entry::Vacant(entry) => {
                        entry.insert(override_value);
                    }
                }
            }
        }
        (base_value, override_value) => {
            *base_value = override_value;
        }
    }
}

fn backend_override_candidates(backend: &str) -> Vec<String> {
    match backend {
        "winit" => vec!["sc_config.winit.toml".into()],
        "tty-udev" => vec![
            "sc_config.tty-udev.toml".into(),
            "sc_config.udev.toml".into(),
        ],
        "x11" => vec!["sc_config.x11.toml".into(), "sc_config.udev.toml".into()],
        other => vec![format!("sc_config.{other}.toml")],
    }
}
