use crate::{
    audio::SoundEffects, config::Config, get_current_font, render_background,
    text_with_config_color, wrap_text, BackgroundState, InputState, Screen, VideoPlayer,
    CURRENT_UPDATE_VERSION, FONT_SIZE, VERSION_NUMBER,
};
use macroquad::prelude::*;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    process::{exit, Command},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
};
use walkdir::WalkDir;

// --- State Management & Structs ---

pub enum UpdateCheckerScreenState {
    Idle,
    Checking,
    UpToDate,
    UpdateAvailable(GithubRelease),
    InProgress(String), // carries status message
    UpdateComplete,     // final screen before shutdown
    Error(String),
}

enum CheckerMessage {
    CheckComplete(Result<UpdateCheckResult, String>),
}

// A new message type for the update thread to send progress back to the UI.
enum UpdateProgressMessage {
    Status(String),
    Complete,
    Error(String),
}

enum UpdateCheckResult {
    UpToDate,
    UpdateAvailable(GithubRelease),
}

pub struct UpdateCheckerState {
    pub screen_state: UpdateCheckerScreenState,
    rx_check: Receiver<CheckerMessage>,
    rx_progress: Receiver<UpdateProgressMessage>,
    pub description_scroll_offset: usize,
    pub max_description_scroll: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GithubRelease {
    pub tag_name: String,
    pub body: String,
    pub assets: Vec<GithubAsset>,
}

// --- Implementation ---

impl UpdateCheckerState {
    pub fn new() -> Self {
        let (_tx_check, rx_check) = channel(); // Use specific names
        let (_tx_progress, rx_progress) = channel(); // Create a dummy channel for now
        Self {
            screen_state: UpdateCheckerScreenState::Idle,
            rx_check,
            rx_progress,
            description_scroll_offset: 0,
            max_description_scroll: 0,
        }
    }

    fn start_check(&mut self) {
        let (tx, rx) = channel();
        check_for_updates(tx);
        self.screen_state = UpdateCheckerScreenState::Checking;
        self.rx_check = rx; // Overwrite the old receiver
        self.description_scroll_offset = 0; // Reset scroll on new check
        self.max_description_scroll = 0;
    }
}

pub fn update(
    state: &mut UpdateCheckerState,
    input_state: &InputState,
    current_screen: &mut Screen,
    sound_effects: &SoundEffects,
    config: &Config,
) {
    if input_state.back {
        *current_screen = Screen::Extras;
        state.screen_state = UpdateCheckerScreenState::Idle; // <-- RESET STATE
        sound_effects.play_back(config);
        return;
    }

    if let Ok(msg) = state.rx_check.try_recv() {
        match msg {
            CheckerMessage::CheckComplete(Ok(result)) => match result {
                UpdateCheckResult::UpToDate => {
                    state.screen_state = UpdateCheckerScreenState::UpToDate
                }
                UpdateCheckResult::UpdateAvailable(release) => {
                    state.screen_state = UpdateCheckerScreenState::UpdateAvailable(release)
                }
            },
            CheckerMessage::CheckComplete(Err(e)) => {
                state.screen_state = UpdateCheckerScreenState::Error(e)
            }
        }
    }

    // Receive messages from the update progress thread
    if let Ok(msg) = state.rx_progress.try_recv() {
        match msg {
            UpdateProgressMessage::Status(text) => {
                state.screen_state = UpdateCheckerScreenState::InProgress(text);
            }
            UpdateProgressMessage::Complete => {
                state.screen_state = UpdateCheckerScreenState::UpdateComplete;
            }
            UpdateProgressMessage::Error(e) => {
                state.screen_state = UpdateCheckerScreenState::Error(e);
            }
        }
    }

    // If we're idle, start a check. This triggers on entering the screen.
    if let UpdateCheckerScreenState::Idle = state.screen_state {
        state.start_check();
    }

    let mut release_to_install: Option<GithubRelease> = None;
    match &state.screen_state {
        UpdateCheckerScreenState::UpdateAvailable(release) => {
            if input_state.select {
                sound_effects.play_select(config);
                release_to_install = Some(release.clone());
            }

            // Handle up/down for scrolling the description text
            if input_state.down {
                // Check against the max value calculated in the previous frame
                if state.description_scroll_offset < state.max_description_scroll {
                    state.description_scroll_offset += 1;
                    sound_effects.play_cursor_move(config);
                }
            }
            if input_state.up {
                if state.description_scroll_offset > 0 {
                    state.description_scroll_offset -= 1;
                    sound_effects.play_cursor_move(config);
                }
            }
        }
        UpdateCheckerScreenState::UpdateComplete => {
            // SOUTH button for shutdown
            if input_state.select {
                sound_effects.play_select(config);
                Command::new("sudo")
                    .arg("shutdown")
                    .arg("now")
                    .status()
                    .ok();
                exit(0); // Fallback in case shutdown command fails
            }
            // WEST button for reboot
            if input_state.secondary {
                sound_effects.play_select(config);
                Command::new("sudo").arg("reboot").status().ok();
                exit(0); // Fallback in case reboot command fails
            }
        }
        UpdateCheckerScreenState::UpToDate | UpdateCheckerScreenState::Error(_) => {
            if input_state.select {
                *current_screen = Screen::MainMenu;
                state.screen_state = UpdateCheckerScreenState::Idle; // <-- RESET STATE
                sound_effects.play_select(config);
            }
        }
        _ => {}
    }

    if let Some(release) = release_to_install {
        // Create a new channel and pass the sender to the thread
        let (tx_progress, rx_progress) = channel();
        state.rx_progress = rx_progress; // Hook up the new receiver

        // Start in the InProgress state
        state.screen_state = UpdateCheckerScreenState::InProgress("Starting update...".to_string());

        thread::spawn(move || {
            // We now check the result of the update logic.
            // If it fails, we send the error string back to the UI.
            if let Err(e) = perform_update_logic(release, tx_progress.clone()) {
                // Use unwrap_or_default() in case the UI is already closed
                tx_progress
                    .send(UpdateProgressMessage::Error(e))
                    .unwrap_or_default();
            }
        });
    }
}

pub fn draw(
    state: &mut UpdateCheckerState,
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
        UpdateCheckerScreenState::Idle => {
            let text = "Connecting to update server...";
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
        UpdateCheckerScreenState::Checking => {
            let text = "Checking for updates...";
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
        UpdateCheckerScreenState::UpToDate => {
            text_with_config_color(
                font_cache,
                config,
                "You are running the latest version.",
                text_x,
                text_y_start,
                font_size,
            );
            text_with_config_color(
                font_cache,
                config,
                &format!("Current version: {}", VERSION_NUMBER),
                text_x,
                text_y_start + line_height,
                font_size,
            );
            text_with_config_color(
                font_cache,
                config,
                "Press [SOUTH] or [EAST] to return.",
                text_x,
                text_y_start + line_height * 3.0,
                font_size,
            );
        }
        UpdateCheckerScreenState::UpdateAvailable(release) => {
            text_with_config_color(
                font_cache,
                config,
                &format!("New version available: {}", release.tag_name),
                text_x,
                text_y_start,
                font_size,
            );
            text_with_config_color(
                font_cache,
                config,
                &format!("Current version: {}", VERSION_NUMBER),
                text_x,
                text_y_start + line_height,
                font_size,
            );

            let separator_y = text_y_start + line_height * 2.5;
            draw_line(
                container_x,
                separator_y,
                container_x + container_w,
                separator_y,
                2.0,
                Color::new(1.0, 1.0, 1.0, 0.2),
            );

            // -- CHANGED -- Implemented scrolling logic
            let img_tag_regex = Regex::new(r"<img[^>]*>").unwrap();
            let md_link_regex = Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();

            let no_images = img_tag_regex.replace_all(&release.body, "");
            let clean_body = md_link_regex.replace_all(&no_images, "$1");

            let wrap_width = container_w - 60.0 * scale_factor;
            let wrapped_lines = wrap_text(clean_body.trim(), font.clone(), font_size, wrap_width);

            let description_area_top = separator_y + 30.0 * scale_factor;
            let description_area_bottom = container_y + container_h - 30.0 * scale_factor;
            let visible_lines =
                ((description_area_bottom - description_area_top) / line_height).floor() as usize;

            let max_scroll_offset = if wrapped_lines.len() > visible_lines {
                wrapped_lines.len() - visible_lines
            } else {
                0
            };

            // Clamp the scroll offset to prevent scrolling past the end
            state.description_scroll_offset =
                state.description_scroll_offset.min(max_scroll_offset);

            state.max_description_scroll = max_scroll_offset;

            // Draw the visible lines of text
            for (i, line) in wrapped_lines
                .iter()
                .skip(state.description_scroll_offset)
                .take(visible_lines)
                .enumerate()
            {
                text_with_config_color(
                    font_cache,
                    config,
                    line,
                    text_x,
                    description_area_top + (i as f32 * line_height),
                    font_size,
                );
            }

            // Draw scroll indicators if needed
            if max_scroll_offset > 0 {
                let indicator_x = container_x + container_w - 20.0 * scale_factor;
                let arrow_size = 4.0 * scale_factor;

                // Calculate the vertical center of the first and last lines
                let first_line_center_y = description_area_top + (line_height / 2.0) - 40.0;
                let last_line_center_y = description_area_bottom - (line_height / 2.0) - 40.0;

                // Up arrow - Aligned with the first line of text
                if state.description_scroll_offset > 0 {
                    draw_triangle(
                        vec2(indicator_x, first_line_center_y - arrow_size),
                        vec2(indicator_x - arrow_size, first_line_center_y + arrow_size),
                        vec2(indicator_x + arrow_size, first_line_center_y + arrow_size),
                        WHITE,
                    );
                }
                // Down arrow - Aligned with the last line of text
                if state.description_scroll_offset < max_scroll_offset {
                    draw_triangle(
                        vec2(indicator_x, last_line_center_y + arrow_size),
                        vec2(indicator_x - arrow_size, last_line_center_y - arrow_size),
                        vec2(indicator_x + arrow_size, last_line_center_y - arrow_size),
                        WHITE,
                    );
                }
            }

            let continue_text = "Press [SOUTH] to Install Update";
            let continue_dims = measure_text(continue_text, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                continue_text,
                screen_width() / 2.0 - continue_dims.width / 2.0,
                container_y + container_h - 20.0 * scale_factor,
                font_size,
            );
        }
        UpdateCheckerScreenState::InProgress(message) => {
            let text_dims = measure_text(message, Some(font), font_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                message,
                screen_width() / 2.0 - text_dims.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );
        }
        UpdateCheckerScreenState::UpdateComplete => {
            let line1 = "Update Complete!";
            let line2 = "Press [SOUTH] to shut down, or [WEST] to reboot.";

            let dims1 = measure_text(line1, Some(font), font_size, 1.0);
            let dims2 = measure_text(line2, Some(font), font_size, 1.0);

            text_with_config_color(
                font_cache,
                config,
                line1,
                screen_width() / 2.0 - dims1.width / 2.0,
                screen_height() / 2.0 - line_height,
                font_size,
            );
            text_with_config_color(
                font_cache,
                config,
                line2,
                screen_width() / 2.0 - dims2.width / 2.0,
                screen_height() / 2.0,
                font_size,
            );
        }
        UpdateCheckerScreenState::Error(msg) => {
            text_with_config_color(
                font_cache,
                config,
                "An error occurred:",
                text_x,
                text_y_start,
                font_size,
            );
            text_with_config_color(
                font_cache,
                config,
                msg,
                text_x,
                text_y_start + line_height,
                font_size,
            );
            text_with_config_color(
                font_cache,
                config,
                "Press [SOUTH] or [EAST] to return.",
                text_x,
                text_y_start + line_height * 3.0,
                font_size,
            );
        }
    }
}

// --- Background Thread Functions ---

fn check_for_updates(tx: Sender<CheckerMessage>) {
    thread::spawn(move || {
        if let Some(local_release) = find_local_update() {
            tx.send(CheckerMessage::CheckComplete(Ok(
                UpdateCheckResult::UpdateAvailable(local_release),
            )))
            .unwrap_or_default();
            return;
        }

        let client = match reqwest::blocking::Client::builder()
            .user_agent("PlayFusion-Updater/1")
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tx.send(CheckerMessage::CheckComplete(Err(e.to_string())))
                    .unwrap();
                return;
            }
        };

        let response = client
            .get("https://api.github.com/repos/pixelgriffstudios/PlayFusion/releases/latest")
            .send();

        let result = match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<GithubRelease>() {
                        Ok(latest_release) => {
                            let package_name =
                                format!("PlayFusion-update-{}.pfu", latest_release.tag_name);
                            let complete_signed_package = latest_release
                                .assets
                                .iter()
                                .any(|asset| asset.name == package_name)
                                && latest_release
                                    .assets
                                    .iter()
                                    .any(|asset| asset.name == format!("{}.sha256", package_name))
                                && latest_release
                                    .assets
                                    .iter()
                                    .any(|asset| asset.name == format!("{}.sig", package_name));
                            if is_newer_version(&latest_release.tag_name, CURRENT_UPDATE_VERSION)
                                && complete_signed_package
                            {
                                Ok(UpdateCheckResult::UpdateAvailable(latest_release))
                            } else {
                                Ok(UpdateCheckResult::UpToDate)
                            }
                        }
                        Err(e) => Err(format!("Failed to parse response: {}", e)),
                    }
                } else {
                    Err(format!("GitHub API Error: {}", resp.status()))
                }
            }
            Err(e) => Err(format!("Failed to fetch from GitHub: {}", e)),
        };
        tx.send(CheckerMessage::CheckComplete(result)).unwrap();
    });
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    fn parse(value: &str) -> Option<(u32, u32, u32)> {
        let numbers = value.trim_start_matches('v').split('.').collect::<Vec<_>>();
        if numbers.len() != 3 {
            return None;
        }
        Some((
            numbers[0].parse().ok()?,
            numbers[1].parse().ok()?,
            numbers[2].parse().ok()?,
        ))
    }
    matches!((parse(candidate), parse(current)), (Some(next), Some(now)) if next > now)
}

// This function now returns a Result, so we can catch all errors
fn perform_update_logic(
    release_info: GithubRelease,
    tx: Sender<UpdateProgressMessage>,
) -> Result<(), String> {
    let package_name = release_info
        .assets
        .iter()
        .find(|asset| asset.name.starts_with("PlayFusion-update-") && asset.name.ends_with(".pfu"))
        .map(|asset| asset.name.clone())
        .ok_or_else(|| "This release has no PlayFusion .pfu update package.".to_string())?;
    let checksum_name = format!("{}.sha256", package_name);
    let signature_name = format!("{}.sig", package_name);
    let required = [&package_name, &checksum_name, &signature_name];
    let stage_dir = Path::new("/var/tmp/playfusion-update-download");
    fs::create_dir_all(stage_dir)
        .map_err(|error| format!("Unable to create update staging area: {error}"))?;

    tx.send(UpdateProgressMessage::Status(
        "Downloading signed PlayFusion update...".to_string(),
    ))
    .map_err(|e| e.to_string())?;

    let client = reqwest::blocking::Client::builder()
        .user_agent("PlayFusion-Updater/1")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| format!("Unable to create update client: {error}"))?;
    let mut staged = Vec::new();
    for name in required {
        let asset = release_info
            .assets
            .iter()
            .find(|asset| asset.name == *name)
            .ok_or_else(|| format!("Required signed asset is missing: {name}"))?;
        let destination = stage_dir.join(name);
        if let Some(local_path) = asset.browser_download_url.strip_prefix("file://") {
            fs::copy(local_path, &destination)
                .map_err(|error| format!("Unable to stage {name}: {error}"))?;
        } else {
            let mut response = client
                .get(&asset.browser_download_url)
                .send()
                .and_then(|response| response.error_for_status())
                .map_err(|error| format!("Unable to download {name}: {error}"))?;
            let mut output = fs::File::create(&destination)
                .map_err(|error| format!("Unable to create {name}: {error}"))?;
            io::copy(&mut response, &mut output)
                .map_err(|error| format!("Unable to save {name}: {error}"))?;
        }
        staged.push(destination);
    }

    tx.send(UpdateProgressMessage::Status(
        "Verifying signature and creating rollback...".to_string(),
    ))
    .map_err(|e| e.to_string())?;
    let output = Command::new("sudo")
        .arg("/usr/bin/playfusion-update-helper")
        .arg("install")
        .args(&staged)
        .output()
        .map_err(|error| format!("Unable to start the protected updater: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if message.is_empty() {
            "Update failed safely; existing PlayFusion files were preserved.".to_string()
        } else {
            message
        });
    }
    for path in staged {
        let _ = fs::remove_file(path);
    }

    // Send "Complete" message and let the thread finish
    tx.send(UpdateProgressMessage::Complete)
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn find_local_update() -> Option<GithubRelease> {
    let roots = [
        Path::new("/var/kazeta/updates"),
        Path::new("/run/media"),
        Path::new("/media"),
    ];
    let mut candidates: Vec<PathBuf> = Vec::new();

    for root in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(6)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let is_complete_package = entry.file_type().is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("pfu")
                && file_name.starts_with("PlayFusion-update-")
                && !file_name.starts_with('.');
            let is_update_folder = root == Path::new("/var/kazeta/updates")
                || path
                    .ancestors()
                    .any(|part| part.file_name().and_then(|name| name.to_str()) == Some("updates"));

            if is_complete_package
                && is_update_folder
                && companion_path(path, ".sha256").is_ok_and(|item| item.is_file())
                && companion_path(path, ".sig").is_ok_and(|item| item.is_file())
            {
                candidates.push(path.to_path_buf());
            }
        }
    }

    candidates.sort();
    candidates.reverse();
    let path = candidates.into_iter().next()?;
    let file_name = path.file_name()?.to_string_lossy().to_string();

    Some(GithubRelease {
        tag_name: format!("LOCAL: {}", file_name),
        body: format!(
            "Local PlayFusion update found:\n{}\n\nIts SHA-256 checksum and PlayFusion signature will be verified before any system file changes.",
            path.display()
        ),
        assets: vec![
            GithubAsset {
                name: file_name.clone(),
                browser_download_url: format!("file://{}", path.display()),
            },
            GithubAsset {
                name: format!("{}.sha256", file_name),
                browser_download_url: format!("file://{}", companion_path(&path, ".sha256").ok()?.display()),
            },
            GithubAsset {
                name: format!("{}.sig", file_name),
                browser_download_url: format!("file://{}", companion_path(&path, ".sig").ok()?.display()),
            },
        ],
    })
}

fn companion_path(update: &Path, suffix: &str) -> Result<PathBuf, String> {
    let file_name = update
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Update package has an invalid filename.".to_string())?;
    Ok(update.with_file_name(format!("{}{}", file_name, suffix)))
}
