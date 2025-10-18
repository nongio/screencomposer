use std::collections::{BTreeMap, HashMap};

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
impl Config {
    pub fn with<R>(f: impl FnOnce(&Config) -> R) -> R {
        CONFIG.with(f)
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
}
