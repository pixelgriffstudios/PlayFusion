use macroquad::prelude::*;
use std::collections::HashMap;
use std::process::Command;

use crate::{
    audio::SoundEffects,
    config::Config,
    get_current_font, measure_text, render_background, render_ui_overlay, text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo, Screen},
    ui::{draw_configured_cursor_frame, text_with_color},
    InputState, VideoPlayer, FONT_SIZE, MENU_OPTION_HEIGHT, MENU_PADDING,
};

pub const EXTRAS_MENU_OPTIONS: &[&str] = &[
    "CONNECT TO WI-FI",
    "PAIR BLUETOOTH CONTROLLER",
    "CD PLAYER",
    "DVD MOVIE",
    "MOVIES",
    "MP3 JUKEBOX",
    "STORAGE EXPANSION",
    "BIOS FILES",
    "GAME MANAGER",
    "CONTROLLER SETUP",
    "USER PROFILES",
    "PC CONTROLLER PROFILES",
];

/// Handles input and state logic for the Extras menu.
pub fn update(
    current_screen: &mut Screen,
    extras_menu_selection: &mut usize,
    input_state: &InputState,
    animation_state: &mut AnimationState,
    sound_effects: &SoundEffects,
    config: &Config,
) {
    if input_state.up {
        *extras_menu_selection = if *extras_menu_selection == 0 {
            EXTRAS_MENU_OPTIONS.len() - 1
        } else {
            *extras_menu_selection - 1
        };
        animation_state.trigger_transition(&config.cursor_transition_speed);
        sound_effects.play_cursor_move(config);
    }
    if input_state.down {
        *extras_menu_selection = (*extras_menu_selection + 1) % EXTRAS_MENU_OPTIONS.len();
        animation_state.trigger_transition(&config.cursor_transition_speed);
        sound_effects.play_cursor_move(config);
    }
    if input_state.back {
        *current_screen = Screen::MainMenu;
        sound_effects.play_back(config);
    }
    if input_state.select {
        sound_effects.play_select(config);
        match *extras_menu_selection {
            0 => *current_screen = Screen::Wifi,
            1 => *current_screen = Screen::Bluetooth,
            2 => *current_screen = Screen::CdPlayer,
            3 => *current_screen = Screen::DvdPlayer,
            4 => *current_screen = Screen::Movies,
            5 => *current_screen = Screen::Jukebox,
            6 => *current_screen = Screen::StorageExpansion,
            7 => *current_screen = Screen::BiosFiles,
            8 => *current_screen = Screen::GameManager,
            9 => *current_screen = Screen::ControllerSetup,
            10 => *current_screen = Screen::Profiles,
            11 => {
                let _ = Command::new("/usr/bin/playfusion-pc-controller-profiles").status();
            }
            _ => {}
        }
    }
}

/// Draws the Extras menu UI.
pub fn draw(
    selected_option: usize,
    animation_state: &AnimationState,
    logo_cache: &HashMap<String, Texture2D>,
    background_cache: &HashMap<String, Texture2D>,
    video_cache: &mut HashMap<String, VideoPlayer>,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    background_state: &mut BackgroundState,
    battery_info: &Option<BatteryInfo>,
    current_time_str: &str,
    gcc_adapter_poll_rate: &Option<u32>,
    scale_factor: f32,
) {
    render_background(background_cache, video_cache, config, background_state);

    // dim the background for easier legibility
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, 0.5),
    );

    render_ui_overlay(
        logo_cache,
        font_cache,
        config,
        battery_info,
        current_time_str,
        gcc_adapter_poll_rate,
        scale_factor,
    );

    let font_size = (FONT_SIZE as f32 * scale_factor) as u16;
    let menu_padding = MENU_PADDING * scale_factor;
    let menu_option_height = MENU_OPTION_HEIGHT * scale_factor;
    let current_font = get_current_font(font_cache, config);

    // Center the menu
    let start_x = screen_width() / 2.0;
    let start_y = screen_height() * 0.3;

    // Keep the menu inside the safe area at 720p while allowing more options
    // to be added later. The selected row stays within a scrolling window.
    let bottom_safe_y = screen_height() - 52.0 * scale_factor;
    let visible_rows = (((bottom_safe_y - start_y) / menu_option_height).floor() as usize)
        .clamp(4, EXTRAS_MENU_OPTIONS.len());
    let max_start = EXTRAS_MENU_OPTIONS.len().saturating_sub(visible_rows);
    let first_visible = selected_option
        .saturating_sub(visible_rows / 2)
        .min(max_start);
    let last_visible = (first_visible + visible_rows).min(EXTRAS_MENU_OPTIONS.len());

    for (row, i) in (first_visible..last_visible).enumerate() {
        let option = EXTRAS_MENU_OPTIONS[i];
        let y_pos = start_y + (row as f32 * menu_option_height);
        let text_dims = measure_text(option, Some(current_font), font_size, 1.0);
        let x_pos = start_x - (text_dims.width / 2.0);

        let is_selected = i == selected_option;

        // Draw selected option highlight
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
            let highlight_color = animation_state.get_cursor_color(config);
            text_with_color(
                font_cache,
                config,
                option,
                x_pos,
                y_pos,
                font_size,
                highlight_color,
            );
        } else {
            text_with_config_color(font_cache, config, option, x_pos, y_pos, font_size);
        }
    }

    let marker_size = (10.0 * scale_factor).max(8.0) as u16;
    if first_visible > 0 {
        let marker = "▲ MORE";
        let width = measure_text(marker, Some(current_font), marker_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            marker,
            (screen_width() - width) / 2.0,
            start_y - 24.0 * scale_factor,
            marker_size,
        );
    }
    if last_visible < EXTRAS_MENU_OPTIONS.len() {
        let marker = "▼ MORE";
        let width = measure_text(marker, Some(current_font), marker_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            marker,
            (screen_width() - width) / 2.0,
            bottom_safe_y + 20.0 * scale_factor,
            marker_size,
        );
    }
}
