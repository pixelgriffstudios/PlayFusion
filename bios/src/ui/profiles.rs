use crate::{
    config::Config,
    get_current_font, render_background, render_ui_overlay_without_version, text_with_config_color,
    types::{AnimationState, BackgroundState, BatteryInfo},
    ui::{draw_configured_cursor_frame, draw_playfusion_panel_frame},
    InputState, VideoPlayer,
};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_PROFILES: usize = 4;
const PROFILE_IDS: [&str; 4] = ["default", "profile-1", "profile-2", "profile-3"];
const KEYBOARD_COLUMNS: usize = 10;
const KEYBOARD_KEYS: [&str; 39] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "SPACE",
    "BACK", "DONE",
];

const AVATAR_BYTES: [&[u8]; 40] = [
    include_bytes!("../../profile-avatars/avatar-01.png"),
    include_bytes!("../../profile-avatars/avatar-02.png"),
    include_bytes!("../../profile-avatars/avatar-03.png"),
    include_bytes!("../../profile-avatars/avatar-04.png"),
    include_bytes!("../../profile-avatars/avatar-05.png"),
    include_bytes!("../../profile-avatars/avatar-06.png"),
    include_bytes!("../../profile-avatars/avatar-07.png"),
    include_bytes!("../../profile-avatars/avatar-08.png"),
    include_bytes!("../../profile-avatars/avatar-09.png"),
    include_bytes!("../../profile-avatars/avatar-10.png"),
    include_bytes!("../../profile-avatars/avatar-11.png"),
    include_bytes!("../../profile-avatars/avatar-12.png"),
    include_bytes!("../../profile-avatars/avatar-13.png"),
    include_bytes!("../../profile-avatars/avatar-14.png"),
    include_bytes!("../../profile-avatars/avatar-15.png"),
    include_bytes!("../../profile-avatars/avatar-16.png"),
    include_bytes!("../../profile-avatars/avatar-17.png"),
    include_bytes!("../../profile-avatars/avatar-18.png"),
    include_bytes!("../../profile-avatars/avatar-19.png"),
    include_bytes!("../../profile-avatars/avatar-20.png"),
    include_bytes!("../../profile-avatars/avatar-21.png"),
    include_bytes!("../../profile-avatars/avatar-22.png"),
    include_bytes!("../../profile-avatars/avatar-23.png"),
    include_bytes!("../../profile-avatars/avatar-24.png"),
    include_bytes!("../../profile-avatars/avatar-25.png"),
    include_bytes!("../../profile-avatars/avatar-26.png"),
    include_bytes!("../../profile-avatars/avatar-27.png"),
    include_bytes!("../../profile-avatars/avatar-28.png"),
    include_bytes!("../../profile-avatars/avatar-29.png"),
    include_bytes!("../../profile-avatars/avatar-30.png"),
    include_bytes!("../../profile-avatars/avatar-31.png"),
    include_bytes!("../../profile-avatars/avatar-32.png"),
    include_bytes!("../../profile-avatars/avatar-33.png"),
    include_bytes!("../../profile-avatars/avatar-34.png"),
    include_bytes!("../../profile-avatars/avatar-35.png"),
    include_bytes!("../../profile-avatars/avatar-36.png"),
    include_bytes!("../../profile-avatars/avatar-37.png"),
    include_bytes!("../../profile-avatars/avatar-38.png"),
    include_bytes!("../../profile-avatars/avatar-39.png"),
    include_bytes!("../../profile-avatars/avatar-40.png"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileRecord {
    pub id: String,
    pub name: String,
    pub avatar: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProfileScreenMode {
    Boot,
    Manage,
    Rename,
    ConfirmDelete,
    Message(String),
}

pub enum ProfileEvent {
    None,
    Move,
    Select,
    Reject,
    BootComplete,
    Back,
}

pub struct ProfilesState {
    pub profiles: Vec<ProfileRecord>,
    pub selection: usize,
    pub mode: ProfileScreenMode,
    pub active_id: String,
    pub rename_text: String,
    pub keyboard_selection: usize,
    avatars: Vec<Texture2D>,
}

impl ProfilesState {
    pub fn load() -> Self {
        let _ = fs::create_dir_all(profiles_root());
        let mut profiles = Vec::new();
        for (slot, id) in PROFILE_IDS.iter().enumerate() {
            let path = profile_path(id);
            let record = fs::read_to_string(&path)
                .ok()
                .and_then(|content| toml::from_str::<ProfileRecord>(&content).ok())
                .filter(|record| record.id == *id)
                .map(|mut record| {
                    record.avatar %= AVATAR_BYTES.len();
                    record.name = clean_profile_name(&record.name)
                        .unwrap_or_else(|| default_profile_name(slot));
                    record
                });
            if let Some(record) = record {
                profiles.push(record);
            } else if slot == 0 {
                let record = ProfileRecord {
                    id: "default".to_string(),
                    name: default_profile_name(0),
                    avatar: 0,
                };
                let _ = save_profile(&record);
                profiles.push(record);
            }
        }

        let mut active_id = read_active_profile();
        if !profiles.iter().any(|profile| profile.id == active_id) {
            active_id = "default".to_string();
            let _ = write_active_profile(&active_id);
        }
        let selection = profiles
            .iter()
            .position(|profile| profile.id == active_id)
            .unwrap_or(0);
        let avatars = AVATAR_BYTES
            .iter()
            .map(|bytes| Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png)))
            .collect();

        Self {
            profiles,
            selection,
            mode: ProfileScreenMode::Manage,
            active_id,
            rename_text: String::new(),
            keyboard_selection: 0,
            avatars,
        }
    }

    pub fn needs_boot_picker(&self) -> bool {
        self.profiles.len() > 1
    }

    pub fn draw_active_badge(
        &self,
        font_cache: &HashMap<String, Font>,
        config: &Config,
        animation_state: &AnimationState,
        scale_factor: f32,
    ) {
        let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| profile.id == self.active_id)
        else {
            return;
        };

        let width = 138.0 * scale_factor;
        let height = 48.0 * scale_factor;
        let margin = 12.0 * scale_factor;
        let x = if config.profile_badge_position.eq_ignore_ascii_case("LEFT") {
            margin
        } else {
            screen_width() - width - margin
        };
        let y = margin;
        draw_rectangle(x, y, width, height, Color::new(0.01, 0.015, 0.05, 0.88));
        // The active-profile badge is part of the selected theme. Use the
        // theme's configured highlight instead of a fixed PlayFusion rainbow
        // frame (the Xbox themes use their yellow selection color here).
        draw_configured_cursor_frame(
            config,
            animation_state,
            x,
            y,
            width,
            height,
            1.5 * scale_factor,
        );

        let avatar_size = 36.0 * scale_factor;
        draw_texture_ex(
            &self.avatars[profile.avatar % self.avatars.len()],
            x + 6.0 * scale_factor,
            y + 6.0 * scale_factor,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(avatar_size, avatar_size)),
                ..Default::default()
            },
        );

        let maximum_chars = 17;
        let mut name = profile.name.chars().take(maximum_chars).collect::<String>();
        if profile.name.chars().count() > maximum_chars {
            name.push_str("...");
        }
        let text_x = x + 49.0 * scale_factor;
        text_with_config_color(
            font_cache,
            config,
            "ACTIVE PROFILE",
            text_x,
            y + 18.0 * scale_factor,
            (6.5 * scale_factor).max(7.0) as u16,
        );
        text_with_config_color(
            font_cache,
            config,
            &name.to_uppercase(),
            text_x,
            y + 34.0 * scale_factor,
            (8.5 * scale_factor).max(8.0) as u16,
        );
    }

    pub fn open_boot(&mut self) {
        self.mode = ProfileScreenMode::Boot;
        self.selection = self
            .profiles
            .iter()
            .position(|profile| profile.id == self.active_id)
            .unwrap_or(0);
    }

    pub fn open_manager(&mut self) {
        self.mode = ProfileScreenMode::Manage;
        self.selection = self
            .profiles
            .iter()
            .position(|profile| profile.id == self.active_id)
            .unwrap_or(0);
    }

    fn activate_selected(&mut self) -> bool {
        let Some(profile) = self.profiles.get(self.selection) else {
            return false;
        };
        if write_active_profile(&profile.id).is_err() {
            return false;
        }
        self.active_id = profile.id.clone();
        true
    }

    fn add_profile(&mut self) -> Result<String, String> {
        if self.profiles.len() >= MAX_PROFILES {
            return Err("FOUR PROFILE LIMIT REACHED".to_string());
        }
        let Some((slot, id)) = PROFILE_IDS
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, id)| !self.profiles.iter().any(|profile| profile.id == **id))
        else {
            return Err("NO PROFILE SLOT AVAILABLE".to_string());
        };
        let avatar_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as usize)
            .unwrap_or(slot);
        let record = ProfileRecord {
            id: id.to_string(),
            name: default_profile_name(slot),
            avatar: avatar_seed % AVATAR_BYTES.len(),
        };
        save_profile(&record).map_err(|error| format!("PROFILE SAVE FAILED: {error}"))?;
        self.profiles.push(record);
        self.profiles
            .sort_by_key(|profile| profile_slot(&profile.id));
        self.selection = self
            .profiles
            .iter()
            .position(|profile| profile.id == *id)
            .unwrap_or(0);
        Ok(format!(
            "{} CREATED",
            default_profile_name(slot).to_uppercase()
        ))
    }

    fn cycle_avatar(&mut self, direction: isize) -> bool {
        let Some(profile) = self.profiles.get_mut(self.selection) else {
            return false;
        };
        profile.avatar =
            (profile.avatar as isize + direction).rem_euclid(AVATAR_BYTES.len() as isize) as usize;
        save_profile(profile).is_ok()
    }

    fn begin_rename(&mut self) {
        if let Some(profile) = self.profiles.get(self.selection) {
            self.rename_text = profile.name.clone();
            self.keyboard_selection = 0;
            self.mode = ProfileScreenMode::Rename;
        }
    }

    fn commit_rename(&mut self) -> bool {
        let Some(name) = clean_profile_name(&self.rename_text) else {
            return false;
        };
        let Some(profile) = self.profiles.get_mut(self.selection) else {
            return false;
        };
        profile.name = name;
        save_profile(profile).is_ok()
    }

    fn delete_selected(&mut self) -> Result<String, String> {
        let Some(profile) = self.profiles.get(self.selection).cloned() else {
            return Err("PROFILE NOT FOUND".to_string());
        };
        if profile.id == "default" {
            return Err("DEFAULT PROFILE CANNOT BE DELETED".to_string());
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let save_dir = profile_save_dir(&profile.id);
        if save_dir.exists() {
            let archive_root = saves_root().join("removed-profiles");
            fs::create_dir_all(&archive_root)
                .map_err(|error| format!("SAVE ARCHIVE FAILED: {error}"))?;
            fs::rename(
                &save_dir,
                archive_root.join(format!("{}-{timestamp}", profile.id)),
            )
            .map_err(|error| format!("SAVE ARCHIVE FAILED: {error}"))?;
        }
        let path = profile_path(&profile.id);
        if path.exists() {
            fs::remove_file(path).map_err(|error| format!("PROFILE DELETE FAILED: {error}"))?;
        }
        if self.active_id == profile.id {
            write_active_profile("default")
                .map_err(|error| format!("PROFILE SWITCH FAILED: {error}"))?;
            self.active_id = "default".to_string();
        }
        self.profiles.retain(|item| item.id != profile.id);
        self.selection = self.selection.min(self.profiles.len().saturating_sub(1));
        Ok(format!(
            "{} REMOVED — SAVES ARCHIVED",
            profile.name.to_uppercase()
        ))
    }

    pub fn handle_input(&mut self, input: &InputState) -> ProfileEvent {
        match self.mode.clone() {
            ProfileScreenMode::Boot => {
                if input.up || input.left {
                    self.selection = if self.selection == 0 {
                        self.profiles.len().saturating_sub(1)
                    } else {
                        self.selection - 1
                    };
                    return ProfileEvent::Move;
                }
                if input.down || input.right {
                    self.selection = (self.selection + 1) % self.profiles.len().max(1);
                    return ProfileEvent::Move;
                }
                if input.select {
                    return if self.activate_selected() {
                        ProfileEvent::BootComplete
                    } else {
                        ProfileEvent::Reject
                    };
                }
            }
            ProfileScreenMode::Manage => {
                let item_count =
                    self.profiles.len() + usize::from(self.profiles.len() < MAX_PROFILES);
                if input.up || input.left {
                    self.selection = if self.selection == 0 {
                        item_count.saturating_sub(1)
                    } else {
                        self.selection - 1
                    };
                    return ProfileEvent::Move;
                }
                if input.down || input.right {
                    self.selection = (self.selection + 1) % item_count.max(1);
                    return ProfileEvent::Move;
                }
                if input.back {
                    return ProfileEvent::Back;
                }
                if self.selection == self.profiles.len() {
                    if input.select || input.cycle {
                        self.mode = ProfileScreenMode::Message(match self.add_profile() {
                            Ok(message) => message,
                            Err(error) => format!("ERROR: {error}"),
                        });
                        return ProfileEvent::Select;
                    }
                    return ProfileEvent::None;
                }
                if input.select {
                    self.mode = ProfileScreenMode::Message(if self.activate_selected() {
                        "ACTIVE PROFILE CHANGED".to_string()
                    } else {
                        "ERROR: PROFILE SWITCH FAILED".to_string()
                    });
                    return ProfileEvent::Select;
                }
                if input.secondary {
                    self.begin_rename();
                    return ProfileEvent::Select;
                }
                if input.prev && self.cycle_avatar(-1) {
                    return ProfileEvent::Move;
                }
                if input.next && self.cycle_avatar(1) {
                    return ProfileEvent::Move;
                }
                if input.cycle {
                    if self.profiles[self.selection].id == "default" {
                        self.mode = ProfileScreenMode::Message(
                            "DEFAULT PROFILE CANNOT BE DELETED".to_string(),
                        );
                    } else {
                        self.mode = ProfileScreenMode::ConfirmDelete;
                    }
                    return ProfileEvent::Select;
                }
            }
            ProfileScreenMode::Rename => {
                while let Some(character) = get_char_pressed() {
                    if (character.is_ascii_alphanumeric()
                        || matches!(character, ' ' | '-' | '_' | '.'))
                        && self.rename_text.chars().count() < 20
                    {
                        self.rename_text.push(character);
                    }
                }
                if is_key_pressed(KeyCode::Backspace) {
                    self.rename_text.pop();
                    return ProfileEvent::Move;
                }
                if is_key_pressed(KeyCode::Enter) {
                    return if self.commit_rename() {
                        self.mode = ProfileScreenMode::Manage;
                        ProfileEvent::Select
                    } else {
                        ProfileEvent::Reject
                    };
                }
                if input.back {
                    self.mode = ProfileScreenMode::Manage;
                    return ProfileEvent::Back;
                }
                let rows = (KEYBOARD_KEYS.len() + KEYBOARD_COLUMNS - 1) / KEYBOARD_COLUMNS;
                if input.left {
                    self.keyboard_selection = if self.keyboard_selection == 0 {
                        KEYBOARD_KEYS.len() - 1
                    } else {
                        self.keyboard_selection - 1
                    };
                    return ProfileEvent::Move;
                }
                if input.right {
                    self.keyboard_selection = (self.keyboard_selection + 1) % KEYBOARD_KEYS.len();
                    return ProfileEvent::Move;
                }
                if input.up {
                    let candidate = self.keyboard_selection.saturating_sub(KEYBOARD_COLUMNS);
                    self.keyboard_selection = if self.keyboard_selection < KEYBOARD_COLUMNS {
                        (self.keyboard_selection + (rows - 1) * KEYBOARD_COLUMNS)
                            .min(KEYBOARD_KEYS.len() - 1)
                    } else {
                        candidate
                    };
                    return ProfileEvent::Move;
                }
                if input.down {
                    let candidate = self.keyboard_selection + KEYBOARD_COLUMNS;
                    self.keyboard_selection = if candidate >= KEYBOARD_KEYS.len() {
                        self.keyboard_selection % KEYBOARD_COLUMNS
                    } else {
                        candidate
                    };
                    return ProfileEvent::Move;
                }
                if input.select {
                    match KEYBOARD_KEYS[self.keyboard_selection] {
                        "SPACE" => {
                            if self.rename_text.chars().count() < 20 {
                                self.rename_text.push(' ');
                            }
                        }
                        "BACK" => {
                            self.rename_text.pop();
                        }
                        "DONE" => {
                            if self.commit_rename() {
                                self.mode = ProfileScreenMode::Manage;
                            } else {
                                return ProfileEvent::Reject;
                            }
                        }
                        key => {
                            if self.rename_text.chars().count() < 20 {
                                self.rename_text.push_str(key);
                            }
                        }
                    }
                    return ProfileEvent::Select;
                }
            }
            ProfileScreenMode::ConfirmDelete => {
                if input.back {
                    self.mode = ProfileScreenMode::Manage;
                    return ProfileEvent::Back;
                }
                if input.select {
                    self.mode = ProfileScreenMode::Message(match self.delete_selected() {
                        Ok(message) => message,
                        Err(error) => format!("ERROR: {error}"),
                    });
                    return ProfileEvent::Select;
                }
            }
            ProfileScreenMode::Message(_) => {
                if input.select || input.back {
                    self.mode = ProfileScreenMode::Manage;
                    return ProfileEvent::Select;
                }
            }
        }
        ProfileEvent::None
    }
}

fn playfusion_data_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/home/gamer"))
        .join(".local/share/kazeta")
}

fn profiles_root() -> PathBuf {
    playfusion_data_root().join("profiles")
}

fn saves_root() -> PathBuf {
    playfusion_data_root().join("saves")
}

fn profile_path(id: &str) -> PathBuf {
    profiles_root().join(format!("{id}.toml"))
}

fn profile_save_dir(id: &str) -> PathBuf {
    if id == "default" {
        saves_root().join("default")
    } else {
        saves_root().join("profiles").join(id)
    }
}

fn profile_slot(id: &str) -> usize {
    PROFILE_IDS
        .iter()
        .position(|candidate| *candidate == id)
        .unwrap_or(99)
}

fn default_profile_name(slot: usize) -> String {
    if slot == 0 {
        "Default Profile".to_string()
    } else {
        format!("Profile {slot}")
    }
}

fn clean_profile_name(name: &str) -> Option<String> {
    let cleaned = name
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.')
        })
        .take(20)
        .collect::<String>();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    (!cleaned.is_empty()).then_some(cleaned)
}

fn save_profile(profile: &ProfileRecord) -> std::io::Result<()> {
    fs::create_dir_all(profiles_root())?;
    let content = toml::to_string_pretty(profile).map_err(std::io::Error::other)?;
    fs::write(profile_path(&profile.id), content)
}

fn active_profile_path() -> PathBuf {
    playfusion_data_root().join("active-profile")
}

fn read_active_profile() -> String {
    fs::read_to_string(active_profile_path())
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| PROFILE_IDS.contains(&value.as_str()))
        .unwrap_or_else(|| "default".to_string())
}

fn write_active_profile(id: &str) -> std::io::Result<()> {
    if !PROFILE_IDS.contains(&id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid profile ID",
        ));
    }
    fs::create_dir_all(playfusion_data_root())?;
    fs::write(active_profile_path(), format!("{id}\n"))
}

pub fn active_profile_id() -> String {
    read_active_profile()
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &ProfilesState,
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
        Color::new(0.0, 0.0, 0.02, 0.66),
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

    match state.mode {
        ProfileScreenMode::Rename => {
            draw_rename(state, animation_state, font_cache, config, scale_factor)
        }
        ProfileScreenMode::ConfirmDelete => {
            draw_cards(state, animation_state, font_cache, config, scale_factor);
            draw_dialog(
                font_cache,
                config,
                "REMOVE THIS PROFILE?",
                "SAVES WILL BE ARCHIVED  |  A YES   B NO",
                scale_factor,
            );
        }
        ProfileScreenMode::Message(ref message) => {
            draw_cards(state, animation_state, font_cache, config, scale_factor);
            draw_dialog(font_cache, config, "USER PROFILES", message, scale_factor);
        }
        _ => draw_cards(state, animation_state, font_cache, config, scale_factor),
    }
}

fn draw_cards(
    state: &ProfilesState,
    animation_state: &AnimationState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    let font = get_current_font(font_cache, config);
    let title = if state.mode == ProfileScreenMode::Boot {
        "CHOOSE PROFILE"
    } else {
        "USER PROFILES"
    };
    centered_text(
        font_cache,
        config,
        title,
        76.0 * scale_factor,
        (18.0 * scale_factor) as u16,
    );

    let show_add = state.mode != ProfileScreenMode::Boot && state.profiles.len() < MAX_PROFILES;
    let count = state.profiles.len() + usize::from(show_add);
    let gap = 10.0 * scale_factor;
    let card_width = 116.0 * scale_factor;
    let card_height = 164.0 * scale_factor;
    let total_width = count as f32 * card_width + count.saturating_sub(1) as f32 * gap;
    let start_x = (screen_width() - total_width) / 2.0;
    let y = 98.0 * scale_factor;

    for index in 0..count {
        let x = start_x + index as f32 * (card_width + gap);
        draw_rectangle(
            x,
            y,
            card_width,
            card_height,
            Color::new(0.01, 0.015, 0.05, 0.94),
        );
        draw_playfusion_panel_frame(x, y, card_width, card_height, 2.0 * scale_factor, 0.62);
        if index == state.selection {
            draw_configured_cursor_frame(
                config,
                animation_state,
                x - 4.0 * scale_factor,
                y - 4.0 * scale_factor,
                card_width + 8.0 * scale_factor,
                card_height + 8.0 * scale_factor,
                3.0 * scale_factor,
            );
        }
        if let Some(profile) = state.profiles.get(index) {
            let avatar_size = 88.0 * scale_factor;
            let avatar_x = x + (card_width - avatar_size) / 2.0;
            let avatar_y = y + 12.0 * scale_factor;
            draw_texture_ex(
                &state.avatars[profile.avatar % state.avatars.len()],
                avatar_x,
                avatar_y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(avatar_size, avatar_size)),
                    ..Default::default()
                },
            );
            let active = profile.id == state.active_id;
            if active {
                centered_text_in(
                    font_cache,
                    config,
                    "ACTIVE",
                    x,
                    card_width,
                    y + 118.0 * scale_factor,
                    (8.0 * scale_factor).max(7.0) as u16,
                );
            }
            centered_text_in(
                font_cache,
                config,
                &profile.name.to_uppercase(),
                x,
                card_width,
                y + 145.0 * scale_factor,
                (9.0 * scale_factor).max(8.0) as u16,
            );
        } else {
            centered_text_in(
                font_cache,
                config,
                "+",
                x,
                card_width,
                y + 82.0 * scale_factor,
                (42.0 * scale_factor) as u16,
            );
            centered_text_in(
                font_cache,
                config,
                "ADD PROFILE",
                x,
                card_width,
                y + 145.0 * scale_factor,
                (9.0 * scale_factor).max(8.0) as u16,
            );
        }
    }

    let controls = if state.mode == ProfileScreenMode::Boot {
        "A SELECT PROFILE"
    } else {
        "A ACTIVATE   X RENAME   LB/RB PICTURE   Y ADD/REMOVE   B BACK"
    };
    let control_size = (8.5 * scale_factor).max(8.0) as u16;
    let width = measure_text(controls, Some(font), control_size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        controls,
        (screen_width() - width) / 2.0,
        screen_height() - 10.0 * scale_factor,
        control_size,
    );
}

fn draw_rename(
    state: &ProfilesState,
    animation_state: &AnimationState,
    font_cache: &HashMap<String, Font>,
    config: &Config,
    scale_factor: f32,
) {
    centered_text(
        font_cache,
        config,
        "RENAME PROFILE",
        70.0 * scale_factor,
        (18.0 * scale_factor) as u16,
    );
    let field_width = 420.0 * scale_factor;
    let field_height = 38.0 * scale_factor;
    let field_x = (screen_width() - field_width) / 2.0;
    let field_y = 88.0 * scale_factor;
    draw_rectangle(
        field_x,
        field_y,
        field_width,
        field_height,
        Color::new(0.01, 0.015, 0.05, 0.96),
    );
    draw_playfusion_panel_frame(
        field_x,
        field_y,
        field_width,
        field_height,
        2.0 * scale_factor,
        0.72,
    );
    centered_text(
        font_cache,
        config,
        &state.rename_text.to_uppercase(),
        field_y + 25.0 * scale_factor,
        (12.0 * scale_factor) as u16,
    );

    let key_width = 46.0 * scale_factor;
    let key_height = 28.0 * scale_factor;
    let gap = 3.0 * scale_factor;
    let keyboard_width = KEYBOARD_COLUMNS as f32 * key_width + (KEYBOARD_COLUMNS - 1) as f32 * gap;
    let start_x = (screen_width() - keyboard_width) / 2.0;
    let start_y = 142.0 * scale_factor;
    for (index, key) in KEYBOARD_KEYS.iter().enumerate() {
        let column = index % KEYBOARD_COLUMNS;
        let row = index / KEYBOARD_COLUMNS;
        let x = start_x + column as f32 * (key_width + gap);
        let y = start_y + row as f32 * (key_height + gap);
        draw_rectangle(
            x,
            y,
            key_width,
            key_height,
            Color::new(0.01, 0.015, 0.05, 0.95),
        );
        draw_playfusion_panel_frame(x, y, key_width, key_height, 1.0 * scale_factor, 0.42);
        if index == state.keyboard_selection {
            draw_configured_cursor_frame(
                config,
                animation_state,
                x - 2.0 * scale_factor,
                y - 2.0 * scale_factor,
                key_width + 4.0 * scale_factor,
                key_height + 4.0 * scale_factor,
                2.0 * scale_factor,
            );
        }
        centered_text_in(
            font_cache,
            config,
            key,
            x,
            key_width,
            y + 19.0 * scale_factor,
            (8.0 * scale_factor).max(7.0) as u16,
        );
    }
    centered_text(
        font_cache,
        config,
        "A TYPE   B CANCEL   KEYBOARD: TYPE NAME + ENTER",
        screen_height() - 10.0 * scale_factor,
        (8.5 * scale_factor).max(8.0) as u16,
    );
}

fn draw_dialog(
    font_cache: &HashMap<String, Font>,
    config: &Config,
    title: &str,
    message: &str,
    scale_factor: f32,
) {
    let width = screen_width() * 0.76;
    let height = 116.0 * scale_factor;
    let x = (screen_width() - width) / 2.0;
    let y = (screen_height() - height) / 2.0;
    draw_rectangle(x, y, width, height, Color::new(0.0, 0.0, 0.02, 0.98));
    draw_playfusion_panel_frame(x, y, width, height, 3.0 * scale_factor, 0.92);
    centered_text(
        font_cache,
        config,
        title,
        y + 36.0 * scale_factor,
        (14.0 * scale_factor) as u16,
    );
    centered_text(
        font_cache,
        config,
        message,
        y + 76.0 * scale_factor,
        (9.0 * scale_factor).max(8.0) as u16,
    );
}

fn centered_text(
    font_cache: &HashMap<String, Font>,
    config: &Config,
    text: &str,
    y: f32,
    size: u16,
) {
    let font = get_current_font(font_cache, config);
    let width = measure_text(text, Some(font), size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        text,
        (screen_width() - width) / 2.0,
        y,
        size,
    );
}

fn centered_text_in(
    font_cache: &HashMap<String, Font>,
    config: &Config,
    text: &str,
    x: f32,
    width: f32,
    y: f32,
    size: u16,
) {
    let font = get_current_font(font_cache, config);
    let text_width = measure_text(text, Some(font), size, 1.0).width;
    text_with_config_color(
        font_cache,
        config,
        text,
        x + (width - text_width) / 2.0,
        y,
        size,
    );
}

#[allow(dead_code)]
fn path_is_below(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}
