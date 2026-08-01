use macroquad::prelude::*;
use std::process::Command;

use crate::{
    audio::SoundEffects,
    config::Config,
    get_current_font, measure_text, render_background, render_ui_overlay, text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo, Screen},
    ui::{draw_configured_cursor_frame, text_with_color},
    InputState, VideoPlayer, FONT_SIZE, MENU_OPTION_HEIGHT, MENU_PADDING,
};
use std::collections::HashMap;

pub const POWER_MENU_OPTIONS: &[&str] = &["SHUT DOWN", "REBOOT", "CANCEL"];

fn run_power_command(action: &str, arguments: &[&str]) {
    if let Err(error) = Command::new("sudo")
        .arg("-n")
        .arg(action)
        .args(arguments)
        .spawn()
    {
        eprintln!("[Power] Failed to run {action}: {error}");
    }
}

pub fn update(
    current_screen: &mut Screen,
    selection: &mut usize,
    input_state: &InputState,
    animation_state: &mut AnimationState,
    sound_effects: &SoundEffects,
    config: &Config,
) {
    if input_state.up {
        *selection = if *selection == 0 {
            POWER_MENU_OPTIONS.len() - 1
        } else {
            *selection - 1
        };
        animation_state.trigger_transition(&config.cursor_transition_speed);
        sound_effects.play_cursor_move(config);
    }
    if input_state.down {
        *selection = (*selection + 1) % POWER_MENU_OPTIONS.len();
        animation_state.trigger_transition(&config.cursor_transition_speed);
        sound_effects.play_cursor_move(config);
    }
    if input_state.back {
        *current_screen = Screen::MainMenu;
        sound_effects.play_back(config);
        return;
    }
    if input_state.select {
        match *selection {
            0 => {
                sound_effects.play_select(config);
                run_power_command("shutdown", &["now"]);
            }
            1 => {
                sound_effects.play_select(config);
                run_power_command("reboot", &[]);
            }
            _ => {
                *current_screen = Screen::MainMenu;
                sound_effects.play_back(config);
            }
        }
    }
}

pub fn draw(
    selection: usize,
    animation_state: &AnimationState,
    logo_cache: &HashMap<String, Texture2D>,
    background_cache: &HashMap<String, Texture2D>,
    video_cache: &mut HashMap<String, VideoPlayer>,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    background_state: &mut BackgroundState,
    battery_info: &Option<BatteryInfo>,
    scale_factor: f32,
) {
    render_background(background_cache, video_cache, config, background_state);
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    render_ui_overlay(
        logo_cache,
        font_cache,
        config,
        battery_info,
        "",
        &None,
        scale_factor,
    );

    let current_font = get_current_font(font_cache, config);
    let title_size = (FONT_SIZE as f32 * 1.25 * scale_factor) as u16;
    let title = "POWER";
    let title_dims = measure_text(title, Some(current_font), title_size, 1.0);
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - title_dims.width) / 2.0,
        screen_height() * 0.32,
        title_size,
    );

    let prompt_size = (FONT_SIZE as f32 * 0.70 * scale_factor) as u16;
    let prompt = "CHOOSE A SYSTEM ACTION";
    let prompt_dims = measure_text(prompt, Some(current_font), prompt_size, 1.0);
    text_with_config_color(
        font_cache,
        config,
        prompt,
        (screen_width() - prompt_dims.width) / 2.0,
        screen_height() * 0.39,
        prompt_size,
    );

    let font_size = (FONT_SIZE as f32 * scale_factor) as u16;
    let menu_padding = MENU_PADDING * scale_factor;
    let menu_option_height = MENU_OPTION_HEIGHT * scale_factor;
    let start_y = screen_height() * 0.54;

    for (index, option) in POWER_MENU_OPTIONS.iter().enumerate() {
        let y_pos = start_y + (index as f32 * menu_option_height);
        let text_dims = measure_text(option, Some(current_font), font_size, 1.0);
        let x_pos = (screen_width() - text_dims.width) / 2.0;
        let is_selected = index == selection;

        if is_selected && config.cursor_style == "BOX" {
            let cursor_scale = animation_state.get_cursor_scale();
            let base_width = text_dims.width + (menu_padding * 2.0);
            let base_height = text_dims.height + (menu_padding * 2.0);
            let scaled_width = base_width * cursor_scale;
            let scaled_height = base_height * cursor_scale;
            let offset_x = (scaled_width - base_width) / 2.0;
            let offset_y = (scaled_height - base_height) / 2.0;

            draw_configured_cursor_frame(
                config,
                animation_state,
                x_pos - menu_padding - offset_x,
                y_pos - text_dims.height - menu_padding - offset_y,
                scaled_width,
                scaled_height,
                4.0 * scale_factor,
            );
        }

        if is_selected && config.cursor_style == "TEXT" {
            text_with_color(
                font_cache,
                config,
                option,
                x_pos,
                y_pos,
                font_size,
                animation_state.get_cursor_color(config),
            );
        } else {
            text_with_config_color(font_cache, config, option, x_pos, y_pos, font_size);
        }
    }
}
