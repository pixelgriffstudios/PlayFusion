// Make sure you have the right imports and make your structs public
use crate::audio::SoundEffects;
use crate::config::{get_user_data_dir, Config};
use crate::MenuPosition;
use macroquad::prelude::*; // for load_string
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

// This needs to be public so main.rs can see it
#[derive(Deserialize, Debug, Clone)]
pub struct ThemeConfigFile {
    pub menu_position: Option<String>,
    pub profile_badge_position: Option<String>,
    pub boot_animation: Option<String>,
    pub font_color: Option<String>,
    pub cursor_color: Option<String>,
    pub cursor_style: Option<String>,
    pub cursor_blink_speed: Option<String>,
    pub cursor_transition_speed: Option<String>,
    pub background_scroll_speed: Option<String>,
    pub color_shift_speed: Option<String>,
    pub sfx_pack: Option<String>,
    pub bgm_track: Option<String>,
    pub logo_selection: Option<String>,
    pub background_selection: Option<String>,
    pub font_selection: Option<String>,
}

// This also needs to be public
#[derive(Clone)]
pub struct Theme {
    pub name: String,
    pub sounds: SoundEffects,
    // Add other pre-loaded assets here if you want
    // pub background: Texture2D,
    pub config: ThemeConfigFile, // Store the parsed config
}

fn parse_menu_position(value: &str) -> MenuPosition {
    match value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
        .as_str()
    {
        "topleft" => MenuPosition::TopLeft,
        "topright" => MenuPosition::TopRight,
        "bottomleft" => MenuPosition::BottomLeft,
        "bottomright" => MenuPosition::BottomRight,
        _ => MenuPosition::Center,
    }
}

/// Applies only appearance-related values, preserving system, audio, profile,
/// network, resolution, and screensaver preferences.
pub fn apply_to_config(config: &mut Config, selected_theme: &Theme) {
    let defaults = Config::default();
    let theme_config = &selected_theme.config;

    config.theme = selected_theme.name.clone();
    config.menu_position = theme_config
        .menu_position
        .as_deref()
        .map(parse_menu_position)
        .unwrap_or(defaults.menu_position);
    config.profile_badge_position = theme_config
        .profile_badge_position
        .as_deref()
        .filter(|position| matches!(position.to_ascii_uppercase().as_str(), "LEFT" | "RIGHT"))
        .map(|position| position.to_ascii_uppercase())
        .unwrap_or(defaults.profile_badge_position);
    config.font_color = theme_config
        .font_color
        .clone()
        .unwrap_or(defaults.font_color);
    config.cursor_color = theme_config
        .cursor_color
        .clone()
        .unwrap_or(defaults.cursor_color);
    config.cursor_style = theme_config
        .cursor_style
        .clone()
        .unwrap_or(defaults.cursor_style);
    config.cursor_blink_speed = theme_config
        .cursor_blink_speed
        .clone()
        .unwrap_or(defaults.cursor_blink_speed);
    config.cursor_transition_speed = theme_config
        .cursor_transition_speed
        .clone()
        .unwrap_or(defaults.cursor_transition_speed);
    config.background_scroll_speed = theme_config
        .background_scroll_speed
        .clone()
        .unwrap_or(defaults.background_scroll_speed);
    config.color_shift_speed = theme_config
        .color_shift_speed
        .clone()
        .unwrap_or(defaults.color_shift_speed);
    config.bgm_track = theme_config.bgm_track.clone().or(defaults.bgm_track);
    config.sfx_pack = theme_config.sfx_pack.clone().unwrap_or(defaults.sfx_pack);
    config.logo_selection = theme_config
        .logo_selection
        .clone()
        .unwrap_or(defaults.logo_selection);
    config.background_selection = theme_config
        .background_selection
        .clone()
        .unwrap_or(defaults.background_selection);
    config.font_selection = theme_config
        .font_selection
        .clone()
        .unwrap_or(defaults.font_selection);
}

// LOAD CUSTOM THEMES
pub async fn load_all_themes() -> HashMap<String, Theme> {
    let mut themes = HashMap::new();
    //let default_sfx = SoundEffects::load("Default").await;
    let default_sfx = SoundEffects::load("Default");

    // create a virtual default theme so we don't crash at startup
    let virtual_default_theme = Theme {
        name: "Default".to_string(),
        sounds: default_sfx.clone(), // Use the pre-loaded default sounds
        config: ThemeConfigFile {
            // Create an empty config, just like from an empty theme.toml
            menu_position: None,
            profile_badge_position: None,
            boot_animation: None,
            font_color: None,
            cursor_color: None,
            cursor_style: None,
            cursor_blink_speed: None,
            cursor_transition_speed: None,
            background_scroll_speed: None,
            color_shift_speed: None,
            sfx_pack: None,
            bgm_track: None,
            logo_selection: None,
            background_selection: None,
            font_selection: None,
        },
    };
    // Insert our virtual theme into the map before scanning for others.
    themes.insert("Default".to_string(), virtual_default_theme);

    let themes_dir = match get_user_data_dir() {
        Some(dir) => dir.join("themes"),
        None => return themes,
    };

    // Use synchronous std::fs to list directories. It's simple and efficient here.
    if let Ok(entries) = fs::read_dir(themes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let theme_name = path.file_name().unwrap().to_string_lossy().into_owned();
                let toml_path = path.join("theme.toml");

                if toml_path.exists() {
                    // Use macroquad's async load_string to read file contents
                    if let Ok(content) = load_string(&toml_path.to_string_lossy()).await {
                        if let Ok(config) = toml::from_str::<ThemeConfigFile>(&content) {
                            let sounds = match &config.sfx_pack {
                                //Some(pack_name) => SoundEffects::load(pack_name).await,
                                Some(pack_name) => SoundEffects::load(pack_name),
                                None => default_sfx.clone(),
                            };

                            let loaded_theme = Theme {
                                name: theme_name.clone(),
                                sounds,
                                config,
                            };

                            println!("[INFO] Loaded theme '{}'", theme_name);
                            themes.insert(theme_name, loaded_theme);
                        }
                    }
                }
            }
        }
    }
    themes
}
