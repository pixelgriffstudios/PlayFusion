use crate::{
    audio::SoundEffects,
    config::{get_user_data_dir, Config},
    draw_configured_cursor_frame, get_current_font, render_background, text_with_config_color,
    wrap_text, BackgroundState, InputState, Screen, VideoPlayer, FONT_SIZE,
};
use macroquad::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
};
use toml;

// --- CONSTANTS ---
const ITEMS_PER_PAGE: usize = 4;
const TOOL_OPTION_COUNT: usize = 5;
const MAX_THEME_PREVIEW_BYTES: usize = 8 * 1024 * 1024;

// --- State Management & Structs ---

pub enum DownloaderState {
    Idle,
    FetchingList,
    DisplayingList,
    Downloading(String),
    Success(String),
    Error(String),
    ConfirmDelete {
        theme_folder_name: String,
        theme_display_name: String,
        selection: usize,
    },
    ConfirmRedownload {
        theme: RemoteTheme,
        selection: usize, // 0=Yes, 1=No
    },
    ConfirmConvertToWav {
        selection: usize,
    }, // 0=Yes, 1=No
    ConfirmConvertToOgg {
        selection: usize,
    }, // 0=Yes, 1=No
    ConfirmDeleteAllBGM {
        selection: usize,
    },
    Converting(String), // Shows progress message, e.g., "Converting files..."
}

enum DownloaderMessage {
    ThemeList(Result<Vec<RemoteTheme>, String>),
    InstallResult(Result<String, String>),
    ConversionResult(Result<String, String>), // -- NEW -- For audio conversion success/error
}

#[derive(Debug, Clone)]
pub struct RemoteTheme {
    pub name: String,        // Display name, e.g., "Soul Calibur II"
    pub folder_name: String, // Directory name, e.g., "soul_calibur_ii"
    pub author: String,
    pub description: String,
    pub download_url: String,
    pub source: String,
    pub preview_bytes: Option<Vec<u8>>,
    pub is_installed: bool,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct ThemeToml {
    author: Option<String>,
    description: Option<String>,
    menu_position: Option<String>,
    profile_badge_position: Option<String>,
    boot_animation: Option<String>,
    font_color: Option<String>,
    cursor_color: Option<String>,
    background_scroll_speed: Option<String>,
    color_shift_speed: Option<String>,
    bgm_track: Option<String>,
    logo_selection: Option<String>,
    background_selection: Option<String>,
    font_selection: Option<String>,
    sfx_pack: Option<String>,
}

pub struct ThemeDownloaderState {
    pub screen_state: DownloaderState,
    pub themes: Vec<RemoteTheme>,
    pub selected_index: usize,
    rx: Receiver<DownloaderMessage>,
    tx: Sender<DownloaderMessage>,
    pub has_audio_tools_option: bool,
    pub current_page: usize,
    preview_textures: HashMap<String, Texture2D>,
}

#[derive(Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    name: String,
    body: String,
    assets: Vec<GithubReleaseAsset>,
}

// --- Implementation ---

impl ThemeDownloaderState {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            screen_state: DownloaderState::Idle,
            themes: Vec::new(),
            selected_index: 0,
            rx,
            tx,
            has_audio_tools_option: true,
            current_page: 0,
            preview_textures: HashMap::new(),
        }
    }

    fn start_fetch(&mut self) {
        fetch_theme_list(self.tx.clone());
        self.screen_state = DownloaderState::FetchingList;
    }
}

pub fn update(
    state: &mut ThemeDownloaderState,
    input_state: &InputState,
    current_screen: &mut Screen,
    sound_effects: &SoundEffects,
    config: &mut Config,
    loaded_themes: &HashMap<String, crate::theme::Theme>,
) {
    if input_state.back {
        sound_effects.play_back(config);
        match &state.screen_state {
            DownloaderState::DisplayingList => {
                *current_screen = Screen::Extras;
                state.screen_state = DownloaderState::Idle; // Reset for next time
            }
            _ => {
                // For any sub-menu, go back to the list and reset page
                state.screen_state = DownloaderState::DisplayingList;
                state.current_page = state.selected_index / ITEMS_PER_PAGE;
            }
        }
        return;
    }

    if let Ok(msg) = state.rx.try_recv() {
        match msg {
            DownloaderMessage::ThemeList(Ok(mut themes)) => {
                // Make themes mutable
                // Get a list of folders currently installed
                let installed_themes = get_installed_theme_folders();

                // Check each remote theme against the installed list
                for theme in themes.iter_mut() {
                    if installed_themes.contains(&theme.folder_name) {
                        theme.is_installed = true;
                        if theme.preview_bytes.is_none() {
                            theme.preview_bytes = local_theme_preview(&theme.folder_name);
                        }
                    }
                }

                // USB/imported themes may not exist in the online release
                // catalog. Keep them visible so they can still be applied.
                for folder_name in installed_themes {
                    if themes.iter().any(|theme| theme.folder_name == folder_name) {
                        continue;
                    }
                    themes.push(local_theme_entry(&folder_name));
                }
                themes.sort_by(|left, right| {
                    left.name.to_lowercase().cmp(&right.name.to_lowercase())
                });

                state.preview_textures.clear();
                for theme in themes.iter_mut() {
                    if let Some(bytes) = theme.preview_bytes.take() {
                        let texture = Texture2D::from_file_with_format(&bytes, None);
                        texture.set_filter(FilterMode::Linear);
                        state
                            .preview_textures
                            .insert(theme.folder_name.clone(), texture);
                    }
                }

                state.themes = themes;
                state.screen_state = DownloaderState::DisplayingList;
            }
            DownloaderMessage::ThemeList(Err(e)) => {
                // Theme activation and reset must keep working offline. Show
                // locally installed themes and management tools even when the
                // GitHub catalog cannot be reached.
                state.themes = get_installed_theme_folders()
                    .into_iter()
                    .map(|folder_name| local_theme_entry(&folder_name))
                    .collect();
                state.preview_textures.clear();
                for theme in state.themes.iter_mut() {
                    if let Some(bytes) = theme.preview_bytes.take() {
                        let texture = Texture2D::from_file_with_format(&bytes, None);
                        texture.set_filter(FilterMode::Linear);
                        state
                            .preview_textures
                            .insert(theme.folder_name.clone(), texture);
                    }
                }
                state.themes.sort_by(|left, right| {
                    left.name.to_lowercase().cmp(&right.name.to_lowercase())
                });
                eprintln!("[Theme Manager] Online catalog unavailable: {e}");
                state.screen_state = DownloaderState::DisplayingList;
            }
            DownloaderMessage::InstallResult(Ok(theme_name)) => {
                state.screen_state =
                    DownloaderState::Success(format!("'{}' installed!", theme_name));
                *current_screen = Screen::ReloadingThemes;
            }
            DownloaderMessage::InstallResult(Err(e)) => {
                state.screen_state = DownloaderState::Error(e);
            }
            DownloaderMessage::ConversionResult(Ok(msg)) => {
                state.screen_state = DownloaderState::Success(msg);
                *current_screen = Screen::ReloadingThemes; // reload assets whenever we delete or convert BGM tracks
            }
            DownloaderMessage::ConversionResult(Err(e)) => {
                state.screen_state = DownloaderState::Error(e);
            }
        }
    }

    // if the screen is idle, trigger a new fetch.
    if let DownloaderState::Idle = state.screen_state {
        state.start_fetch();
    }

    match &mut state.screen_state {
        DownloaderState::DisplayingList => {
            let total_options = state.themes.len()
                + if state.has_audio_tools_option {
                    TOOL_OPTION_COUNT
                } else {
                    0
                };
            if total_options == 0 {
                return;
            }

            let old_selection = state.selected_index;
            if input_state.down {
                state.selected_index = (state.selected_index + 2).min(total_options - 1);
            }
            if input_state.up {
                state.selected_index = state.selected_index.saturating_sub(2);
            }
            if input_state.right {
                state.selected_index = (state.selected_index + 1).min(total_options - 1);
            }
            if input_state.left {
                state.selected_index = state.selected_index.saturating_sub(1);
            }
            if state.selected_index != old_selection {
                sound_effects.play_cursor_move(config);
            }

            // Auto-update current page based on selection
            state.current_page = state.selected_index / ITEMS_PER_PAGE;

            // Handle selection
            if input_state.select {
                sound_effects.play_select(config);
                if state.selected_index < state.themes.len() {
                    let theme = state.themes[state.selected_index].clone();

                    if theme.is_installed {
                        if let Some(installed_theme) = loaded_themes.get(&theme.folder_name) {
                            crate::theme::apply_to_config(config, installed_theme);
                            config.save();
                            state.screen_state = DownloaderState::Success(format!(
                                "'{}' applied. Your other settings were preserved.",
                                theme.name
                            ));
                            *current_screen = Screen::ReloadingThemes;
                        } else {
                            state.screen_state = DownloaderState::Error(format!(
                                "'{}' is installed but its theme.toml could not be loaded.",
                                theme.name
                            ));
                        }
                    } else {
                        // Not installed, download immediately
                        state.screen_state = DownloaderState::Downloading(theme.name.clone());
                        download_and_extract_theme(theme, state.tx.clone());
                    }
                } else {
                    // This is the existing logic for audio tools
                    let tool_index = state.selected_index - state.themes.len();
                    if tool_index == 0 {
                        state.screen_state = DownloaderState::Converting(
                            "Scanning USB/SD for themes...".to_string(),
                        );
                        import_themes_from_usb(state.tx.clone());
                    } else if tool_index == 1 {
                        if let Some(default_theme) = loaded_themes.get("Default") {
                            crate::theme::apply_to_config(config, default_theme);
                            config.save();
                            state.screen_state = DownloaderState::Success(
                                "PlayFusion default theme restored. Your other settings were preserved."
                                    .to_string(),
                            );
                            *current_screen = Screen::ReloadingThemes;
                        } else {
                            state.screen_state = DownloaderState::Error(
                                "The built-in PlayFusion default theme is unavailable.".to_string(),
                            );
                        }
                    } else if tool_index == 2 {
                        state.screen_state = DownloaderState::ConfirmConvertToWav { selection: 1 };
                    } else if tool_index == 3 {
                        state.screen_state = DownloaderState::ConfirmConvertToOgg { selection: 1 };
                    } else if tool_index == 4 {
                        // New option
                        state.screen_state = DownloaderState::ConfirmDeleteAllBGM { selection: 1 };
                    }
                }
            }
            // Handle delete
            if input_state.secondary && state.selected_index < state.themes.len() {
                let theme_to_delete = &state.themes[state.selected_index];

                // Only allow deletion if the theme is installed AND it's not the "Default" theme
                if theme_to_delete.is_installed && theme_to_delete.name != "Default" {
                    sound_effects.play_select(config); // Or a "delete" sound
                    state.screen_state = DownloaderState::ConfirmDelete {
                        theme_folder_name: theme_to_delete.folder_name.clone(),
                        theme_display_name: theme_to_delete.name.clone(),
                        selection: 1, // Default to "NO"
                    };
                } else {
                    // Play reject sound if theme is not installed or is "Default"
                    sound_effects.play_reject(config);
                }
            }
        }
        DownloaderState::ConfirmDelete {
            theme_folder_name,
            theme_display_name,
            selection,
        } => {
            if input_state.left || input_state.right {
                *selection = 1 - *selection;
                sound_effects.play_cursor_move(&config);
            }
            if input_state.select {
                sound_effects.play_select(config);
                if *selection == 0 {
                    let theme_path = get_user_data_dir()
                        .unwrap()
                        .join("themes")
                        .join(theme_folder_name.as_str());
                    if config.theme == theme_folder_name.as_str() {
                        if let Some(default_theme) = loaded_themes.get("Default") {
                            crate::theme::apply_to_config(config, default_theme);
                            config.save();
                        }
                    }
                    match fs::remove_dir_all(&theme_path) {
                        Ok(_) => {
                            state.screen_state = DownloaderState::Success(format!(
                                "'{}' deleted.",
                                theme_display_name
                            ));
                            *current_screen = Screen::ReloadingThemes;
                        }
                        Err(e) => {
                            state.screen_state =
                                DownloaderState::Error(format!("Failed to delete: {}", e));
                        }
                    }
                } else {
                    state.screen_state = DownloaderState::DisplayingList;
                }
            }
            if input_state.back {
                sound_effects.play_back(config);
                state.screen_state = DownloaderState::DisplayingList;
            }
        }
        DownloaderState::ConfirmRedownload { theme, selection } => {
            if input_state.left || input_state.right {
                *selection = 1 - *selection; // Flips between 0 (Yes) and 1 (No)
                sound_effects.play_cursor_move(config);
            }
            if input_state.select {
                sound_effects.play_select(config);
                if *selection == 0 {
                    // User selected YES
                    // Clone the theme *before* changing the state,
                    // so we are not using the borrowed `theme` variable after the state change.
                    let theme_to_download = theme.clone();

                    state.screen_state =
                        DownloaderState::Downloading(theme_to_download.name.clone());
                    download_and_extract_theme(theme_to_download, state.tx.clone());
                } else {
                    // User selected NO
                    state.screen_state = DownloaderState::DisplayingList;
                }
            }
            // Back button also cancels
            if input_state.back {
                sound_effects.play_back(config);
                state.screen_state = DownloaderState::DisplayingList;
            }
        }
        DownloaderState::ConfirmConvertToWav { selection } => {
            if input_state.left || input_state.right {
                *selection = 1 - *selection;
                sound_effects.play_cursor_move(&config);
            }
            if input_state.select {
                sound_effects.play_select(config);
                if *selection == 0 {
                    // YES
                    state.screen_state =
                        DownloaderState::Converting("Searching for .ogg files...".to_string());
                    convert_files_to_wav(state.tx.clone());
                } else {
                    // NO
                    state.screen_state = DownloaderState::DisplayingList;
                }
            }
        }
        DownloaderState::ConfirmConvertToOgg { selection } => {
            if input_state.left || input_state.right {
                *selection = 1 - *selection;
                sound_effects.play_cursor_move(&config);
            }
            if input_state.select {
                sound_effects.play_select(config);
                if *selection == 0 {
                    // YES
                    state.screen_state =
                        DownloaderState::Converting("Searching for .wav files...".to_string());
                    convert_files_to_ogg(state.tx.clone());
                } else {
                    // NO
                    state.screen_state = DownloaderState::DisplayingList;
                }
            }
        }
        DownloaderState::ConfirmDeleteAllBGM { selection } => {
            if input_state.left || input_state.right {
                *selection = 1 - *selection;
                sound_effects.play_cursor_move(config);
            }
            if input_state.select {
                sound_effects.play_select(config);
                if *selection == 0 {
                    // YES
                    state.screen_state =
                        DownloaderState::Converting("Deleting all BGM files...".to_string());
                    delete_all_bgm_files(state.tx.clone()); // Call the new function
                } else {
                    // NO
                    state.screen_state = DownloaderState::DisplayingList;
                }
            }
            if input_state.back {
                sound_effects.play_back(config);
                state.screen_state = DownloaderState::DisplayingList;
            }
        }
        DownloaderState::Success(_) | DownloaderState::Error(_) => {
            if input_state.select || input_state.back {
                // After success/error, go back to the list.
                // Setting to Idle will trigger a re-fetch of the remote list.
                state.screen_state = DownloaderState::Idle;
                sound_effects.play_select(config);
            }
        }
        _ => {}
    }
}

pub fn draw(
    state: &ThemeDownloaderState,
    animation_state: &mut crate::AnimationState,
    background_cache: &HashMap<String, Texture2D>,
    video_cache: &mut HashMap<String, VideoPlayer>,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    background_state: &mut BackgroundState,
    scale_factor: f32,
) {
    render_background(&background_cache, video_cache, &config, background_state);

    let font = get_current_font(font_cache, config);
    let font_size = (FONT_SIZE as f32 * scale_factor) as u16;
    let line_height = font_size as f32 * 1.5;

    // Create a container for the UI
    let container_w = screen_width() * 0.9;
    let container_h = screen_height() * 0.8;
    let container_x = (screen_width() - container_w) / 2.0;
    let container_y = (screen_height() - container_h) / 2.0;
    draw_rectangle(
        container_x,
        container_y,
        container_w,
        container_h,
        Color::new(0.0, 0.0, 0.0, 0.75),
    );

    let text_x = container_x + 30.0 * scale_factor;
    let text_y_start = container_y + 40.0 * scale_factor;

    match &state.screen_state {
        DownloaderState::Idle => {
            let text = "Connecting to theme repository...";
            let text_dims = measure_text(text, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                text,
                screen_width() / 2.0 - text_dims.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );
        }
        DownloaderState::FetchingList => {
            let text = "Fetching theme list from GitHub...";
            let text_dims = measure_text(text, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                text,
                screen_width() / 2.0 - text_dims.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );
        }
        DownloaderState::DisplayingList => {
            let total_options = state.themes.len()
                + if state.has_audio_tools_option {
                    TOOL_OPTION_COUNT
                } else {
                    0
                };
            if total_options == 0 {
                text_with_config_color(
                    font_cache,
                    config,
                    "No themes or tools available.",
                    text_x,
                    text_y_start,
                    font_size,
                );
                return;
            }
            let total_pages = (total_options + ITEMS_PER_PAGE - 1) / ITEMS_PER_PAGE;
            let start_index = state.current_page * ITEMS_PER_PAGE;
            let end_index = (start_index + ITEMS_PER_PAGE).min(total_options);

            // Controller-friendly 2x2 gallery. Remote themes use a branded
            // fallback tile; installed themes can still supply their own
            // backgrounds and folder artwork once applied.
            let grid_x = container_x + container_w * 0.04;
            let grid_y = container_y + container_h * 0.10;
            let grid_w = container_w * 0.92;
            let grid_h = container_h * 0.47;
            let gap_x = container_w * 0.025;
            let gap_y = container_h * 0.025;
            let card_w = (grid_w - gap_x) * 0.5;
            let card_h = (grid_h - gap_y) * 0.5;
            let card_font_size = (font_size as f32 * 0.68).max(9.0) as u16;
            let source_font_size = (font_size as f32 * 0.52).max(8.0) as u16;
            for i in start_index..end_index {
                let item_on_page = i - start_index;
                let column = item_on_page % 2;
                let row = item_on_page / 2;
                let card_x = grid_x + column as f32 * (card_w + gap_x);
                let card_y = grid_y + row as f32 * (card_h + gap_y);
                let is_selected = i == state.selected_index;
                let (display_text, source_text, installed_text, tile_color) = if i < state.themes.len() {
                    let theme = &state.themes[i];
                    let installed_flag = if config.theme == theme.folder_name {
                        "ACTIVE"
                    } else if theme.is_installed {
                        "INSTALLED"
                    } else {
                        ""
                    };
                    let color = if theme.source == "PlayFusion" {
                        Color::new(0.45, 0.03, 0.62, 0.92)
                    } else if theme.source == "Kazeta+" {
                        Color::new(0.02, 0.28, 0.48, 0.92)
                    } else {
                        Color::new(0.20, 0.20, 0.28, 0.92)
                    };
                    (
                        theme.name.clone(),
                        format!("{} - {}", theme.source, theme.author),
                        installed_flag.to_string(),
                        color,
                    )
                } else {
                    let tool_index = i - state.themes.len();
                    let name = if tool_index == 0 {
                        "IMPORT FROM USB / SD"
                    } else if tool_index == 1 {
                        "RESTORE DEFAULT"
                    } else if tool_index == 2 {
                        "CONVERT OGG TO WAV"
                    } else if tool_index == 3 {
                        "CONVERT WAV TO OGG"
                    } else {
                        "DELETE THEME BGM"
                    };
                    (
                        name.to_string(),
                        "THEME TOOL".to_string(),
                        String::new(),
                        Color::new(0.34, 0.08, 0.38, 0.92),
                    )
                };

                draw_rectangle(card_x, card_y, card_w, card_h, tile_color);
                let content_x = card_x + 3.0 * scale_factor;
                let content_y = card_y + 3.0 * scale_factor;
                let content_w = card_w - 6.0 * scale_factor;
                let content_h = card_h - 6.0 * scale_factor;
                let preview = if i < state.themes.len() {
                    state
                        .preview_textures
                        .get(&state.themes[i].folder_name)
                } else {
                    None
                };
                if let Some(texture) = preview {
                    let texture_aspect = texture.width() / texture.height().max(1.0);
                    let card_aspect = content_w / content_h.max(1.0);
                    let source = if texture_aspect > card_aspect {
                        let source_w = texture.height() * card_aspect;
                        Rect::new(
                            (texture.width() - source_w) * 0.5,
                            0.0,
                            source_w,
                            texture.height(),
                        )
                    } else {
                        let source_h = texture.width() / card_aspect;
                        Rect::new(
                            0.0,
                            (texture.height() - source_h) * 0.5,
                            texture.width(),
                            source_h,
                        )
                    };
                    draw_texture_ex(
                        texture,
                        content_x,
                        content_y,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(content_w, content_h)),
                            source: Some(source),
                            ..Default::default()
                        },
                    );
                    draw_rectangle(
                        content_x,
                        content_y,
                        content_w,
                        content_h,
                        Color::new(0.0, 0.0, 0.0, 0.44),
                    );
                } else {
                    draw_rectangle(
                        content_x,
                        content_y,
                        content_w,
                        content_h,
                        Color::new(0.01, 0.0, 0.06, 0.72),
                    );
                }
                if is_selected {
                    draw_configured_cursor_frame(
                        config,
                        animation_state,
                        card_x - 2.0 * scale_factor,
                        card_y - 2.0 * scale_factor,
                        card_w + 4.0 * scale_factor,
                        card_h + 4.0 * scale_factor,
                        3.0 * scale_factor,
                    );
                }

                let badge = if i < state.themes.len() { "THEME" } else { "TOOLS" };
                let badge_dims = measure_text(badge, Some(font), source_font_size, 1.0);
                text_with_config_color(
                    font_cache,
                    config,
                    badge,
                    card_x + (card_w - badge_dims.width) * 0.5,
                    card_y + card_h * 0.26,
                    source_font_size,
                );
                let name_lines = wrap_text(
                    display_text.trim(),
                    font.clone(),
                    card_font_size,
                    card_w - 24.0 * scale_factor,
                );
                for (line_index, line) in name_lines.iter().take(2).enumerate() {
                    let dims = measure_text(line, Some(font), card_font_size, 1.0);
                    text_with_config_color(
                        font_cache,
                        config,
                        line,
                        card_x + (card_w - dims.width) * 0.5,
                        card_y + card_h * 0.55 + line_index as f32 * card_font_size as f32 * 1.1,
                        card_font_size,
                    );
                }
                let source_dims = measure_text(&source_text, Some(font), source_font_size, 1.0);
                text_with_config_color(
                    font_cache,
                    config,
                    &source_text,
                    card_x + ((card_w - source_dims.width) * 0.5).max(5.0 * scale_factor),
                    card_y + card_h - 10.0 * scale_factor,
                    source_font_size,
                );
                if !installed_text.is_empty() {
                    text_with_config_color(
                        font_cache,
                        config,
                        &installed_text,
                        card_x + 8.0 * scale_factor,
                        card_y + 14.0 * scale_factor,
                        source_font_size,
                    );
                }
            }

            // Draw description panel
            let separator_y = container_y + container_h * 0.62;
            draw_line(
                container_x,
                separator_y,
                container_x + container_w,
                separator_y,
                2.0,
                Color::new(1.0, 1.0, 1.0, 0.2),
            );

            let description_text = if state.selected_index < state.themes.len() {
                let selected_theme = &state.themes[state.selected_index];
                let description_without_author = selected_theme
                    .description
                    .lines()
                    .filter(|line| !line.trim().to_lowercase().starts_with("author:"))
                    .collect::<Vec<&str>>()
                    .join("\n");
                let img_tag_regex = Regex::new(r"<img[^>]*>").unwrap();
                img_tag_regex
                    .replace_all(&description_without_author, "")
                    .to_string()
            } else {
                let tool_index = state.selected_index - state.themes.len();
                if tool_index == 0 {
                    "Imports compatible PlayFusion or Kazeta+ theme folders and safe ZIP files from USB/SD.\n\nOptional system-folders artwork is preserved; older themes continue to use built-in folder covers.".to_string()
                } else if tool_index == 1 {
                    "Restores the built-in PlayFusion logo, ProjectM Fusion background, font, colors, cursor and sounds.\n\nResolution, audio, profiles, network and other system settings are preserved.".to_string()
                } else if tool_index == 2 {
                    "Converts space-saving .ogg files into faster-loading .wav files.\n\nThis uses more disk space.".to_string()
                } else if tool_index == 3 {
                    "Converts large .wav files into space-saving .ogg files.\n\nThis may increase theme loading times.".to_string()
                } else {
                    // New description
                    "Deletes all .wav and .ogg BGM files from all theme and bgm folders.\n\nThis will NOT delete sound effects (SFX) packs.".to_string()
                }
            };

            // -- NEW -- Define a smaller font size and line height for the description
            let description_font_size = (font_size as f32 * 0.8) as u16;
            let description_line_height = description_font_size as f32 * 1.5;

            let wrap_width = container_w - 60.0 * scale_factor;
            // -- CHANGED -- Use the new, smaller font size for text wrapping
            let wrapped_lines = wrap_text(
                description_text.trim(),
                font.clone(),
                description_font_size,
                wrap_width,
            );
            for (i, line) in wrapped_lines.iter().enumerate() {
                // -- CHANGED -- Use the new line height and font size for drawing
                let y_pos =
                    separator_y + 40.0 * scale_factor + (i as f32 * description_line_height);
                text_with_config_color(
                    font_cache,
                    config,
                    line,
                    text_x,
                    y_pos,
                    description_font_size,
                );
            }

            // Draw pagination controls and hint text
            let hint_y = container_y + container_h - 20.0;
            let hint_text = "[SOUTH] Apply / Download    [WEST] Delete    [EAST] Back";
            let hint_dims =
                measure_text(hint_text, Some(font), (font_size as f32 * 0.8) as u16, 1.0);
            text_with_config_color(
                font_cache,
                config,
                hint_text,
                screen_width() / 2.0 - hint_dims.width / 2.0,
                hint_y,
                (font_size as f32 * 0.8) as u16,
            );

            if total_pages > 1 {
                let page_text = format!("Page {} / {}", state.current_page + 1, total_pages);
                let page_dims =
                    measure_text(&page_text, Some(font), (font_size as f32 * 0.8) as u16, 1.0);
                text_with_config_color(
                    font_cache,
                    config,
                    &page_text,
                    screen_width() / 2.0 - page_dims.width / 2.0,
                    text_y_start - (line_height * 0.8),
                    (font_size as f32 * 0.8) as u16,
                );
            }
        }
        DownloaderState::ConfirmDelete {
            theme_display_name,
            selection,
            ..
        } => {
            let dialog_w = 400.0 * scale_factor;
            let dialog_h = 150.0 * scale_factor;
            let dialog_x = screen_width() / 2.0 - dialog_w / 2.0;
            let dialog_y = screen_height() / 2.0 - dialog_h / 2.0;
            draw_rectangle(
                dialog_x,
                dialog_y,
                dialog_w,
                dialog_h,
                Color::new(0.1, 0.1, 0.1, 0.9),
            );
            draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 3.0, WHITE);

            let question = format!("Delete '{}'?", theme_display_name);
            let question_dims = measure_text(&question, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                &question,
                screen_width() / 2.0 - question_dims.width / 2.0,
                dialog_y + 40.0 * scale_factor,
                font_size,
            );

            let yes_text = "YES";
            let no_text = "NO";
            let yes_dims = measure_text(yes_text, Some(font), font_size, 1.0);
            let no_dims = measure_text(no_text, Some(font), font_size, 1.0);
            let yes_x = screen_width() / 2.0 - yes_dims.width - 20.0 * scale_factor;
            let no_x = screen_width() / 2.0 + 20.0 * scale_factor;
            let options_y = dialog_y + dialog_h - 50.0 * scale_factor;
            text_with_config_color(font_cache, config, yes_text, yes_x, options_y, font_size);
            text_with_config_color(font_cache, config, no_text, no_x, options_y, font_size);

            let cursor_x = if *selection == 0 { yes_x } else { no_x };
            let cursor_w = if *selection == 0 {
                yes_dims.width
            } else {
                no_dims.width
            };
            let cursor_color = animation_state.get_cursor_color(config);
            draw_rectangle_lines(
                cursor_x - 5.0,
                options_y - font_size as f32,
                cursor_w + 10.0,
                line_height,
                3.0,
                cursor_color,
            );
        }
        DownloaderState::ConfirmRedownload { theme, selection } => {
            let dialog_w = 500.0 * scale_factor; // Made dialog wider for new text
            let dialog_h = 170.0 * scale_factor; // Made dialog taller
            let dialog_x = screen_width() / 2.0 - dialog_w / 2.0;
            let dialog_y = screen_height() / 2.0 - dialog_h / 2.0;
            draw_rectangle(
                dialog_x,
                dialog_y,
                dialog_w,
                dialog_h,
                Color::new(0.1, 0.1, 0.1, 0.9),
            );
            draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 3.0, WHITE);

            // Line 1
            let question = format!("'{}' is already installed.", theme.name);
            let question_dims = measure_text(&question, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                &question,
                screen_width() / 2.0 - question_dims.width / 2.0,
                dialog_y + 40.0 * scale_factor,
                font_size,
            );

            // Line 2
            let question2 = "Re-download and overwrite?";
            let question_dims2 = measure_text(question2, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                question2,
                screen_width() / 2.0 - question_dims2.width / 2.0,
                dialog_y + 40.0 * scale_factor + line_height,
                font_size,
            );

            let yes_text = "YES";
            let no_text = "NO";
            let yes_dims = measure_text(yes_text, Some(font), font_size, 1.0);
            let no_dims = measure_text(no_text, Some(font), font_size, 1.0);
            let yes_x = screen_width() / 2.0 - yes_dims.width - 20.0 * scale_factor;
            let no_x = screen_width() / 2.0 + 20.0 * scale_factor;
            let options_y = dialog_y + dialog_h - 50.0 * scale_factor;
            text_with_config_color(font_cache, config, yes_text, yes_x, options_y, font_size);
            text_with_config_color(font_cache, config, no_text, no_x, options_y, font_size);

            let cursor_x = if *selection == 0 { yes_x } else { no_x };
            let cursor_w = if *selection == 0 {
                yes_dims.width
            } else {
                no_dims.width
            };
            let cursor_color = animation_state.get_cursor_color(config);
            draw_rectangle_lines(
                cursor_x - 5.0,
                options_y - font_size as f32,
                cursor_w + 10.0,
                line_height,
                3.0,
                cursor_color,
            );
        }
        DownloaderState::ConfirmConvertToWav { selection } => {
            // -- FIX -- Pass `font` directly without cloning
            draw_conversion_dialog(
                font_cache,
                config,
                font,
                font_size,
                line_height,
                scale_factor,
                animation_state,
                "Convert Audio to .WAV?",
                &[
                    "This will convert all .ogg BGM files to .wav format.",
                    "Benefits: Faster theme loading times.",
                    "Drawbacks: Uses significantly more disk space.",
                ],
                *selection,
            );
        }
        DownloaderState::ConfirmConvertToOgg { selection } => {
            // -- FIX -- Pass `font` directly without cloning
            draw_conversion_dialog(
                font_cache,
                config,
                font,
                font_size,
                line_height,
                scale_factor,
                animation_state,
                "Convert Audio to .OGG?",
                &[
                    "This will convert all .wav BGM files to .ogg format.",
                    "Benefits: Frees up a lot of disk space.",
                    "Drawbacks: Slower theme loading times.",
                ],
                *selection,
            );
        }
        DownloaderState::ConfirmDeleteAllBGM { selection } => {
            draw_conversion_dialog(
                font_cache,
                config,
                font,
                font_size,
                line_height,
                scale_factor,
                animation_state,
                "Delete All BGM Tracks?",
                &[
                    "This will delete all .wav and .ogg files from:",
                    "  - /themes/...",
                    "  - /bgm/...",
                    "\nSound effect packs (SFX) will NOT be touched.",
                    "This cannot be undone.",
                ],
                *selection,
            );
        }
        DownloaderState::Converting(msg) => {
            let text_dims = measure_text(msg, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                msg,
                screen_width() / 2.0 - text_dims.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );
        }
        DownloaderState::Downloading(name) => {
            let text = format!("Downloading {}...", name);
            let text_dims = measure_text(&text, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                &text,
                screen_width() / 2.0 - text_dims.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );
        }
        DownloaderState::Success(msg) | DownloaderState::Error(msg) => {
            let text_dims = measure_text(msg, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                msg,
                screen_width() / 2.0 - text_dims.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );

            let continue_text = "Press [SOUTH] to continue";
            let continue_dims = measure_text(continue_text, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                continue_text,
                screen_width() / 2.0 - continue_dims.width / 2.0,
                screen_height() / 2.0 + line_height * 2.0,
                font_size,
            );
        }
    }
}

// -- NEW -- Helper function to draw the dialog box for conversions
fn draw_conversion_dialog(
    font_cache: &HashMap<String, Font>,
    config: &Config,
    font: &Font,
    font_size: u16,
    line_height: f32,
    scale_factor: f32,
    animation_state: &mut crate::AnimationState,
    title: &str,
    body_lines: &[&str],
    selection: usize,
) {
    let dialog_w = 600.0 * scale_factor;
    let dialog_h = 300.0 * scale_factor;
    let dialog_x = screen_width() / 2.0 - dialog_w / 2.0;
    let dialog_y = screen_height() / 2.0 - dialog_h / 2.0;
    draw_rectangle(
        dialog_x,
        dialog_y,
        dialog_w,
        dialog_h,
        Color::new(0.1, 0.1, 0.1, 0.9),
    );
    draw_rectangle_lines(dialog_x, dialog_y, dialog_w, dialog_h, 3.0, WHITE);

    let title_dims = measure_text(title, Some(font), font_size, 1.0);
    text_with_config_color(
        font_cache,
        config,
        title,
        screen_width() / 2.0 - title_dims.width / 2.0,
        dialog_y + 40.0 * scale_factor,
        font_size,
    );

    for (i, line) in body_lines.iter().enumerate() {
        text_with_config_color(
            font_cache,
            config,
            line,
            dialog_x + 20.0 * scale_factor,
            dialog_y + 80.0 * scale_factor + (i as f32 * line_height),
            font_size,
        );
    }

    let yes_text = "YES";
    let no_text = "NO";
    let yes_dims = measure_text(yes_text, Some(font), font_size, 1.0);
    let no_dims = measure_text(no_text, Some(font), font_size, 1.0);
    let yes_x = screen_width() / 2.0 - yes_dims.width - 40.0 * scale_factor;
    let no_x = screen_width() / 2.0 + 40.0 * scale_factor;
    let options_y = dialog_y + dialog_h - 50.0 * scale_factor;
    text_with_config_color(font_cache, config, yes_text, yes_x, options_y, font_size);
    text_with_config_color(font_cache, config, no_text, no_x, options_y, font_size);

    let cursor_x = if selection == 0 { yes_x } else { no_x };
    let cursor_w = if selection == 0 {
        yes_dims.width
    } else {
        no_dims.width
    };
    let cursor_color = animation_state.get_cursor_color(config);
    draw_rectangle_lines(
        cursor_x - 10.0,
        options_y - font_size as f32,
        cursor_w + 20.0,
        line_height,
        3.0,
        cursor_color,
    );
}

// --- Background Thread Functions ---

fn import_themes_from_usb(tx: Sender<DownloaderMessage>) {
    thread::spawn(move || {
        let result = match Command::new("/usr/bin/playfusion-theme-import").output() {
            Ok(output) if output.status.success() => {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            }
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let message = if !stdout.trim().is_empty() {
                    stdout
                } else {
                    stderr
                };
                Err(message.trim().to_string())
            }
            Err(error) => Err(format!("Unable to start theme importer: {error}")),
        };
        let _ = tx.send(DownloaderMessage::ConversionResult(result));
    });
}

fn fetch_theme_list(tx: Sender<DownloaderMessage>) {
    thread::spawn(move || {
        // Build missing gallery thumbnails off the UI thread. Video themes
        // are decoded once after installation and then use the cached PNG.
        for folder_name in get_installed_theme_folders() {
            let _ = ensure_video_theme_preview(&folder_name);
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent("PlayFusion-Theme-Manager")
            .build()
            .unwrap();
        let repositories = [
            (
                "PlayFusion",
                "https://api.github.com/repos/pixelgriffstudios/PlayFusion-Themes/releases",
            ),
            (
                "Kazeta+",
                "https://api.github.com/repos/the-outcaster/kazeta-plus-themes/releases",
            ),
        ];
        let mut themes: Vec<RemoteTheme> = Vec::new();
        let mut successful_repositories = 0usize;
        for (source, url) in repositories {
            let Ok(response) = client.get(url).send() else {
                eprintln!("[Theme Manager] Unable to fetch {source} theme catalog");
                continue;
            };
            let Ok(releases) = response.json::<Vec<GithubRelease>>() else {
                eprintln!("[Theme Manager] Unable to parse {source} theme catalog");
                continue;
            };
            successful_repositories += 1;
            for release in releases {
                for asset in release
                    .assets
                    .iter()
                    .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
                {
                    let author = release
                        .body
                        .lines()
                        .find(|line| line.to_lowercase().starts_with("author:"))
                        .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string())
                        .unwrap_or_else(|| {
                            if source == "PlayFusion" {
                                "PixelGriff Studios".to_string()
                            } else {
                                "Kazeta+ community".to_string()
                            }
                        });
                    let folder_name = asset
                        .name
                        .strip_suffix(".zip")
                        .unwrap_or(&asset.name)
                        .to_string();
                    let name = if release.assets.len() == 1 {
                        release.name.clone()
                    } else {
                        folder_name
                            .split(['_', '-'])
                            .filter(|part| !part.is_empty())
                            .map(|part| {
                                let mut chars = part.chars();
                                chars
                                    .next()
                                    .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                                    .unwrap_or_default()
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    let remote_theme = RemoteTheme {
                        name,
                        folder_name: folder_name.clone(),
                        author,
                        description: release.body.clone(),
                        download_url: asset.browser_download_url.clone(),
                        source: source.to_string(),
                        preview_bytes: find_remote_preview_asset(&release.assets, &asset.name)
                            .and_then(|preview_asset| {
                                download_theme_preview(&client, preview_asset)
                            }),
                        is_installed: false,
                    };
                    if let Some(existing) = themes
                        .iter_mut()
                        .find(|theme| theme.folder_name == folder_name)
                    {
                        if source == "PlayFusion" {
                            *existing = remote_theme;
                        }
                    } else {
                        themes.push(remote_theme);
                    }
                }
            }
        }
        let result = if successful_repositories == 0 {
            Err("Failed to fetch theme catalogs from GitHub.".to_string())
        } else {
            Ok(themes)
        };
        tx.send(DownloaderMessage::ThemeList(result)).unwrap();
    });
}

fn download_and_extract_theme(theme: RemoteTheme, tx: Sender<DownloaderMessage>) {
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let themes_dir = get_user_data_dir()
                .ok_or("Could not find user data directory.")?
                .join("themes");
            let response_bytes = reqwest::blocking::get(&theme.download_url)
                .map_err(|e| format!("Download failed: {}", e))?
                .bytes()
                .map_err(|e| format!("Failed to read download: {}", e))?;
            let reader = io::Cursor::new(response_bytes);
            let mut archive =
                zip::ZipArchive::new(reader).map_err(|e| format!("Invalid zip file: {}", e))?;
            archive
                .extract(&themes_dir)
                .map_err(|e| format!("Failed to extract theme: {}", e))?;
            for folder_name in get_installed_theme_folders() {
                let _ = ensure_video_theme_preview(&folder_name);
            }
            Ok(theme.name)
        })();
        tx.send(DownloaderMessage::InstallResult(result)).unwrap();
    });
}

fn convert_files_to_wav(tx: Sender<DownloaderMessage>) {
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let files_to_convert = find_files_by_extension(".ogg")?;
            if files_to_convert.is_empty() {
                return Ok("No .ogg files found to convert.".to_string());
            }

            for path in &files_to_convert {
                let mut wav_path = path.clone();
                wav_path.set_extension("wav");

                let status = Command::new("ffmpeg")
                    .arg("-i")
                    .arg(path)
                    .arg("-y") // Overwrite output file if it exists
                    .arg(&wav_path)
                    .status()
                    .map_err(|e| format!("Is ffmpeg installed? Command failed: {}", e))?;

                if !status.success() {
                    return Err(format!("ffmpeg failed for {}", path.display()));
                }

                update_theme_toml(path, ".wav")?;
                fs::remove_file(path).map_err(|e| format!("Failed to delete old file: {}", e))?;
            }
            Ok(format!(
                "Successfully converted {} file(s) to .wav!",
                files_to_convert.len()
            ))
        })();
        tx.send(DownloaderMessage::ConversionResult(result))
            .unwrap();
    });
}

// -- REVERTED -- Back to using the simpler ffmpeg command-line tool.
fn convert_files_to_ogg(tx: Sender<DownloaderMessage>) {
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let files_to_convert = find_files_by_extension(".wav")?;
            if files_to_convert.is_empty() {
                return Ok("No .wav files found to convert.".to_string());
            }

            for path in &files_to_convert {
                let mut ogg_path = path.clone();
                ogg_path.set_extension("ogg");

                let status = Command::new("ffmpeg")
                    .arg("-i")
                    .arg(path)
                    .arg("-y")
                    .arg("-acodec")
                    .arg("libvorbis") // Specify ogg codec
                    .arg(&ogg_path)
                    .status()
                    .map_err(|e| format!("Is ffmpeg installed? Command failed: {}", e))?;

                if !status.success() {
                    return Err(format!("ffmpeg failed for {}", path.display()));
                }

                update_theme_toml(path, ".ogg")?;
                fs::remove_file(path).map_err(|e| format!("Failed to delete old file: {}", e))?;
            }
            Ok(format!(
                "Successfully converted {} file(s) to .ogg!",
                files_to_convert.len()
            ))
        })();
        tx.send(DownloaderMessage::ConversionResult(result))
            .unwrap();
    });
}

// -- CHANGED -- Now ignores files in directories containing `_sfx`.
fn find_files_by_extension(ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut found_files = Vec::new();
    let base_dir = get_user_data_dir().ok_or("Could not find user data directory.")?;
    let dirs_to_search = [base_dir.join("bgm"), base_dir.join("themes")];

    for dir in dirs_to_search.iter() {
        if !dir.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(dir) {
            let entry = entry.map_err(|e| format!("Error walking directory: {}", e))?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some(&ext[1..]) {
                // Check if any part of the path indicates it's an SFX pack.
                let is_sfx_file = path
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().contains("_sfx"));

                if !is_sfx_file {
                    found_files.push(path.to_path_buf());
                }
            }
        }
    }
    Ok(found_files)
}

// -- NEW -- Helper to update the theme.toml file after a conversion
fn update_theme_toml(audio_path: &Path, new_ext: &str) -> Result<(), String> {
    // Find a theme.toml in the parent directory of the audio file
    if let Some(parent_dir) = audio_path.parent() {
        let toml_path = parent_dir.join("theme.toml");
        if toml_path.exists() {
            let content = fs::read_to_string(&toml_path)
                .map_err(|e| format!("Failed to read theme.toml: {}", e))?;
            let mut theme_data: ThemeToml = toml::from_str(&content)
                .map_err(|e| format!("Failed to parse theme.toml: {}", e))?;

            if let Some(bgm_track) = theme_data.bgm_track.as_mut() {
                let mut new_track_path = PathBuf::from(bgm_track.as_str());
                new_track_path.set_extension(&new_ext[1..]);
                *bgm_track = new_track_path.to_string_lossy().to_string();
            }

            let new_content = toml::to_string(&theme_data)
                .map_err(|e| format!("Failed to serialize theme.toml: {}", e))?;
            fs::write(toml_path, new_content)
                .map_err(|e| format!("Failed to write theme.toml: {}", e))?;
        }
    }
    Ok(())
}

fn local_theme_entry(folder_name: &str) -> RemoteTheme {
    let metadata = get_user_data_dir()
        .map(|dir| dir.join("themes").join(folder_name).join("theme.toml"))
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|content| toml::from_str::<ThemeToml>(&content).ok())
        .unwrap_or_default();
    let display_name = folder_name
        .split(|character| character == '_' || character == '-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    RemoteTheme {
        name: if display_name.is_empty() {
            folder_name.to_string()
        } else {
            display_name
        },
        folder_name: folder_name.to_string(),
        author: metadata.author.unwrap_or_else(|| "Local theme".to_string()),
        description: metadata
            .description
            .unwrap_or_else(|| "Imported from USB / SD storage.".to_string()),
        download_url: String::new(),
        source: "Local".to_string(),
        preview_bytes: local_theme_preview(folder_name),
        is_installed: true,
    }
}

fn local_theme_preview(folder_name: &str) -> Option<Vec<u8>> {
    let theme_dir = get_user_data_dir()?.join("themes").join(folder_name);
    for name in ["preview.png", "preview.jpg", "preview.jpeg", "preview.webp"] {
        if let Some(bytes) = read_theme_preview_file(&theme_dir.join(name)) {
            return Some(bytes);
        }
    }

    // Older themes did not define preview.png. A static theme background is a
    // safe fallback; videos are intentionally skipped so gallery browsing
    // never starts a decoder for every card.
    let configured_background = fs::read_to_string(theme_dir.join("theme.toml"))
        .ok()
        .and_then(|content| toml::from_str::<ThemeToml>(&content).ok())
        .and_then(|metadata| metadata.background_selection);
    if let Some(background) = configured_background {
        let extension = Path::new(&background)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            if let Some(bytes) = read_theme_preview_file(&theme_dir.join(background)) {
                return Some(bytes);
            }
        }
    }

    for name in [
        "background.png",
        "background.jpg",
        "background.jpeg",
        "background.webp",
    ] {
        if let Some(bytes) = read_theme_preview_file(&theme_dir.join(name)) {
            return Some(bytes);
        }
    }
    None
}

fn read_theme_preview_file(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_THEME_PREVIEW_BYTES as u64 {
        return None;
    }
    fs::read(path).ok()
}

fn ensure_video_theme_preview(folder_name: &str) -> Option<Vec<u8>> {
    if let Some(preview) = local_theme_preview(folder_name) {
        return Some(preview);
    }

    let theme_dir = get_user_data_dir()?.join("themes").join(folder_name);
    let theme_dir_canonical = theme_dir.canonicalize().ok()?;
    let metadata = fs::read_to_string(theme_dir.join("theme.toml"))
        .ok()
        .and_then(|content| toml::from_str::<ThemeToml>(&content).ok())
        .unwrap_or_default();
    let mut candidates = Vec::new();
    if let Some(background) = metadata.background_selection {
        candidates.push(background);
    }
    candidates.push("background.mp4".to_string());
    if let Some(animation) = metadata.boot_animation {
        candidates.push(animation);
    }
    candidates.push("boot_animation.mp4".to_string());

    let video_path = candidates.into_iter().find_map(|candidate| {
        let relative = Path::new(&candidate);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return None;
        }
        let extension = relative
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "mp4" | "mkv" | "webm" | "mov") {
            return None;
        }
        let path = theme_dir.join(relative);
        let canonical = path.canonicalize().ok()?;
        if canonical.starts_with(&theme_dir_canonical) && canonical.is_file() {
            Some(canonical)
        } else {
            None
        }
    })?;

    let preview_path = theme_dir.join("preview.png");
    let temporary_path = theme_dir.join(".preview-generating.png");
    let render_frame = |timestamp: &str| {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-ss", timestamp, "-i"])
            .arg(&video_path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=black",
                "-y",
            ])
            .arg(&temporary_path)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    };
    let rendered = render_frame("1.0") || render_frame("0.0");
    if !rendered {
        let _ = fs::remove_file(&temporary_path);
        return None;
    }
    if fs::rename(&temporary_path, &preview_path).is_err() {
        let _ = fs::remove_file(&temporary_path);
        return None;
    }
    read_theme_preview_file(&preview_path)
}

fn find_remote_preview_asset<'a>(
    assets: &'a [GithubReleaseAsset],
    archive_name: &str,
) -> Option<&'a GithubReleaseAsset> {
    let archive_stem = archive_name
        .strip_suffix(".zip")
        .unwrap_or(archive_name)
        .to_ascii_lowercase();
    let supported = |name: &str| {
        let lower = name.to_ascii_lowercase();
        lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".webp")
    };
    let exact_names = [
        format!("{archive_stem}-preview.png"),
        format!("{archive_stem}-preview.jpg"),
        format!("{archive_stem}-preview.jpeg"),
        format!("{archive_stem}-preview.webp"),
        format!("{archive_stem}.png"),
        format!("{archive_stem}.jpg"),
        format!("{archive_stem}.jpeg"),
        format!("{archive_stem}.webp"),
    ];
    assets.iter().find(|asset| {
        let lower = asset.name.to_ascii_lowercase();
        supported(&lower) && exact_names.iter().any(|candidate| candidate == &lower)
    })
}

fn download_theme_preview(
    client: &reqwest::blocking::Client,
    asset: &GithubReleaseAsset,
) -> Option<Vec<u8>> {
    let response = client.get(&asset.browser_download_url).send().ok()?;
    if let Some(length) = response.content_length() {
        if length > MAX_THEME_PREVIEW_BYTES as u64 {
            return None;
        }
    }
    let bytes = response.bytes().ok()?;
    if bytes.len() > MAX_THEME_PREVIEW_BYTES {
        return None;
    }
    Some(bytes.to_vec())
}

/// Scans the user's themes directory and returns a HashSet of installed theme folder names.
fn get_installed_theme_folders() -> HashSet<String> {
    if let Some(themes_dir) = get_user_data_dir().map(|d| d.join("themes")) {
        if let Ok(entries) = fs::read_dir(themes_dir) {
            // Use flatten() to filter out any read errors on individual entries
            return entries
                .flatten()
                .filter_map(|entry| {
                    // Check if it's a directory
                    if entry.path().is_dir() {
                        // Try to convert the file/folder name to a String
                        entry.file_name().into_string().ok()
                    } else {
                        None
                    }
                })
                .collect();
        }
    }
    // Return an empty set if any step failed
    HashSet::new()
}

fn delete_all_bgm_files(tx: Sender<DownloaderMessage>) {
    thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            let wav_files = find_files_by_extension(".wav")?;
            let ogg_files = find_files_by_extension(".ogg")?;

            if wav_files.is_empty() && ogg_files.is_empty() {
                return Ok("No BGM files found to delete.".to_string());
            }

            let mut delete_count = 0;
            let mut toml_paths = HashSet::new();

            // Iterate over all files, delete them, and collect their parent toml paths
            for path in wav_files.iter().chain(ogg_files.iter()) {
                // Find the theme.toml file *before* deleting the file
                if let Some(parent) = path.parent() {
                    let toml_path = parent.join("theme.toml");
                    if toml_path.exists() {
                        toml_paths.insert(toml_path);
                    }
                }

                // Delete the file
                if fs::remove_file(path).is_ok() {
                    delete_count += 1;
                } else {
                    eprintln!("[WARN] Failed to delete file: {}", path.display());
                }
            }

            // Now, update all collected theme.toml files
            for toml_path in toml_paths {
                if let Ok(content) = fs::read_to_string(&toml_path) {
                    if let Ok(mut theme_data) = toml::from_str::<ThemeToml>(&content) {
                        // Set bgm_track to None (which serializes as it being removed or null)
                        theme_data.bgm_track = None;

                        // Reserialize and write back
                        if let Ok(new_content) = toml::to_string(&theme_data) {
                            let _ = fs::write(toml_path, new_content);
                        }
                    }
                }
            }

            Ok(format!(
                "Successfully deleted {} BGM file(s)!",
                delete_count
            ))
        })();

        // Send the result back, whether Ok or Err
        tx.send(DownloaderMessage::ConversionResult(result))
            .unwrap_or_default();
    });
}
