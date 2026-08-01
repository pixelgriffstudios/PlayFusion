use crate::{
    config::Config,
    get_current_font, render_background, render_ui_overlay_without_version, text_with_config_color,
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

const HELPER: &str = "/usr/bin/kazeta-internal-game-helper";

#[derive(Clone, Debug, PartialEq)]
pub enum DriveKind {
    Active,
    Available,
}

#[derive(Clone, Debug)]
pub struct ExpansionDrive {
    pub kind: DriveKind,
    pub uuid: String,
    pub device: String,
    pub partition: String,
    pub size: String,
    pub free: String,
    pub model: String,
    pub serial: String,
    pub health: String,
    pub mount_path: String,
}

#[derive(Clone)]
pub enum StorageMode {
    List,
    ConfirmErase(ExpansionDrive),
    Busy(String),
    Message(String),
}

pub enum StorageEvent {
    None,
    Back,
    Move,
    Select,
    Reject,
}

pub struct StorageExpansionState {
    pub drives: Vec<ExpansionDrive>,
    pub selection: usize,
    pub loaded: bool,
    pub status: String,
    pub mode: StorageMode,
    operation_rx: Receiver<Result<String, String>>,
}

impl Default for StorageExpansionState {
    fn default() -> Self {
        let (_tx, operation_rx) = channel();
        Self {
            drives: Vec::new(),
            selection: 0,
            loaded: false,
            status: String::new(),
            mode: StorageMode::List,
            operation_rx,
        }
    }
}

impl StorageExpansionState {
    pub fn refresh(&mut self) {
        self.drives.clear();
        self.selection = 0;
        let output = Command::new("sudo")
            .arg(HELPER)
            .arg("list-storage")
            .output();
        match output {
            Ok(output) if output.status.success() => {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    if fields.len() != 10 {
                        continue;
                    }
                    let kind = match fields[0] {
                        "active" => DriveKind::Active,
                        "available" => DriveKind::Available,
                        _ => continue,
                    };
                    self.drives.push(ExpansionDrive {
                        kind,
                        uuid: fields[1].to_string(),
                        device: fields[2].to_string(),
                        partition: fields[3].to_string(),
                        size: fields[4].to_string(),
                        free: fields[5].to_string(),
                        model: fields[6].to_string(),
                        serial: fields[7].to_string(),
                        health: fields[8].to_string(),
                        mount_path: fields[9].to_string(),
                    });
                }
                self.drives.sort_by(|a, b| a.device.cmp(&b.device));
                let active = self
                    .drives
                    .iter()
                    .filter(|drive| drive.kind == DriveKind::Active)
                    .count();
                self.status = if self.drives.is_empty() {
                    "NO ELIGIBLE INTERNAL EXPANSION DRIVES".to_string()
                } else {
                    format!(
                        "{} DRIVE(S) DETECTED  |  {} ACTIVE",
                        self.drives.len(),
                        active
                    )
                };
            }
            Ok(output) => {
                let detail = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .last()
                    .unwrap_or("Unable to scan internal drives")
                    .trim()
                    .trim_start_matches("Error: ")
                    .to_string();
                self.status = format!("STORAGE SCAN FAILED: {detail}");
            }
            Err(error) => self.status = format!("STORAGE HELPER FAILED: {error}"),
        }
        self.loaded = true;
    }

    fn poll_operation(&mut self) {
        if let Ok(result) = self.operation_rx.try_recv() {
            self.mode = match result {
                Ok(message) => {
                    self.loaded = false;
                    StorageMode::Message(message)
                }
                Err(error) => StorageMode::Message(format!("ERROR: {error}")),
            };
        }
    }

    fn start_format(&mut self, drive: ExpansionDrive) {
        let device = drive.device.clone();
        let label = if drive.model.is_empty() {
            drive.device.clone()
        } else {
            drive.model.clone()
        };
        let (operation_tx, operation_rx) = channel();
        self.operation_rx = operation_rx;
        self.mode = StorageMode::Busy(format!("FORMATTING {label}..."));
        thread::spawn(move || {
            let result = Command::new("sudo")
                .arg(HELPER)
                .arg("format-storage")
                .arg(&device)
                .output()
                .map_err(|error| format!("Failed to start formatter: {error}"))
                .and_then(|output| {
                    if output.status.success() {
                        let message = String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .last()
                            .unwrap_or("Storage configured successfully")
                            .trim()
                            .to_uppercase();
                        Ok(message)
                    } else {
                        let detail = String::from_utf8_lossy(&output.stderr)
                            .lines()
                            .last()
                            .unwrap_or("Storage formatting failed")
                            .trim()
                            .trim_start_matches("Error: ")
                            .to_string();
                        Err(detail)
                    }
                });
            operation_tx.send(result).ok();
        });
    }

    pub fn handle_input(&mut self, input: &InputState) -> StorageEvent {
        self.poll_operation();
        match self.mode.clone() {
            StorageMode::List => {
                if input.up && !self.drives.is_empty() {
                    self.selection = if self.selection == 0 {
                        self.drives.len() - 1
                    } else {
                        self.selection - 1
                    };
                    return StorageEvent::Move;
                }
                if input.down && !self.drives.is_empty() {
                    self.selection = (self.selection + 1) % self.drives.len();
                    return StorageEvent::Move;
                }
                if input.secondary {
                    self.refresh();
                    return StorageEvent::Select;
                }
                if input.back {
                    return StorageEvent::Back;
                }
                if input.select {
                    let Some(drive) = self.drives.get(self.selection).cloned() else {
                        return StorageEvent::Reject;
                    };
                    self.mode = if drive.kind == DriveKind::Available {
                        StorageMode::ConfirmErase(drive)
                    } else {
                        StorageMode::Message(format!(
                            "{} | {} | {} FREE | HEALTH {}",
                            display_name(&drive),
                            drive.size,
                            drive.free,
                            drive.health
                        ))
                    };
                    return StorageEvent::Select;
                }
            }
            StorageMode::ConfirmErase(drive) => {
                if input.cycle {
                    self.start_format(drive);
                    return StorageEvent::Select;
                }
                if input.back {
                    self.mode = StorageMode::List;
                    return StorageEvent::Select;
                }
            }
            StorageMode::Busy(_) => {}
            StorageMode::Message(_) => {
                if input.select || input.back {
                    if !self.loaded {
                        self.refresh();
                    }
                    self.mode = StorageMode::List;
                    return StorageEvent::Select;
                }
            }
        }
        StorageEvent::None
    }
}

fn display_name(drive: &ExpansionDrive) -> String {
    if drive.model.trim().is_empty() {
        format!("INTERNAL DRIVE {}", drive.device)
    } else {
        drive.model.to_uppercase()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &StorageExpansionState,
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
        Color::new(0.0, 0.0, 0.02, 0.58),
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
    let body_size = (12.0 * scale_factor) as u16;
    let small_size = (9.0 * scale_factor).max(8.0) as u16;
    let title = "STORAGE EXPANSION";
    let title_width = measure_text(title, Some(font), title_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - title_width) / 2.0,
        82.0 * scale_factor,
        title_size,
    );

    if state.drives.is_empty() {
        let width = measure_text(&state.status, Some(font), body_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            &state.status,
            (screen_width() - width) / 2.0,
            screen_height() * 0.48,
            body_size,
        );
    } else {
        let row_width = screen_width() * 0.70;
        let row_height = 70.0 * scale_factor;
        let start_x = (screen_width() - row_width) / 2.0;
        let start_y = 116.0 * scale_factor;
        for (index, drive) in state.drives.iter().enumerate() {
            let y = start_y + index as f32 * (row_height + 11.0 * scale_factor);
            if y + row_height > screen_height() - 55.0 * scale_factor {
                break;
            }
            draw_rectangle(
                start_x,
                y,
                row_width,
                row_height,
                Color::new(0.01, 0.015, 0.04, 0.90),
            );
            draw_playfusion_panel_frame(
                start_x,
                y,
                row_width,
                row_height,
                2.0 * scale_factor,
                0.48,
            );
            if index == state.selection {
                draw_configured_cursor_frame(
                    config,
                    animation_state,
                    start_x - 4.0 * scale_factor,
                    y - 4.0 * scale_factor,
                    row_width + 8.0 * scale_factor,
                    row_height + 8.0 * scale_factor,
                    3.0 * scale_factor,
                );
            }
            let status = if drive.kind == DriveKind::Active {
                "PLAYFUSION STORAGE"
            } else {
                "AVAILABLE - FORMAT REQUIRED"
            };
            text_with_config_color(
                font_cache,
                config,
                &display_name(drive),
                start_x + 14.0 * scale_factor,
                y + 23.0 * scale_factor,
                body_size,
            );
            let device_label = if drive.kind == DriveKind::Active {
                &drive.partition
            } else {
                &drive.device
            };
            text_with_config_color(
                font_cache,
                config,
                &format!("{}  |  {}  |  {}", device_label, drive.size, status),
                start_x + 14.0 * scale_factor,
                y + 45.0 * scale_factor,
                small_size,
            );
            let right = if drive.kind == DriveKind::Active {
                format!("{} FREE  |  HEALTH {}", drive.free, drive.health)
            } else {
                format!("HEALTH {}", drive.health)
            };
            let right_width = measure_text(&right, Some(font), small_size, 1.0).width;
            text_with_config_color(
                font_cache,
                config,
                &right,
                start_x + row_width - right_width - 14.0 * scale_factor,
                y + 45.0 * scale_factor,
                small_size,
            );
        }
    }

    let controls = "A DETAILS / FORMAT   X REFRESH   B BACK";
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
    state: &StorageExpansionState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    let (title, lines) = match &state.mode {
        StorageMode::List => return,
        StorageMode::ConfirmErase(drive) => (
            "ERASE INTERNAL DRIVE?",
            vec![
                format!(
                    "{}  |  {}  |  {}",
                    display_name(drive),
                    drive.device,
                    drive.size
                ),
                format!(
                    "SERIAL: {}",
                    if drive.serial.is_empty() {
                        "UNKNOWN"
                    } else {
                        &drive.serial
                    }
                ),
                "ALL PARTITIONS AND FILES ON THIS DRIVE WILL BE LOST".to_string(),
                "THE PLAYFUSION BOOT DRIVE IS EXCLUDED AUTOMATICALLY".to_string(),
                "PRESS Y TO ERASE + FORMAT GPT/EXT4".to_string(),
                "PRESS B TO CANCEL".to_string(),
            ],
        ),
        StorageMode::Busy(message) => (
            "PLEASE WAIT",
            vec![
                message.clone(),
                "DO NOT POWER OFF OR DISCONNECT THE DRIVE".to_string(),
            ],
        ),
        StorageMode::Message(message) => (
            "STORAGE EXPANSION",
            vec![message.clone(), "A OR B TO CONTINUE".to_string()],
        ),
    };

    let font = get_current_font(font_cache, config);
    let font_size = (12.0 * scale_factor) as u16;
    let line_height = 22.0 * scale_factor;
    let width = screen_width() * 0.76;
    let height = (lines.len() as f32 + 2.5) * line_height;
    let x = (screen_width() - width) / 2.0;
    let y = (screen_height() - height) / 2.0;
    draw_rectangle(x, y, width, height, Color::new(0.0, 0.0, 0.02, 0.96));
    draw_playfusion_panel_frame(x, y, width, height, 3.0 * scale_factor, 0.92);
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
