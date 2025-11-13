use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub mod default_apps;
pub mod shortcuts;

use shortcuts::{build_bindings, ShortcutBinding, ShortcutMap};
use toml::map::Entry;
use tracing::warn;

use crate::theme::ThemeScheme;
use smithay::input::keyboard::{keysyms::KEY_NoSymbol, xkb, Keysym, ModifiersState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub screen_scale: f64,
    #[serde(default)]
    pub displays: DisplaysConfig,
    pub cursor_theme: String,
    pub cursor_size: u32,
    pub natural_scroll: bool,
    #[serde(default)]
    pub dock: DockConfig,
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
    #[serde(default)]
    pub modifier_remap: BTreeMap<String, String>,
    #[serde(default)]
    pub key_remap: BTreeMap<String, String>,
    #[serde(skip)]
    #[serde(default)]
    modifier_lookup: HashMap<ModifierKind, ModifierKind>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

impl Default for Config {
    fn default() -> Self {
        let mut config = Self {
            screen_scale: 2.0,
            displays: DisplaysConfig::default(),
            cursor_theme: "Notwaita-Black".to_string(),
            cursor_size: 24,
            natural_scroll: true,
            dock: DockConfig::default(),
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
            modifier_remap: BTreeMap::new(),
            key_remap: BTreeMap::new(),
            modifier_lookup: HashMap::new(),
        };
        config.rebuild_shortcut_bindings();
        config.rebuild_remap_tables();
        config
    }
}
pub const WINIT_DISPLAY_ID: &str = "winit";

impl Config {
    pub fn with<R>(f: impl FnOnce(&Config) -> R) -> R {
        let config = CONFIG.get_or_init(Config::init);
        f(config)
    }
    fn init() -> Self {
        let mut merged =
            toml::Value::try_from(Self::default()).expect("default config is always valid toml");

        if let Ok(content) = std::fs::read_to_string("sc_config.toml") {
            match content.parse::<toml::Value>() {
                Ok(mut value) => {
                    sanitize_remap_tables(&mut value);
                    merge_value(&mut merged, value)
                }
                Err(err) => warn!("Failed to parse sc_config.toml: {err}"),
            }
        }

        if let Ok(backend) = std::env::var("SCREEN_COMPOSER_BACKEND") {
            for candidate in backend_override_candidates(&backend) {
                println!("Trying to load backend override config: {}", &candidate);
                if let Ok(content) = std::fs::read_to_string(&candidate) {
                    match content.parse::<toml::Value>() {
                        Ok(mut value) => {
                            sanitize_remap_tables(&mut value);
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

        sanitize_remap_tables(&mut merged);

        let mut config: Config = merged.try_into().unwrap_or_else(|err| {
            warn!("Falling back to default config due to invalid overrides: {err}");
            Self::default()
        });

        config.rebuild_shortcut_bindings();
        config.rebuild_remap_tables();
        let scaled_cursor_size = (config.cursor_size as f64) as u32;
        std::env::set_var("XCURSOR_SIZE", (scaled_cursor_size).to_string());
        std::env::set_var("XCURSOR_THEME", config.cursor_theme.clone());
        // std::env::set_var("GDK_DPI_SCALE", (config.screen_scale).to_string());
        print!("Config initialized: {:#?}", config.theme_scheme);
        config
    }

    fn rebuild_shortcut_bindings(&mut self) {
        self.shortcut_bindings = build_bindings(&self.keyboard_shortcuts);
    }

    pub fn shortcut_bindings(&self) -> &[ShortcutBinding] {
        &self.shortcut_bindings
    }

    pub fn apply_modifier_remap(&self, state: ModifiersState) -> ModifiersState {
        if self.modifier_lookup.is_empty() {
            return state;
        }

        let mut result = state;
        for &kind in ModifierKind::ALL {
            kind.set(&mut result, false);
        }

        for &kind in ModifierKind::ALL {
            if !kind.get(&state) {
                continue;
            }

            let target = self.modifier_lookup.get(&kind).copied().unwrap_or(kind);
            let already = target.get(&result);
            target.set(&mut result, already || true);
        }

        result
    }

    pub fn parsed_key_remaps(&self) -> Vec<(Keysym, Keysym)> {
        self.key_remap
            .iter()
            .filter_map(|(from, to)| {
                let from_sym = parse_keysym_name(from);
                let to_sym = parse_keysym_name(to);
                match (from_sym, to_sym) {
                    (Some(src), Some(dst)) => Some((src, dst)),
                    (None, _) => {
                        warn!(from = %from, "unknown keysym in key_remap entry");
                        None
                    }
                    (_, None) => {
                        warn!(to = %to, "unknown target keysym in key_remap entry");
                        None
                    }
                }
            })
            .collect()
    }

    pub fn resolve_display_profile<'a>(
        &self,
        name: &str,
        descriptor: &DisplayDescriptor<'a>,
    ) -> Option<DisplayProfile> {
        self.displays.resolve(name, descriptor)
    }

    fn rebuild_remap_tables(&mut self) {
        self.modifier_lookup.clear();
        for (from, to) in &self.modifier_remap {
            match (parse_modifier_kind(from), parse_modifier_kind(to)) {
                (Some(from_kind), Some(to_kind)) => {
                    if self.modifier_lookup.insert(from_kind, to_kind).is_some() {
                        warn!(from = %from, "duplicate modifier remap entry; last value wins");
                    }
                }
                (None, _) => warn!(from = %from, "unknown modifier in remap entry"),
                (_, None) => warn!(to = %to, "unknown modifier target in remap entry"),
            }
        }
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

fn sanitize_remap_tables(value: &mut toml::Value) {
    if let toml::Value::Table(table) = value {
        if let Some(key_remap) = table.get_mut("key_remap") {
            if let toml::Value::Table(map) = key_remap {
                let invalid_keys: Vec<String> = map
                    .iter()
                    .filter(|(_, v)| !matches!(v, toml::Value::String(_)))
                    .map(|(k, _)| k.clone())
                    .collect();

                for key in invalid_keys {
                    warn!(key, "ignoring key remap entry with non-string target");
                    map.remove(&key);
                }
            } else {
                warn!("ignoring malformed key_remap table");
                table.remove("key_remap");
            }
        }

        if let Some(mod_remap) = table.get_mut("modifier_remap") {
            if let toml::Value::Table(map) = mod_remap {
                let invalid_keys: Vec<String> = map
                    .iter()
                    .filter(|(_, v)| !matches!(v, toml::Value::String(_)))
                    .map(|(k, _)| k.clone())
                    .collect();

                for key in invalid_keys {
                    warn!(key, "ignoring modifier remap entry with non-string target");
                    map.remove(&key);
                }
            } else {
                warn!("ignoring malformed modifier_remap table");
                table.remove("modifier_remap");
            }
        }

        for (_, value) in table.iter_mut() {
            sanitize_remap_tables(value);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DockConfig {
    #[serde(default)]
    pub bookmarks: Vec<DockBookmark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockBookmark {
    pub desktop_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub exec_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplaysConfig {
    #[serde(default)]
    pub named: BTreeMap<String, DisplayProfile>,
    #[serde(default)]
    pub generic: Vec<DisplayProfileMatch>,
}

impl DisplaysConfig {
    pub fn resolve<'a>(
        &self,
        name: &str,
        descriptor: &DisplayDescriptor<'a>,
    ) -> Option<DisplayProfile> {
        if let Some(profile) = self.named.get(name) {
            return Some(profile.clone());
        }

        self.generic
            .iter()
            .find(|entry| entry.matcher.matches(name, descriptor))
            .map(|entry| entry.profile.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplayProfile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub resolution: Option<DisplayResolution>,
    #[serde(default)]
    pub refresh_hz: Option<f64>,
    #[serde(default)]
    pub position: Option<DisplayPosition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayResolution {
    pub width: u32,
    pub height: u32,
}

impl DisplayResolution {
    pub fn as_f64(self) -> (f64, f64) {
        (self.width as f64, self.height as f64)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DisplayPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplayProfileMatch {
    #[serde(default, rename = "match")]
    pub matcher: DisplayMatcher,
    #[serde(flatten)]
    pub profile: DisplayProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplayMatcher {
    #[serde(default)]
    pub connector: Option<String>,
    #[serde(default)]
    pub connector_prefix: Option<String>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub kind: Option<DisplayKind>,
}

impl DisplayMatcher {
    fn matches(&self, connector: &str, descriptor: &DisplayDescriptor<'_>) -> bool {
        if let Some(expected) = &self.connector {
            if expected != connector && descriptor.connector != expected {
                return false;
            }
        }

        if let Some(prefix) = &self.connector_prefix {
            let matches_actual = connector.starts_with(prefix);
            let matches_descriptor = descriptor.connector.starts_with(prefix);
            if !matches_actual && !matches_descriptor {
                return false;
            }
        }

        if let Some(expected_vendor) = &self.vendor {
            match descriptor.vendor {
                Some(vendor) if equals_ignore_case(vendor, expected_vendor) => {}
                _ => return false,
            }
        }

        if let Some(expected_model) = &self.model {
            match descriptor.model {
                Some(model) if equals_ignore_case(model, expected_model) => {}
                _ => return false,
            }
        }

        if let Some(expected_kind) = self.kind {
            if descriptor.kind.unwrap_or(DisplayKind::Unknown) != expected_kind {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayKind {
    Internal,
    External,
    Virtual,
    Unknown,
}

impl Default for DisplayKind {
    fn default() -> Self {
        DisplayKind::Unknown
    }
}

#[derive(Debug, Clone)]
pub struct DisplayDescriptor<'a> {
    pub connector: &'a str,
    pub vendor: Option<&'a str>,
    pub model: Option<&'a str>,
    pub kind: Option<DisplayKind>,
}

impl<'a> DisplayDescriptor<'a> {
    pub fn new(connector: &'a str) -> Self {
        Self {
            connector,
            vendor: None,
            model: None,
            kind: None,
        }
    }
}

fn equals_ignore_case(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ModifierKind {
    Ctrl,
    Alt,
    Shift,
    Logo,
    CapsLock,
    NumLock,
}

impl ModifierKind {
    const ALL: &'static [ModifierKind] = &[
        ModifierKind::Ctrl,
        ModifierKind::Alt,
        ModifierKind::Shift,
        ModifierKind::Logo,
        ModifierKind::CapsLock,
        ModifierKind::NumLock,
    ];

    fn get(self, state: &ModifiersState) -> bool {
        match self {
            ModifierKind::Ctrl => state.ctrl,
            ModifierKind::Alt => state.alt,
            ModifierKind::Shift => state.shift,
            ModifierKind::Logo => state.logo,
            ModifierKind::CapsLock => state.caps_lock,
            ModifierKind::NumLock => state.num_lock,
        }
    }

    fn set(self, state: &mut ModifiersState, value: bool) {
        match self {
            ModifierKind::Ctrl => state.ctrl = value,
            ModifierKind::Alt => state.alt = value,
            ModifierKind::Shift => state.shift = value,
            ModifierKind::Logo => state.logo = value,
            ModifierKind::CapsLock => state.caps_lock = value,
            ModifierKind::NumLock => state.num_lock = value,
        }
    }
}

fn parse_modifier_kind(input: &str) -> Option<ModifierKind> {
    match input.trim().to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "primary" => Some(ModifierKind::Ctrl),
        "alt" | "option" => Some(ModifierKind::Alt),
        "shift" => Some(ModifierKind::Shift),
        "logo" | "win" | "super" | "meta" | "command" | "cmd" => Some(ModifierKind::Logo),
        "caps" | "capslock" | "caps_lock" => Some(ModifierKind::CapsLock),
        "num" | "numlock" | "num_lock" => Some(ModifierKind::NumLock),
        _ => None,
    }
}

fn parse_keysym_name(name: &str) -> Option<Keysym> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sym = xkb::keysym_from_name(trimmed, xkb::KEYSYM_CASE_INSENSITIVE);
    if sym.raw() == KEY_NoSymbol {
        None
    } else {
        Some(sym)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_remap_logo_to_ctrl() {
        let mut config = Config::default();
        config.modifier_remap.insert("logo".into(), "ctrl".into());
        config.rebuild_remap_tables();

        let mut mods = ModifiersState::default();
        mods.logo = true;

        let remapped = config.apply_modifier_remap(mods);
        assert!(remapped.ctrl);
        assert!(!remapped.logo);
    }

    #[test]
    fn key_remap_backspace_to_delete() {
        let mut config = Config::default();
        config.key_remap.insert("BackSpace".into(), "Delete".into());
        config.rebuild_remap_tables();

        let backspace = xkb::keysym_from_name("BackSpace", xkb::KEYSYM_NO_FLAGS);
        let delete = xkb::keysym_from_name("Delete", xkb::KEYSYM_NO_FLAGS);

        let entries = config.parsed_key_remaps();
        assert!(entries.contains(&(backspace, delete)));
    }

    #[test]
    fn config_loads_shortcuts_and_remaps_from_toml() {
        let overrides = r#"
            [keyboard_shortcuts]
            "Logo+Q" = "Quit"

            [key_remap]
            BackSpace = "Delete"
            BadEntry = ["not", "a", "string"]
        "#;

        let mut merged = toml::Value::try_from(Config::default()).unwrap();
        let mut overrides_value = overrides.parse::<toml::Value>().unwrap();
        sanitize_remap_tables(&mut overrides_value);
        merge_value(&mut merged, overrides_value);
        sanitize_remap_tables(&mut merged);

        let mut config: Config = merged.try_into().expect("config should deserialize");
        config.rebuild_shortcut_bindings();
        config.rebuild_remap_tables();

        assert_eq!(config.shortcut_bindings().len(), 1);
        assert!(config
            .shortcut_bindings()
            .iter()
            .any(|binding| binding.trigger_repr == "Logo+Q"));

        let backspace = xkb::keysym_from_name("BackSpace", xkb::KEYSYM_NO_FLAGS);
        let delete = xkb::keysym_from_name("Delete", xkb::KEYSYM_NO_FLAGS);
        let entries = config.parsed_key_remaps();
        assert!(entries.contains(&(backspace, delete)));

        assert!(config.key_remap.keys().all(|key| key != "BadEntry"));
    }

    #[test]
    fn theme_scheme_defaults_to_light() {
        let config = Config::default();
        assert!(matches!(config.theme_scheme, ThemeScheme::Light));
    }

    #[test]
    fn theme_scheme_overrides_to_dark_in_toml() {
        let overrides = r#"
            theme_scheme = "Dark"
        "#;

        let config: Config = toml::from_str(overrides).expect("Config should deserialize");
        assert!(matches!(config.theme_scheme, ThemeScheme::Dark));
    }
}
