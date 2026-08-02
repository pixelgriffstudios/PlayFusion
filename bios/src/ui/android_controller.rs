use crate::{
    config::Config,
    get_current_font, render_background, render_ui_overlay_without_version, text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo},
    ui::{draw_configured_cursor_frame, text_with_color},
    InputState, VideoPlayer,
};
use macroquad::prelude::*;
use std::{collections::HashMap, process::Command};

#[derive(Clone)]
struct AndroidGame {
    id: String,
    name: String,
    source: String,
    mode: String,
}

pub enum AndroidControllerEvent {
    None,
    Move,
    Select,
    Reject,
    Back,
}

#[derive(Default)]
pub struct AndroidControllerState {
    games: Vec<AndroidGame>,
    selection: usize,
    loaded: bool,
    message: String,
}

impl AndroidControllerState {
    pub fn refresh(&mut self) {
        self.games.clear();
        self.message.clear();
        match Command::new("/usr/bin/playfusion-android-controller-mode")
            .arg("list")
            .output()
        {
            Ok(output) if output.status.success() => {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields: Vec<_> = line.split('\t').collect();
                    if fields.len() == 4 {
                        self.games.push(AndroidGame {
                            id: fields[0].to_string(),
                            name: fields[1].to_string(),
                            source: fields[2].to_string(),
                            mode: fields[3].to_string(),
                        });
                    }
                }
            }
            Ok(output) => self.message = String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Err(error) => self.message = error.to_string(),
        }
        self.selection = self.selection.min(self.games.len().saturating_sub(1));
        self.loaded = true;
    }

    fn toggle_selected(&mut self) -> bool {
        let Some(game) = self.games.get_mut(self.selection) else {
            return false;
        };
        let next = if game.mode == "touch" {
            "gamepad"
        } else {
            "touch"
        };
        match Command::new("/usr/bin/playfusion-android-controller-mode")
            .args(["set", &game.id, next])
            .output()
        {
            Ok(output) if output.status.success() => {
                game.mode = next.to_string();
                self.message = format!("{} SET TO {} CONTROLS", game.name, next.to_uppercase());
                true
            }
            Ok(output) => {
                self.message = String::from_utf8_lossy(&output.stderr).trim().to_string();
                false
            }
            Err(error) => {
                self.message = error.to_string();
                false
            }
        }
    }

    pub fn handle_input(&mut self, input: &InputState) -> AndroidControllerEvent {
        if !self.loaded {
            self.refresh();
        }
        if input.up && !self.games.is_empty() {
            self.selection = if self.selection == 0 {
                self.games.len() - 1
            } else {
                self.selection - 1
            };
            return AndroidControllerEvent::Move;
        }
        if input.down && !self.games.is_empty() {
            self.selection = (self.selection + 1) % self.games.len();
            return AndroidControllerEvent::Move;
        }
        if input.back {
            self.loaded = false;
            return AndroidControllerEvent::Back;
        }
        if input.cycle {
            self.refresh();
            return AndroidControllerEvent::Select;
        }
        if input.select {
            return if self.toggle_selected() {
                AndroidControllerEvent::Select
            } else {
                AndroidControllerEvent::Reject
            };
        }
        AndroidControllerEvent::None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &AndroidControllerState,
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
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, 0.68),
    );
    render_ui_overlay_without_version(
        logo_cache,
        font_cache,
        config,
        battery_info,
        current_time_str,
        gcc_adapter_poll_rate,
        scale_factor,
    );
    let font = get_current_font(font_cache, config);
    let title_size = (25.0 * scale_factor) as u16;
    let text_size = (15.0 * scale_factor) as u16;
    let small_size = (10.0 * scale_factor).max(8.0) as u16;
    let title = "ANDROID CONTROLLER SUPPORT";
    let width = measure_text(title, Some(font), title_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - width) / 2.0,
        screen_height() * 0.20,
        title_size,
    );

    if state.games.is_empty() {
        let message = if state.message.is_empty() {
            "NO INSTALLED OR USB ANDROID GAMES FOUND"
        } else {
            &state.message
        };
        let width = measure_text(message, Some(font), text_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            message,
            (screen_width() - width) / 2.0,
            screen_height() * 0.48,
            text_size,
        );
    } else {
        let start_y = screen_height() * 0.31;
        let row_height = 38.0 * scale_factor;
        let bottom = screen_height() - 64.0 * scale_factor;
        let visible = (((bottom - start_y) / row_height).floor() as usize)
            .clamp(3, 9)
            .min(state.games.len());
        let first = state
            .selection
            .saturating_sub(visible / 2)
            .min(state.games.len().saturating_sub(visible));
        for (row, index) in (first..(first + visible)).enumerate() {
            let game = &state.games[index];
            let y = start_y + row as f32 * row_height;
            let mode = if game.mode == "touch" {
                "TOUCH CONTROLS"
            } else {
                "GAMEPAD CONTROLS"
            };
            let display = format!("{}  [{}]  {}", game.name, game.source, mode);
            let clipped: String = display.chars().take(68).collect();
            let dims = measure_text(&clipped, Some(font), text_size, 1.0);
            let x = (screen_width() - dims.width) / 2.0;
            if index == state.selection {
                draw_configured_cursor_frame(
                    config,
                    animation_state,
                    x - 14.0 * scale_factor,
                    y - dims.height - 7.0 * scale_factor,
                    dims.width + 28.0 * scale_factor,
                    dims.height + 14.0 * scale_factor,
                    3.0 * scale_factor,
                );
                text_with_color(
                    font_cache,
                    config,
                    &clipped,
                    x,
                    y,
                    text_size,
                    animation_state.get_cursor_color(config),
                );
            } else {
                text_with_config_color(font_cache, config, &clipped, x, y, text_size);
            }
        }
    }
    if !state.message.is_empty() {
        let clipped: String = state.message.chars().take(80).collect();
        let width = measure_text(&clipped, Some(font), small_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            &clipped,
            (screen_width() - width) / 2.0,
            screen_height() - 39.0 * scale_factor,
            small_size,
        );
    }
    let footer = "A SWITCH TOUCH / GAMEPAD   Y REFRESH USB   B BACK";
    let width = measure_text(footer, Some(font), small_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        footer,
        (screen_width() - width) / 2.0,
        screen_height() - 17.0 * scale_factor,
        small_size,
    );
}
