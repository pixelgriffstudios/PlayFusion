use crate::{
    audio::SoundEffects,
    config::Config,
    copy_session_logs_to_sd, get_current_font, measure_text, render_background, render_ui_overlay,
    save, start_log_reader, text_disabled, text_with_config_color, trigger_game_launch,
    types::{AnimationState, BackgroundState, BatteryInfo, MenuPosition},
    ui::{draw_configured_cursor_frame, text_with_color},
    InputState, Screen, ShakeTarget, StorageMediaState, UIFocus, VideoPlayer, DEV_MODE,
    FLASH_MESSAGE_DURATION, FONT_SIZE, MENU_OPTION_HEIGHT, MENU_PADDING,
};
use macroquad::prelude::*;
use rodio::{buffer::SamplesBuffer, Sink};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::Ordering,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

pub const MAIN_MENU_OPTIONS: &[&str] = &[
    "DATA",
    "PLAY",
    "INTERNAL GAMES",
    "COPY SESSION LOGS",
    "SETTINGS",
    "EXTRAS",
    "ABOUT",
    "POWER",
];

struct CartNameCache {
    checked_at: Option<Instant>,
    name: Option<String>,
}

fn detected_cart_name(play_enabled: bool) -> Option<String> {
    static CACHE: OnceLock<Mutex<CartNameCache>> = OnceLock::new();
    if !play_enabled {
        return None;
    }
    let cache = CACHE.get_or_init(|| {
        Mutex::new(CartNameCache {
            checked_at: None,
            name: None,
        })
    });
    let mut cache = cache.lock().ok()?;
    if cache
        .checked_at
        .map(|checked| checked.elapsed() < Duration::from_millis(750))
        .unwrap_or(false)
    {
        return cache.name.clone();
    }

    let name = save::find_all_game_files().ok().and_then(|(paths, _)| {
        let mut names = paths
            .iter()
            .filter_map(|path| {
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.eq_ignore_ascii_case("kzi"))
                    .unwrap_or(false)
                {
                    save::parse_kzi_file(path)
                        .ok()
                        .map(|cart| cart.name.unwrap_or(cart.id))
                } else {
                    path.file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                }
            })
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        match names.len() {
            0 => None,
            1 => names.pop(),
            count => Some(format!("MULTI-CART ({count} GAMES)")),
        }
    });
    cache.checked_at = Some(Instant::now());
    cache.name = name.clone();
    name
}

fn shortened_cart_name(name: &str, maximum_chars: usize) -> String {
    let mut characters = name.chars();
    let shortened = characters.by_ref().take(maximum_chars).collect::<String>();
    if characters.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

pub fn update(
    current_screen: &mut Screen,
    main_menu_selection: &mut usize,
    play_option_enabled: &mut bool,
    copy_logs_option_enabled: &mut bool,
    cart_connected: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    input_state: &mut InputState,
    animation_state: &mut AnimationState,
    sound_effects: &SoundEffects,
    config: &Config,
    log_messages: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    storage_state: &Arc<Mutex<StorageMediaState>>,
    fade_start_time: &mut Option<f64>,
    current_bgm: &mut Option<Sink>,
    music_cache: &HashMap<String, SamplesBuffer>,
    game_icon_queue: &mut Vec<(String, PathBuf)>,
    available_games: &mut Vec<(save::CartInfo, PathBuf)>,
    game_selection: &mut usize,
    flash_message: &mut Option<(String, f32)>,
    game_process: &mut Option<std::process::Child>,
    power_menu_selection: &mut usize,
) {
    // Update play option enabled status based on cart connection
    *play_option_enabled = cart_connected.load(Ordering::Relaxed);

    // Update copy logs option enabled status based on cart connection
    *copy_logs_option_enabled = cart_connected.load(Ordering::Relaxed);

    // Handle main menu navigation
    if input_state.up {
        if *main_menu_selection == 0 {
            *main_menu_selection = MAIN_MENU_OPTIONS.len() - 1;
        } else {
            *main_menu_selection = (*main_menu_selection - 1) % MAIN_MENU_OPTIONS.len();
        }
        animation_state.trigger_transition(&config.cursor_transition_speed);
        sound_effects.play_cursor_move(&config);
    }
    if input_state.down {
        *main_menu_selection = (*main_menu_selection + 1) % MAIN_MENU_OPTIONS.len();
        animation_state.trigger_transition(&config.cursor_transition_speed);
        sound_effects.play_cursor_move(&config);
    }
    if input_state.select {
        match *main_menu_selection {
            0 => {
                // SAVE DATA
                // Trigger a refresh the next time the data screen is entered.
                if let Ok(mut state) = storage_state.lock() {
                    state.needs_memory_refresh = true;
                }

                *current_screen = Screen::SaveData;
                input_state.ui_focus = UIFocus::Grid;
                sound_effects.play_select(&config);
            }
            1 => {
                // PLAY option
                if *play_option_enabled {
                    sound_effects.play_select(&config);
                    log_messages.lock().unwrap().clear();

                    match save::find_all_game_files() {
                        Ok((game_paths, mut debug_log)) => {
                            log_messages.lock().unwrap().append(&mut debug_log);

                            let mut games: Vec<(save::CartInfo, PathBuf)> = Vec::new();
                            let parse_errors: Vec<String> = Vec::new();

                            for path in &game_paths {
                                // Handle .kzp vs .kzi parsing
                                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                    if ext == "kzi" {
                                        // Standard parsing for KZI
                                        if let Ok(info) = save::parse_kzi_file(path) {
                                            games.push((info, path.clone()));
                                        }
                                    } else if ext == "kzp" {
                                        // Logic for KZP (Compressed Package)
                                        // Since we can't easily read inside the package without mounting,
                                        // we construct a CartInfo based on the filename.
                                        let filename =
                                            path.file_stem().unwrap().to_string_lossy().to_string();

                                        // We assume the ID is the filename
                                        let info = save::CartInfo {
                                            name: Some(filename.clone()), // Use filename as Game Name
                                            id: filename,
                                            exec: String::from("internal"), // Placeholder
                                            icon: String::from("icon.png"), // Placeholder
                                            runtime: Some(String::from("erofs")),
                                            // Add other fields as necessary for your struct
                                            ..Default::default()
                                        };
                                        games.push((info, path.clone()));
                                    }
                                }
                            }

                            match games.len() {
                                0 => {
                                    // Case: Found files, but none were valid
                                    let mut logs = log_messages.lock().unwrap();
                                    logs.push(format!("[Info] Found {} potential game file(s), but none could be parsed.", game_paths.len()));
                                    logs.push("--- ERRORS ---".to_string());
                                    logs.extend(parse_errors);
                                    *current_screen = Screen::Debug;
                                }
                                1 => {
                                    // Case: Exactly one game found, go to Debug screen and launch
                                    let (cart_info, kzi_path) = games.remove(0);
                                    sound_effects.play_select(&config);

                                    if DEV_MODE {
                                        {
                                            // Scoped lock to add messages
                                            let mut logs = log_messages.lock().unwrap();
                                            logs.push("--- CARTRIDGE FOUND ---".to_string());
                                            logs.push(format!(
                                                "Name: {}",
                                                cart_info.name.as_deref().unwrap_or("N/A")
                                            ));
                                            logs.push(format!("ID: {}", cart_info.id));
                                            logs.push(format!("Exec: {}", cart_info.exec));
                                            logs.push(format!(
                                                "Runtime: {}",
                                                cart_info.runtime.as_deref().unwrap_or("None")
                                            ));
                                            logs.push(format!("KZI Path: {}", kzi_path.display()));
                                        }
                                        println!("[Debug] Single Cartridge Found! Preparing to launch...");
                                        println!(
                                            "[Debug]   Name: {}",
                                            cart_info.name.as_deref().unwrap_or("N/A")
                                        );
                                        println!("[Debug]   ID: {}", cart_info.id);
                                        println!("[Debug]   Exec: {}", cart_info.exec);
                                        println!(
                                            "[Debug]   Runtime: {}",
                                            cart_info.runtime.as_deref().unwrap_or("None")
                                        );
                                        println!("[Debug]   KZI Path: {}", kzi_path.display());

                                        match save::launch_game(&cart_info, &kzi_path) {
                                            Ok(mut child) => {
                                                log_messages
                                                    .lock()
                                                    .unwrap()
                                                    .push("\n--- LAUNCHING GAME ---".to_string());
                                                start_log_reader(&mut child, log_messages.clone());
                                                *game_process = Some(child);
                                            }
                                            Err(e) => {
                                                log_messages.lock().unwrap().push(format!(
                                                    "\n--- LAUNCH FAILED ---\nError: {}",
                                                    e
                                                ));
                                            }
                                        }
                                        *current_screen = Screen::Debug;
                                    } else {
                                        // --- PRODUCTION MODE: Fade out and launch ---
                                        (*current_screen, *fade_start_time) = trigger_game_launch(
                                            &cart_info,
                                            &kzi_path,
                                            current_bgm,
                                            &music_cache,
                                        );
                                    }
                                }
                                _ => {
                                    // multiple games found
                                    println!(
                                        "[Debug] Found {} games. Switching to selection screen.",
                                        games.len()
                                    );

                                    game_icon_queue.clear();
                                    for (cart_info, game_path) in &games {
                                        // Intelligent Icon Pathing
                                        let is_package =
                                            game_path.extension().map_or(false, |e| e == "kzp");

                                        let icon_path = if is_package {
                                            // For .kzp, the icon is inside the image (inaccessible).
                                            // 1. Try to find a "sidecar" icon (e.g. game.png next to game.kzp)
                                            let sidecar_png = game_path.with_extension("png");
                                            let sidecar_jpg = game_path.with_extension("jpg");

                                            if sidecar_png.exists() {
                                                sidecar_png
                                            } else if sidecar_jpg.exists() {
                                                sidecar_jpg
                                            } else {
                                                // Instead of a file path, we use a "Magic String" that main.rs will recognize.
                                                PathBuf::from("::KZP_PLACEHOLDER::")
                                            }
                                        } else {
                                            // Standard .kzi behavior
                                            game_path.parent().unwrap().join(&cart_info.icon)
                                        };

                                        game_icon_queue.push((cart_info.id.clone(), icon_path));
                                    }

                                    *available_games = games;
                                    *game_selection = 0;
                                    *current_screen = Screen::GameSelection;
                                }
                            }
                        }
                        Err(e) => {
                            // Handle the error case
                            let error_msg = format!("[Error] Error scanning for cartridges: {}", e);
                            println!("[Error] {}", &error_msg);
                            log_messages.lock().unwrap().push(error_msg);
                            *current_screen = Screen::Debug;
                        }
                    }
                } else {
                    sound_effects.play_reject(&config);
                    animation_state.trigger_play_option_shake();
                }
            }
            2 => {
                // INTERNAL GAMES
                *current_screen = Screen::InternalGames;
                sound_effects.play_select(&config);
            }
            3 => {
                // SESSION LOG COPY
                if *copy_logs_option_enabled {
                    sound_effects.play_select(&config);

                    // Call our new function and handle the result
                    match copy_session_logs_to_sd() {
                        Ok(path) => {
                            *flash_message =
                                Some((format!("SUCCESS: {}", path), FLASH_MESSAGE_DURATION));
                        }
                        Err(e) => {
                            *flash_message =
                                Some((format!("ERROR: {}", e), FLASH_MESSAGE_DURATION));
                        }
                    }
                } else {
                    sound_effects.play_reject(&config);
                    animation_state.trigger_copy_log_option_shake();
                }
            }
            4 => {
                // SETTINGS
                *current_screen = Screen::GeneralSettings;
                sound_effects.play_select(&config);
            }
            5 => {
                // EXTRAS
                *current_screen = Screen::Extras;
                sound_effects.play_select(&config);
            }
            6 => {
                // ABOUT
                *current_screen = Screen::About;
                sound_effects.play_select(&config);
            }
            7 => {
                // POWER
                // Default to CANCEL so a repeated/held select press cannot
                // accidentally shut down the system.
                *power_menu_selection = 2;
                *current_screen = Screen::Power;
                sound_effects.play_select(&config);
            }
            _ => {}
        }
    }
}

pub fn draw(
    menu_options: &[&str],
    selected_option: usize,
    play_option_enabled: bool,
    copy_logs_option_enabled: bool,
    animation_state: &AnimationState,
    logo_cache: &HashMap<String, Texture2D>,
    background_cache: &HashMap<String, Texture2D>,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    background_state: &mut BackgroundState,
    video_cache: &mut HashMap<String, VideoPlayer>,
    battery_info: &Option<BatteryInfo>,
    _current_time_str: &str,
    _gcc_adapter_poll_rate: &Option<u32>,
    scale_factor: f32,
    flash_message: Option<&str>,
    ftp_endpoint: &str,
    profiles_state: &crate::ui::profiles::ProfilesState,
) {
    render_background(background_cache, video_cache, config, background_state);
    // Keep the main screen uncluttered. The clock and GCC polling diagnostic
    // remain available on the other interface screens.
    render_ui_overlay(
        logo_cache,
        font_cache,
        config,
        battery_info,
        "",
        &None,
        scale_factor,
    );

    // --- Define layout constants ---
    let font_size = (FONT_SIZE as f32 * scale_factor) as u16;
    let menu_padding = MENU_PADDING * scale_factor;
    let menu_option_height = MENU_OPTION_HEIGHT * scale_factor;
    let margin_x = 30.0 * scale_factor;
    let margin_y = 45.0 * scale_factor;

    let current_font = get_current_font(font_cache, config);
    let cart_name = detected_cart_name(play_option_enabled);

    // --- Determine menu position based on config ---
    let (start_x, start_y, is_centered) = match config.menu_position {
        MenuPosition::Center => (
            screen_width() / 2.0,
            (screen_height() * 0.3).max(margin_y),
            true,
        ),
        MenuPosition::TopLeft => (margin_x, margin_y, false),
        MenuPosition::TopRight => (screen_width() - margin_x, margin_y, false),
        MenuPosition::BottomLeft => (
            margin_x,
            screen_height() - (menu_options.len() as f32 * menu_option_height),
            false,
        ),
        MenuPosition::BottomRight => (
            screen_width() - margin_x,
            screen_height() - (menu_options.len() as f32 * menu_option_height),
            false,
        ),
    };

    // Draw menu options
    for (i, &option) in menu_options.iter().enumerate() {
        let y_pos = start_y + (i as f32 * menu_option_height);

        // --- Calculate text dimensions and horizontal position ---
        let text_dims = measure_text(option, Some(current_font), font_size, 1.0);
        let mut x_pos = if is_centered {
            start_x - (text_dims.width / 2.0)
        } else if start_x > screen_width() / 2.0 {
            start_x - text_dims.width
        } else {
            start_x
        };

        // --- Handle shake effect for disabled options ---
        if i == 1 && !play_option_enabled && i == selected_option {
            x_pos += animation_state.calculate_shake_offset(ShakeTarget::PlayOption);
        }
        if i == 3 && !copy_logs_option_enabled && i == selected_option {
            x_pos += animation_state.calculate_shake_offset(ShakeTarget::CopyLogOption);
        }

        let is_selected = i == selected_option;
        let is_disabled = match option {
            "PLAY" => !play_option_enabled,
            "COPY SESSION LOGS" => !copy_logs_option_enabled,
            _ => false,
        };

        // --- Draw selected option highlight ---
        if is_selected && config.cursor_style == "BOX" {
            let cursor_scale = animation_state.get_cursor_scale();
            let base_width = text_dims.width + (menu_padding * 2.0);
            let base_height = text_dims.height + (menu_padding * 2.0);
            let scaled_width = base_width * cursor_scale;
            let scaled_height = base_height * cursor_scale;
            let offset_x = (scaled_width - base_width) / 2.0;
            let offset_y = (scaled_height - base_height) / 2.0;
            let rect_x = x_pos - menu_padding;
            let rect_y = y_pos - text_dims.height - menu_padding;

            draw_configured_cursor_frame(
                config,
                animation_state,
                rect_x - offset_x,
                rect_y - offset_y,
                scaled_width,
                scaled_height,
                4.0 * scale_factor,
            );
        }

        if is_selected && config.cursor_style == "TEXT" {
            let mut highlight_color = animation_state.get_cursor_color(config);

            if is_disabled {
                // If the item is disabled, dim the cursor color by 50%.
                // It will look "Red-ish" (showing selection) but Dark (showing disabled).
                highlight_color.r *= 0.5;
                highlight_color.g *= 0.5;
                highlight_color.b *= 0.5;
                // Ensure alpha is solid so it doesn't look like a ghost
                highlight_color.a = 1.0;
            }

            text_with_color(
                font_cache,
                config,
                option,
                x_pos,
                y_pos,
                font_size,
                highlight_color,
            );
        } else if is_disabled {
            // Not selected, just disabled -> Gray
            text_disabled(font_cache, config, option, x_pos, y_pos, font_size);
        } else {
            // Normal -> Config Color (White/etc)
            text_with_config_color(font_cache, config, option, x_pos, y_pos, font_size);
        }

        if option == "PLAY" && play_option_enabled {
            if let Some(name) = cart_name.as_deref() {
                let label = format!("• {}", shortened_cart_name(name, 36));
                let label_size = ((FONT_SIZE as f32 * 0.62) * scale_factor) as u16;
                let label_dims = measure_text(&label, Some(current_font), label_size, 1.0);
                let desired_x = x_pos + text_dims.width + (18.0 * scale_factor);
                let label_x =
                    desired_x.min(screen_width() - label_dims.width - (10.0 * scale_factor));
                text_with_config_color(font_cache, config, &label, label_x, y_pos, label_size);
            }
        }
    }

    // --- Draw the Flash Message if it exists ---
    if let Some(message) = flash_message {
        let font_size = (FONT_SIZE as f32 * scale_factor) as u16;
        let current_font = get_current_font(font_cache, config);

        // Measure the text to center it
        let dims = measure_text(message, Some(current_font), font_size, 1.0);

        // Calculate position (centered, near the bottom)
        let x = screen_width() / 2.0 - dims.width / 2.0;
        let y = screen_height() - (60.0 * scale_factor); // A bit above the version number

        // Draw a semi-transparent background for readability
        draw_rectangle(
            x - (10.0 * scale_factor),
            y - dims.height,
            dims.width + (20.0 * scale_factor),
            dims.height + (10.0 * scale_factor),
            Color::new(0.0, 0.0, 0.0, 0.7),
        );

        // Draw the message text itself
        text_with_config_color(font_cache, config, message, x, y, font_size);
    }

    // Keep the connection details visible without disturbing the configured
    // menu position. The FTP account itself is configured by the upgrade kit.
    let footer_font_size = (12.0 * scale_factor) as u16;
    let footer_y = screen_height() - (10.0 * scale_factor);
    text_with_config_color(
        font_cache,
        config,
        ftp_endpoint,
        10.0 * scale_factor,
        footer_y,
        footer_font_size,
    );

    profiles_state.draw_active_badge(font_cache, config, animation_state, scale_factor);
}
