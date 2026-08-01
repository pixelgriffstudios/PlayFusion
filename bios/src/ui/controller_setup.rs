use crate::{
    config::Config, get_current_font, measure_text, render_background,
    render_ui_overlay_without_version, text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo},
    ui::{draw_configured_cursor_frame, text_with_color},
    InputState, VideoPlayer,
};
use macroquad::prelude::*;
use std::{collections::HashMap, process::Command};

const SYSTEMS: &[&str] = &[
    "DEFAULT", "ARCADE", "NES", "SNES", "NINTENDO 64", "GAME BOY FAMILY",
    "NINTENDO DS", "NINTENDO 3DS", "GAMECUBE", "WII", "WII U", "SEGA GENESIS",
    "SEGA CD", "SEGA SATURN", "DREAMCAST", "PLAYSTATION", "PLAYSTATION 2", "PSP",
    "PLAYSTATION VITA", "ORIGINAL XBOX", "PC GAMES",
];

const PRESETS: &[(&str, &str)] = &[
    ("XBOX STANDARD", "xbox-standard"),
    ("NINTENDO FACE BUTTONS", "nintendo-face"),
    ("SWAP A / B", "swap-ab"),
    ("SWAP X / Y", "swap-xy"),
];

#[derive(Clone, PartialEq)]
enum ControllerSetupMode {
    Main,
    Systems,
    Presets,
    Message,
}

pub enum ControllerSetupEvent {
    None,
    Move,
    Select,
    Reject,
    Back,
}

pub struct ControllerSetupState {
    mode: ControllerSetupMode,
    selection: usize,
    system_selection: usize,
    preset_selection: usize,
    controllers: Vec<String>,
    message: String,
    loaded: bool,
}

impl Default for ControllerSetupState {
    fn default() -> Self {
        Self {
            mode: ControllerSetupMode::Main,
            selection: 0,
            system_selection: 0,
            preset_selection: 0,
            controllers: Vec::new(),
            message: String::new(),
            loaded: false,
        }
    }
}

impl ControllerSetupState {
    pub fn refresh(&mut self) {
        self.controllers.clear();
        if let Ok(output) = Command::new("sudo")
            .arg("/usr/bin/playfusion-controller-setup")
            .arg("list")
            .output()
        {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    if fields.len() >= 5 {
                        self.controllers.push(format!(
                            "{}  [{}:{}]  {}",
                            fields[1], fields[2], fields[3], fields[4]
                        ));
                    }
                }
            }
        }
        if self.controllers.is_empty() {
            self.controllers
                .push("NO PHYSICAL CONTROLLER DETECTED".to_string());
        }
        self.loaded = true;
    }

    fn register_controller(&mut self) -> bool {
        match Command::new("sudo")
            .arg("/usr/bin/playfusion-controller-setup")
            .arg("register")
            .output()
        {
            Ok(output) if output.status.success() => {
                self.message = format!(
                    "{}\nUNPLUG AND RECONNECT THE CONTROLLER ONCE.",
                    String::from_utf8_lossy(&output.stdout).trim()
                );
                true
            }
            Ok(output) => {
                let error = String::from_utf8_lossy(&output.stderr);
                self.message = if error.contains("No unsupported controller") {
                    "ALL CONNECTED CONTROLLERS ARE ALREADY XBOX READY".to_string()
                } else {
                    format!("CONTROLLER REGISTRATION FAILED\n{}", error.trim())
                };
                false
            }
            Err(error) => {
                self.message = format!("CONTROLLER REGISTRATION FAILED\n{error}");
                false
            }
        }
    }

    fn save_preset(&mut self) -> bool {
        let system = SYSTEMS[self.system_selection];
        let (label, preset) = PRESETS[self.preset_selection];
        match Command::new("sudo")
            .arg("/usr/bin/playfusion-controller-setup")
            .arg("set-system")
            .arg(system)
            .arg(preset)
            .output()
        {
            Ok(output) if output.status.success() => {
                self.message = format!("{system}\n{label}\nSAVED");
                true
            }
            Ok(output) => {
                self.message = format!(
                    "UNABLE TO SAVE LAYOUT\n{}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                false
            }
            Err(error) => {
                self.message = format!("UNABLE TO SAVE LAYOUT\n{error}");
                false
            }
        }
    }

    pub fn handle_input(&mut self, input: &InputState) -> ControllerSetupEvent {
        if !self.loaded {
            self.refresh();
        }
        match self.mode {
            ControllerSetupMode::Main => {
                const OPTIONS: usize = 3;
                if input.up {
                    self.selection = if self.selection == 0 {
                        OPTIONS - 1
                    } else {
                        self.selection - 1
                    };
                    return ControllerSetupEvent::Move;
                }
                if input.down {
                    self.selection = (self.selection + 1) % OPTIONS;
                    return ControllerSetupEvent::Move;
                }
                if input.back {
                    self.loaded = false;
                    return ControllerSetupEvent::Back;
                }
                if input.select {
                    match self.selection {
                        0 => {
                            let success = self.register_controller();
                            self.mode = ControllerSetupMode::Message;
                            return if success {
                                ControllerSetupEvent::Select
                            } else {
                                ControllerSetupEvent::Reject
                            };
                        }
                        1 => {
                            self.mode = ControllerSetupMode::Systems;
                            return ControllerSetupEvent::Select;
                        }
                        _ => {
                            self.refresh();
                            return ControllerSetupEvent::Select;
                        }
                    }
                }
            }
            ControllerSetupMode::Systems => {
                if input.up {
                    self.system_selection = if self.system_selection == 0 {
                        SYSTEMS.len() - 1
                    } else {
                        self.system_selection - 1
                    };
                    return ControllerSetupEvent::Move;
                }
                if input.down {
                    self.system_selection = (self.system_selection + 1) % SYSTEMS.len();
                    return ControllerSetupEvent::Move;
                }
                if input.back {
                    self.mode = ControllerSetupMode::Main;
                    return ControllerSetupEvent::Select;
                }
                if input.select {
                    self.preset_selection = 0;
                    self.mode = ControllerSetupMode::Presets;
                    return ControllerSetupEvent::Select;
                }
            }
            ControllerSetupMode::Presets => {
                if input.up {
                    self.preset_selection = if self.preset_selection == 0 {
                        PRESETS.len() - 1
                    } else {
                        self.preset_selection - 1
                    };
                    return ControllerSetupEvent::Move;
                }
                if input.down {
                    self.preset_selection = (self.preset_selection + 1) % PRESETS.len();
                    return ControllerSetupEvent::Move;
                }
                if input.back {
                    self.mode = ControllerSetupMode::Systems;
                    return ControllerSetupEvent::Select;
                }
                if input.select {
                    let success = self.save_preset();
                    self.mode = ControllerSetupMode::Message;
                    return if success {
                        ControllerSetupEvent::Select
                    } else {
                        ControllerSetupEvent::Reject
                    };
                }
            }
            ControllerSetupMode::Message => {
                if input.select || input.back {
                    self.mode = ControllerSetupMode::Main;
                    self.refresh();
                    return ControllerSetupEvent::Select;
                }
            }
        }
        ControllerSetupEvent::None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &ControllerSetupState,
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
        Color::new(0.0, 0.0, 0.0, 0.62),
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
    let title_size = (28.0 * scale_factor) as u16;
    let text_size = (18.0 * scale_factor) as u16;
    let small_size = (12.0 * scale_factor) as u16;
    let title = "CONTROLLER SETUP";
    let title_width = measure_text(title, Some(font), title_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        title,
        (screen_width() - title_width) / 2.0,
        screen_height() * 0.22,
        title_size,
    );

    let (items, selection, footer): (Vec<String>, usize, &str) = match state.mode {
        ControllerSetupMode::Main => {
            let controller = state
                .controllers
                .first()
                .cloned()
                .unwrap_or_else(|| "NO CONTROLLER".to_string());
            (
                vec![
                    "REGISTER CONNECTED CONTROLLER AS XBOX".to_string(),
                    "SYSTEM BUTTON LAYOUTS".to_string(),
                    "REFRESH CONTROLLERS".to_string(),
                    String::new(),
                    controller,
                ],
                state.selection,
                "A SELECT   B BACK",
            )
        }
        ControllerSetupMode::Systems => (
            SYSTEMS.iter().map(|value| (*value).to_string()).collect(),
            state.system_selection,
            "A CHOOSE SYSTEM   B BACK",
        ),
        ControllerSetupMode::Presets => (
            PRESETS
                .iter()
                .map(|(label, _)| (*label).to_string())
                .collect(),
            state.preset_selection,
            "A SAVE LAYOUT   B BACK",
        ),
        ControllerSetupMode::Message => (
            state.message.lines().map(str::to_string).collect(),
            usize::MAX,
            "A OR B CONTINUE",
        ),
    };

    let start_y = screen_height() * 0.31;
    let row_height = 38.0 * scale_factor;
    let bottom_safe_y = screen_height() - 68.0 * scale_factor;
    let visible_rows = (((bottom_safe_y - start_y) / row_height).floor() as usize)
        .clamp(3, 9)
        .min(items.len().max(1));
    let scroll_start = if selection == usize::MAX {
        0
    } else {
        selection
            .saturating_sub(visible_rows / 2)
            .min(items.len().saturating_sub(visible_rows))
    };
    for (visible_index, item) in items
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_rows)
    {
        let y = start_y + ((visible_index - scroll_start) as f32 * row_height);
        let dims = measure_text(item, Some(font), text_size, 1.0);
        let x = (screen_width() - dims.width) / 2.0;
        if visible_index == selection {
            draw_configured_cursor_frame(
                config,
                animation_state,
                x - 18.0 * scale_factor,
                y - dims.height - 8.0 * scale_factor,
                dims.width + 36.0 * scale_factor,
                dims.height + 16.0 * scale_factor,
                4.0 * scale_factor,
            );
            text_with_color(
                font_cache,
                config,
                item,
                x,
                y,
                text_size,
                animation_state.get_cursor_color(config),
            );
        } else {
            text_with_config_color(font_cache, config, item, x, y, text_size);
        }
    }

    if scroll_start > 0 {
        let marker = "▲ MORE";
        let width = measure_text(marker, Some(font), small_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            marker,
            (screen_width() - width) / 2.0,
            start_y - 24.0 * scale_factor,
            small_size,
        );
    }
    if scroll_start + visible_rows < items.len() {
        let marker = "▼ MORE";
        let width = measure_text(marker, Some(font), small_size, 1.0).width;
        text_with_config_color(
            font_cache,
            config,
            marker,
            (screen_width() - width) / 2.0,
            bottom_safe_y + 22.0 * scale_factor,
            small_size,
        );
    }

    let footer_width = measure_text(footer, Some(font), small_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        footer,
        (screen_width() - footer_width) / 2.0,
        screen_height() - 20.0 * scale_factor,
        small_size,
    );
}
