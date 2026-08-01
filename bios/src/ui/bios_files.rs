use crate::{
    config::Config,
    get_current_font, render_background, render_ui_overlay_without_version, text_with_color,
    text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo},
    ui::{draw_configured_cursor_frame, draw_playfusion_panel_frame},
    InputState, VideoPlayer,
};
use macroquad::prelude::*;
use std::{
    collections::HashMap,
    process::Command,
    sync::mpsc::{channel, Receiver},
    thread,
};

const HELPER: &str = "/usr/bin/playfusion-bios-manager";
#[derive(Clone, Debug)]
pub struct BiosEntry {
    pub system: String,
    pub state: String,
    pub requirement: String,
    pub detail: String,
}

#[derive(Clone)]
pub enum BiosMode {
    List,
    Busy,
    Message(String),
}

pub enum BiosEvent {
    None,
    Back,
    Move,
    Select,
}

pub struct BiosFilesState {
    pub entries: Vec<BiosEntry>,
    pub selection: usize,
    pub loaded: bool,
    pub status: String,
    pub mode: BiosMode,
    operation_rx: Receiver<Result<String, String>>,
}

impl Default for BiosFilesState {
    fn default() -> Self {
        let (_tx, operation_rx) = channel();
        Self {
            entries: Vec::new(),
            selection: 0,
            loaded: false,
            status: String::new(),
            mode: BiosMode::List,
            operation_rx,
        }
    }
}

impl BiosFilesState {
    pub fn refresh(&mut self) {
        self.entries.clear();
        let output = Command::new("sudo").arg(HELPER).arg("status").output();
        match output {
            Ok(output) if output.status.success() => {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    if fields.len() != 4 {
                        continue;
                    }
                    self.entries.push(BiosEntry {
                        system: fields[0].to_string(),
                        state: fields[1].to_string(),
                        requirement: fields[2].to_string(),
                        detail: fields[3].to_string(),
                    });
                }
                let present = self
                    .entries
                    .iter()
                    .filter(|entry| entry.state == "PRESENT")
                    .count();
                let required_missing = self
                    .entries
                    .iter()
                    .filter(|entry| {
                        (entry.state == "MISSING" || entry.state == "INCOMPLETE")
                            && entry.requirement == "REQUIRED"
                    })
                    .count();
                self.status = format!(
                    "{} PRESENT  |  {} REQUIRED MISSING/INCOMPLETE",
                    present, required_missing
                );
                if !self.entries.is_empty() {
                    self.selection = self.selection.min(self.entries.len() - 1);
                }
            }
            Ok(output) => {
                self.status = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .last()
                    .unwrap_or("BIOS INVENTORY FAILED")
                    .trim()
                    .to_uppercase();
            }
            Err(error) => self.status = format!("BIOS HELPER FAILED: {error}"),
        }
        self.loaded = true;
    }

    fn start_scan(&mut self) {
        let (operation_tx, operation_rx) = channel();
        self.operation_rx = operation_rx;
        self.mode = BiosMode::Busy;
        thread::spawn(move || {
            let result = Command::new("sudo")
                .arg(HELPER)
                .arg("scan")
                .output()
                .map_err(|error| format!("FAILED TO START USB SCAN: {error}"))
                .and_then(|output| {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let summary = stdout
                        .lines()
                        .rev()
                        .find(|line| line.starts_with("SUMMARY\t"))
                        .map(|line| line.replace('\t', "  "))
                        .or_else(|| {
                            stdout
                                .lines()
                                .rev()
                                .find(|line| !line.trim().is_empty())
                                .map(|line| line.replace('\t', "  "))
                        })
                        .unwrap_or_else(|| "NO RECOGNIZED BIOS FILES FOUND".to_string());
                    if output.status.success() {
                        Ok(summary)
                    } else {
                        let detail = String::from_utf8_lossy(&output.stderr)
                            .lines()
                            .last()
                            .filter(|line| !line.trim().is_empty())
                            .map(str::to_string)
                            .or_else(|| {
                                stdout
                                    .lines()
                                    .last()
                                    .filter(|line| !line.trim().is_empty())
                                    .map(str::to_string)
                            })
                            .unwrap_or_else(|| "NO RECOGNIZED BIOS FILES FOUND".to_string());
                        Err(detail.replace('\t', "  "))
                    }
                });
            operation_tx.send(result).ok();
        });
    }

    fn poll_operation(&mut self) {
        if let Ok(result) = self.operation_rx.try_recv() {
            self.refresh();
            self.mode = BiosMode::Message(match result {
                Ok(message) => message,
                Err(error) => format!("SCAN FAILED: {error}"),
            });
        }
    }

    pub fn handle_input(&mut self, input: &InputState) -> BiosEvent {
        self.poll_operation();
        match self.mode.clone() {
            BiosMode::List => {
                if input.up && !self.entries.is_empty() {
                    self.selection = if self.selection == 0 {
                        self.entries.len() - 1
                    } else {
                        self.selection - 1
                    };
                    return BiosEvent::Move;
                }
                if input.down && !self.entries.is_empty() {
                    self.selection = (self.selection + 1) % self.entries.len();
                    return BiosEvent::Move;
                }
                if input.secondary {
                    self.refresh();
                    return BiosEvent::Select;
                }
                if input.select {
                    self.start_scan();
                    return BiosEvent::Select;
                }
                if input.back {
                    return BiosEvent::Back;
                }
            }
            BiosMode::Busy => {}
            BiosMode::Message(_) => {
                if input.select || input.back {
                    self.mode = BiosMode::List;
                    return BiosEvent::Select;
                }
            }
        }
        BiosEvent::None
    }
}

fn state_color(state: &str) -> Color {
    match state {
        "PRESENT" => Color::new(0.25, 1.0, 0.55, 1.0),
        "MISSING" | "INCOMPLETE" => Color::new(1.0, 0.28, 0.42, 1.0),
        "MANUAL" => Color::new(0.95, 0.55, 1.0, 1.0),
        _ => Color::new(1.0, 0.82, 0.28, 1.0),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &BiosFilesState,
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
        Color::new(0.0, 0.0, 0.02, 0.62),
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
    let title_size = (20.0 * scale_factor) as u16;
    let body_size = (11.0 * scale_factor).max(8.0) as u16;
    let small_size = (8.5 * scale_factor).max(7.0) as u16;
    let title = "BIOS FILES";
    let title_width = measure_text(title, Some(font), title_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - title_width) / 2.0,
        screen_height() * 0.12,
        title_size,
    );

    let summary_width = measure_text(&state.status, Some(font), small_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        &state.status,
        (screen_width() - summary_width) / 2.0,
        screen_height() * 0.155,
        small_size,
    );

    if state.entries.is_empty() {
        let message = "NO BIOS INVENTORY AVAILABLE";
        let width = measure_text(message, Some(font), body_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            message,
            (screen_width() - width) / 2.0,
            screen_height() * 0.48,
            body_size,
        );
    } else {
        let row_width = screen_width() * 0.76;
        let row_height = 48.0 * scale_factor;
        let gap = 7.0 * scale_factor;
        let start_x = (screen_width() - row_width) / 2.0;
        let start_y = screen_height() * 0.19;
        let bottom_safe_y = screen_height() - 55.0 * scale_factor;
        let visible_rows = (((bottom_safe_y - start_y + gap) / (row_height + gap)).floor()
            as usize)
            .clamp(3, 6)
            .min(state.entries.len().max(1));
        let first = state
            .selection
            .saturating_sub(visible_rows / 2)
            .min(state.entries.len().saturating_sub(visible_rows));

        for (visible_index, entry) in state
            .entries
            .iter()
            .skip(first)
            .take(visible_rows)
            .enumerate()
        {
            let index = first + visible_index;
            let y = start_y + visible_index as f32 * (row_height + gap);
            draw_rectangle(
                start_x,
                y,
                row_width,
                row_height,
                Color::new(0.01, 0.015, 0.04, 0.92),
            );
            draw_playfusion_panel_frame(
                start_x,
                y,
                row_width,
                row_height,
                1.5 * scale_factor,
                0.46,
            );
            if index == state.selection {
                draw_configured_cursor_frame(
                    config,
                    animation_state,
                    start_x - 3.0 * scale_factor,
                    y - 3.0 * scale_factor,
                    row_width + 6.0 * scale_factor,
                    row_height + 6.0 * scale_factor,
                    2.5 * scale_factor,
                );
            }
            text_with_config_color(
                font_cache,
                config,
                &entry.system,
                start_x + 12.0 * scale_factor,
                y + 18.0 * scale_factor,
                body_size,
            );
            text_with_config_color(
                font_cache,
                config,
                &entry.detail,
                start_x + 12.0 * scale_factor,
                y + 37.0 * scale_factor,
                small_size,
            );
            let state_label = format!("{}  |  {}", entry.state, entry.requirement);
            let state_width = measure_text(&state_label, Some(font), small_size, 1.0).width;
            text_with_color(
                font_cache,
                config,
                &state_label,
                start_x + row_width - state_width - 12.0 * scale_factor,
                y + 18.0 * scale_factor,
                small_size,
                state_color(&entry.state),
            );
        }

        if first > 0 {
            let marker = "▲ MORE";
            let width = measure_text(marker, Some(font), small_size, 1.0).width;
            text_with_config_color(
                font_cache,
                config,
                marker,
                (screen_width() - width) / 2.0,
                start_y - 10.0 * scale_factor,
                small_size,
            );
        }
        if first + visible_rows < state.entries.len() {
            let marker = "▼ MORE";
            let width = measure_text(marker, Some(font), small_size, 1.0).width;
            text_with_config_color(
                font_cache,
                config,
                marker,
                (screen_width() - width) / 2.0,
                bottom_safe_y + 18.0 * scale_factor,
                small_size,
            );
        }
    }

    let page = if state.entries.is_empty() {
        String::new()
    } else {
        format!("{} / {}", state.selection + 1, state.entries.len())
    };
    text_with_config_color(
        font_cache,
        config,
        &page,
        18.0 * scale_factor,
        screen_height() - 10.0 * scale_factor,
        small_size,
    );
    let controls = "A SCAN USB   X REFRESH   B BACK";
    let controls_width = measure_text(controls, Some(font), small_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        controls,
        (screen_width() - controls_width) / 2.0,
        screen_height() - 10.0 * scale_factor,
        small_size,
    );
    draw_mode_overlay(state, font_cache, config, scale_factor);
}

fn draw_mode_overlay(
    state: &BiosFilesState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    let (title, lines) = match &state.mode {
        BiosMode::List => return,
        BiosMode::Busy => (
            "SCANNING USB",
            vec![
                "CHECKING RECOGNIZED BIOS NAMES AND FILE SIZES".to_string(),
                "EXISTING FILES WILL NOT BE OVERWRITTEN".to_string(),
                "DO NOT REMOVE THE USB DRIVE".to_string(),
            ],
        ),
        BiosMode::Message(message) => (
            "BIOS USB IMPORT",
            vec![
                message.clone(),
                "UNKNOWN FILES WERE LEFT UNTOUCHED".to_string(),
                "A OR B TO CONTINUE".to_string(),
            ],
        ),
    };

    let font = get_current_font(font_cache, config);
    let font_size = (11.0 * scale_factor).max(8.0) as u16;
    let line_height = 23.0 * scale_factor;
    let width = screen_width() * 0.78;
    let height = (lines.len() as f32 + 2.5) * line_height;
    let x = (screen_width() - width) / 2.0;
    let y = (screen_height() - height) / 2.0;
    draw_rectangle(x, y, width, height, Color::new(0.0, 0.0, 0.02, 0.97));
    draw_playfusion_panel_frame(x, y, width, height, 3.0 * scale_factor, 0.94);
    let title_width = measure_text(title, Some(font), font_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - title_width) / 2.0,
        y + line_height,
        font_size,
    );
    for (index, line) in lines.iter().enumerate() {
        let line_width = measure_text(line, Some(font), font_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            line,
            (screen_width() - line_width) / 2.0,
            y + line_height * (2.2 + index as f32),
            font_size,
        );
    }
}
