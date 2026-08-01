use crate::{
    audio::SoundEffects, config::Config, get_current_font, measure_text, render_background,
    render_ui_overlay, text_with_config_color, BackgroundState, BatteryInfo, InputState, Screen,
    SystemInfo, VideoPlayer, FONT_SIZE,
};
use macroquad::prelude::*;
use std::collections::HashMap;

pub fn update(
    input_state: &InputState,
    current_screen: &mut Screen,
    sound_effects: &SoundEffects,
    config: &Config,
) {
    if input_state.back {
        *current_screen = Screen::MainMenu;
        sound_effects.play_back(config);
    }
}

pub fn draw(
    _system_info: &SystemInfo,
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
    render_ui_overlay(
        logo_cache,
        font_cache,
        config,
        battery_info,
        current_time_str,
        gcc_adapter_poll_rate,
        scale_factor,
    );

    let font = get_current_font(font_cache, config);
    let title_size = (FONT_SIZE as f32 * scale_factor * 1.05) as u16;
    let body_size = (FONT_SIZE as f32 * scale_factor * 0.78) as u16;
    let small_size = (FONT_SIZE as f32 * scale_factor * 0.66) as u16;
    let panel_width = screen_width() * 0.64;
    let panel_height = 225.0 * scale_factor;
    let panel_x = (screen_width() - panel_width) * 0.5;
    let panel_y = 92.0 * scale_factor;
    let light_background = super::backgrounds::is_light_background(&config.background_selection);
    let panel_color = if light_background {
        Color::new(1.0, 1.0, 1.0, 0.86)
    } else {
        Color::new(0.01, 0.015, 0.035, 0.82)
    };

    draw_rectangle(panel_x, panel_y, panel_width, panel_height, panel_color);
    draw_rectangle_lines(
        panel_x,
        panel_y,
        panel_width,
        panel_height,
        2.0 * scale_factor,
        Color::new(0.35, 0.82, 1.0, 0.66),
    );
    draw_line(
        panel_x + 32.0 * scale_factor,
        panel_y + 42.0 * scale_factor,
        panel_x + panel_width - 32.0 * scale_factor,
        panel_y + 42.0 * scale_factor,
        2.0 * scale_factor,
        Color::new(1.0, 0.30, 0.76, 0.62),
    );

    draw_centered(
        "ABOUT PLAYFUSION",
        panel_x + panel_width * 0.5,
        panel_y + 31.0 * scale_factor,
        title_size,
        font,
        font_cache,
        config,
    );
    draw_centered(
        "A fork of Kazeta and Kazeta+",
        panel_x + panel_width * 0.5,
        panel_y + 70.0 * scale_factor,
        body_size,
        font,
        font_cache,
        config,
    );

    let credits = [
        ("KAZETA", "Alkazar"),
        ("KAZETA+", "Linux Gaming Central"),
        ("KAZETA+", "The \"Overly Complex\" Kazeta+ Guy"),
        ("PLAYFUSION", "Jason Griffith"),
    ];
    let mut y = panel_y + 102.0 * scale_factor;
    for (project, developer) in credits {
        let line = format!("{project}  -  {developer}");
        draw_centered(
            &line,
            panel_x + panel_width * 0.5,
            y,
            body_size,
            font,
            font_cache,
            config,
        );
        y += 28.0 * scale_factor;
    }

    draw_centered(
        "Every generation. One system.",
        panel_x + panel_width * 0.5,
        panel_y + 211.0 * scale_factor,
        small_size,
        font,
        font_cache,
        config,
    );
}

fn draw_centered(
    text: &str,
    center_x: f32,
    y: f32,
    font_size: u16,
    font: &Font,
    font_cache: &HashMap<String, Font>,
    config: &Config,
) {
    let dims = measure_text(text, Some(font), font_size, 1.0);
    text_with_config_color(
        font_cache,
        config,
        text,
        center_x - dims.width * 0.5,
        y,
        font_size,
    );
}
