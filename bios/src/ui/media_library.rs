use crate::{
    config::Config,
    types::{AnimationState, BackgroundState, BatteryInfo},
    ui::{
        draw_configured_cursor_frame, get_current_font, measure_text, render_background,
        render_ui_overlay_without_version, text_with_color, text_with_config_color,
    },
    InputState, VideoPlayer,
};
use macroquad::prelude::*;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

const HELPER: &str = "/usr/bin/playfusion-media-library";
const PROGRESS_FILE: &str = "/run/kazeta/media-library/progress";
const GRID_COLUMNS: usize = 4;
const GRID_ROWS: usize = 2;
const ITEMS_PER_PAGE: usize = GRID_COLUMNS * GRID_ROWS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaLibraryKind {
    Movies,
    Music,
}

impl MediaLibraryKind {
    fn argument(self) -> &'static str {
        match self {
            Self::Movies => "movies",
            Self::Music => "music",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Movies => "MOVIES",
            Self::Music => "MUSIC LIBRARY",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub path: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub installed: bool,
}

pub enum MediaLibraryEvent {
    None,
    Move,
    Select,
    Reject,
    Back,
    Launch(PathBuf),
}

pub struct MediaLibraryState {
    pub items: Vec<MediaItem>,
    pub selection: usize,
    pub cover_cache: HashMap<String, Texture2D>,
    cover_queue: Vec<(String, PathBuf)>,
    loaded_kind: Option<MediaLibraryKind>,
    last_refresh: f64,
    install_process: Option<Child>,
    busy_message: Option<String>,
    busy_progress: Option<f32>,
    notice: Option<(String, f64)>,
}

impl MediaLibraryState {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selection: 0,
            cover_cache: HashMap::new(),
            cover_queue: Vec::new(),
            loaded_kind: None,
            last_refresh: -10.0,
            install_process: None,
            busy_message: None,
            busy_progress: None,
            notice: None,
        }
    }

    pub fn ensure_loaded(&mut self, kind: MediaLibraryKind) {
        self.poll_install(kind);
        if self.loaded_kind != Some(kind)
            || (self.install_process.is_none() && get_time() - self.last_refresh >= 5.0)
        {
            self.refresh(kind);
        }
    }

    pub fn refresh(&mut self, kind: MediaLibraryKind) {
        let selected_id = self
            .items
            .get(self.selection)
            .map(|item| item.id.clone());
        let output = Command::new("sudo")
            .args(["-n", HELPER, "list", kind.argument()])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                match serde_json::from_slice::<Vec<MediaItem>>(&output.stdout) {
                    Ok(items) => {
                        self.items = items;
                        self.cover_cache.clear();
                        self.cover_queue = self
                            .items
                            .iter()
                            .filter(|item| !item.cover.is_empty())
                            .map(|item| (item.id.clone(), PathBuf::from(&item.cover)))
                            .collect();
                        self.selection = selected_id
                            .as_ref()
                            .and_then(|id| self.items.iter().position(|item| &item.id == id))
                            .unwrap_or(self.selection)
                            .min(self.items.len().saturating_sub(1));
                    }
                    Err(error) => {
                        self.notice = Some((format!("MEDIA CATALOG ERROR: {error}"), get_time()));
                    }
                }
            }
            Ok(output) => {
                let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
                self.notice = Some((
                    if error.is_empty() {
                        "MEDIA SCAN FAILED".to_string()
                    } else {
                        error
                    },
                    get_time(),
                ));
            }
            Err(error) => {
                self.notice = Some((format!("MEDIA SCAN ERROR: {error}"), get_time()));
            }
        }
        self.loaded_kind = Some(kind);
        self.last_refresh = get_time();
    }

    pub async fn load_next_cover(&mut self) {
        if let Some((id, path)) = self.cover_queue.pop() {
            if let Ok(texture) = load_common_image_texture(&path) {
                texture.set_filter(FilterMode::Linear);
                self.cover_cache.insert(id, texture);
            }
        }
    }

    fn poll_install(&mut self, kind: MediaLibraryKind) {
        if self.install_process.is_none() {
            return;
        }
        if let Ok(raw) = fs::read_to_string(PROGRESS_FILE) {
            let mut fields = raw.trim().splitn(2, '\t');
            self.busy_progress = fields
                .next()
                .and_then(|value| value.parse::<f32>().ok())
                .map(|value| (value / 100.0).clamp(0.0, 1.0));
            if let Some(phase) = fields.next() {
                self.busy_message = Some(phase.to_string());
            }
        }

        let finished = self
            .install_process
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten());
        let Some(status) = finished else {
            return;
        };
        self.install_process = None;
        self.busy_message = None;
        self.busy_progress = None;
        let _ = fs::remove_file(PROGRESS_FILE);
        if status.success() {
            self.notice = Some(("MEDIA INSTALLED".to_string(), get_time()));
            self.refresh(kind);
        } else {
            self.notice = Some(("MEDIA INSTALL FAILED".to_string(), get_time()));
        }
    }

    fn start_install(&mut self, kind: MediaLibraryKind, item: &MediaItem) -> bool {
        if item.installed {
            self.notice = Some(("ALREADY INSTALLED".to_string(), get_time()));
            return false;
        }
        self.start_install_path(kind, Path::new(&item.path))
    }

    pub fn start_install_path(&mut self, kind: MediaLibraryKind, path: &Path) -> bool {
        let _ = fs::remove_file(PROGRESS_FILE);
        match Command::new("sudo")
            .args(["-n", HELPER, "install", kind.argument()])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.install_process = Some(child);
                self.busy_message = Some("INSTALLING".to_string());
                self.busy_progress = Some(0.0);
                true
            }
            Err(error) => {
                self.notice = Some((format!("INSTALL ERROR: {error}"), get_time()));
                false
            }
        }
    }

    pub fn handle_input(
        &mut self,
        kind: MediaLibraryKind,
        input: &InputState,
    ) -> MediaLibraryEvent {
        self.poll_install(kind);
        if self.install_process.is_some() {
            return MediaLibraryEvent::None;
        }
        if input.back {
            return MediaLibraryEvent::Back;
        }
        if self.items.is_empty() {
            return MediaLibraryEvent::None;
        }

        let page_start = (self.selection / ITEMS_PER_PAGE) * ITEMS_PER_PAGE;
        let page_end = (page_start + ITEMS_PER_PAGE).min(self.items.len());
        if input.left {
            self.selection = if self.selection == page_start {
                page_end.saturating_sub(1)
            } else {
                self.selection - 1
            };
            return MediaLibraryEvent::Move;
        }
        if input.right {
            self.selection = if self.selection + 1 >= page_end {
                page_start
            } else {
                self.selection + 1
            };
            return MediaLibraryEvent::Move;
        }
        if input.up {
            self.selection = self.selection.saturating_sub(GRID_COLUMNS);
            return MediaLibraryEvent::Move;
        }
        if input.down {
            self.selection =
                (self.selection + GRID_COLUMNS).min(self.items.len().saturating_sub(1));
            return MediaLibraryEvent::Move;
        }
        if input.prev && self.selection >= ITEMS_PER_PAGE {
            self.selection = self.selection.saturating_sub(ITEMS_PER_PAGE);
            return MediaLibraryEvent::Move;
        }
        if input.next && page_end < self.items.len() {
            self.selection = (self.selection + ITEMS_PER_PAGE)
                .min(self.items.len().saturating_sub(1));
            return MediaLibraryEvent::Move;
        }
        if input.secondary {
            let item = self.items[self.selection].clone();
            return if self.start_install(kind, &item) {
                MediaLibraryEvent::Select
            } else {
                MediaLibraryEvent::Reject
            };
        }
        if input.select {
            return MediaLibraryEvent::Launch(PathBuf::from(
                &self.items[self.selection].path,
            ));
        }
        MediaLibraryEvent::None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        kind: MediaLibraryKind,
        animation_state: &AnimationState,
        placeholder: &Texture2D,
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
        let title_size = (16.0 * scale_factor) as u16;
        let body_size = (10.0 * scale_factor).max(9.0) as u16;
        let small_size = (8.0 * scale_factor).max(8.0) as u16;
        let title = format!(
            "{}  |  {} ITEM(S)",
            kind.title(),
            self.items.len()
        );
        let title_dims = measure_text(&title, Some(font), title_size, 1.0);
        text_with_config_color(
            font_cache,
            config,
            &title,
            (screen_width() - title_dims.width) * 0.5,
            64.0 * scale_factor,
            title_size,
        );

        if self.items.is_empty() {
            let empty = match kind {
                MediaLibraryKind::Movies => {
                    "NO MOVIES FOUND - INSERT AN SD CARD OR ADD FILES TO /var/kazeta/movies"
                }
                MediaLibraryKind::Music => {
                    "NO MUSIC FOUND - INSERT AN SD CARD OR ADD FILES TO /var/kazeta/music"
                }
            };
            let dims = measure_text(empty, Some(font), body_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                empty,
                (screen_width() - dims.width) * 0.5,
                screen_height() * 0.5,
                body_size,
            );
        } else {
            let page = self.selection / ITEMS_PER_PAGE;
            let first = page * ITEMS_PER_PAGE;
            let last = (first + ITEMS_PER_PAGE).min(self.items.len());
            let margin_x = 38.0 * scale_factor;
            let top_y = 84.0 * scale_factor;
            let bottom_safe = screen_height() - 47.0 * scale_factor;
            let available_width = screen_width() - margin_x * 2.0;
            let cell_width = available_width / GRID_COLUMNS as f32;
            let cell_height = (bottom_safe - top_y) / GRID_ROWS as f32;

            for item_index in first..last {
                let item = &self.items[item_index];
                let visible = item_index - first;
                let column = visible % GRID_COLUMNS;
                let row = visible / GRID_COLUMNS;
                let cell_x = margin_x + column as f32 * cell_width;
                let cell_y = top_y + row as f32 * cell_height;
                let selected = item_index == self.selection;

                let cover_height = (cell_height - 34.0 * scale_factor).max(80.0);
                let cover_width = match kind {
                    MediaLibraryKind::Movies => cover_height * 0.67,
                    MediaLibraryKind::Music => cover_height.min(cell_width - 28.0 * scale_factor),
                };
                let cover_x = cell_x + (cell_width - cover_width) * 0.5;
                let cover_y = cell_y;
                if selected {
                    draw_configured_cursor_frame(
                        config,
                        animation_state,
                        cover_x - 5.0 * scale_factor,
                        cover_y - 5.0 * scale_factor,
                        cover_width + 10.0 * scale_factor,
                        cover_height + 10.0 * scale_factor,
                        6.0 * scale_factor,
                    );
                }
                let texture = self.cover_cache.get(&item.id).unwrap_or(placeholder);
                draw_texture_ex(
                    texture,
                    cover_x,
                    cover_y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(cover_width, cover_height)),
                        ..Default::default()
                    },
                );

                let mut label = item.title.clone();
                if label.chars().count() > 27 {
                    label = label.chars().take(24).collect::<String>() + "...";
                }
                let label_dims = measure_text(&label, Some(font), body_size, 1.0);
                if selected {
                    text_with_color(
                        font_cache,
                        config,
                        &label,
                        cell_x + (cell_width - label_dims.width) * 0.5,
                        cover_y + cover_height + 14.0 * scale_factor,
                        body_size,
                        animation_state.get_cursor_color(config),
                    );
                } else {
                    text_with_config_color(
                        font_cache,
                        config,
                        &label,
                        cell_x + (cell_width - label_dims.width) * 0.5,
                        cover_y + cover_height + 14.0 * scale_factor,
                        body_size,
                    );
                }

                let subtitle = match kind {
                    MediaLibraryKind::Movies => {
                        if item.year.is_empty() {
                            format!("{}  |  {}", source_label(item), format_size(item.size))
                        } else {
                            format!(
                                "{}  |  {}  |  {}",
                                item.year,
                                source_label(item),
                                format_size(item.size)
                            )
                        }
                    }
                    MediaLibraryKind::Music => format!(
                        "{}  |  {}",
                        if item.artist.is_empty() {
                            "UNKNOWN ARTIST"
                        } else {
                            &item.artist
                        },
                        source_label(item)
                    ),
                };
                let mut subtitle = subtitle;
                if subtitle.chars().count() > 34 {
                    subtitle = subtitle.chars().take(31).collect::<String>() + "...";
                }
                let subtitle_dims = measure_text(&subtitle, Some(font), small_size, 1.0);
                text_with_config_color(
                    font_cache,
                    config,
                    &subtitle,
                    cell_x + (cell_width - subtitle_dims.width) * 0.5,
                    cover_y + cover_height + 27.0 * scale_factor,
                    small_size,
                );
            }
        }

        if let Some(message) = self
            .busy_message
            .as_ref()
            .or_else(|| self.notice.as_ref().and_then(|(message, started)| {
                if get_time() - *started < 4.0 {
                    Some(message)
                } else {
                    None
                }
            }))
        {
            let overlay_width = 300.0 * scale_factor;
            let overlay_height = 62.0 * scale_factor;
            let overlay_x = (screen_width() - overlay_width) * 0.5;
            let overlay_y = (screen_height() - overlay_height) * 0.5;
            draw_rectangle(
                overlay_x,
                overlay_y,
                overlay_width,
                overlay_height,
                Color::new(0.02, 0.01, 0.08, 0.94),
            );
            let dims = measure_text(message, Some(font), body_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                message,
                overlay_x + (overlay_width - dims.width) * 0.5,
                overlay_y + 25.0 * scale_factor,
                body_size,
            );
            if let Some(progress) = self.busy_progress {
                let bar_x = overlay_x + 18.0 * scale_factor;
                let bar_y = overlay_y + 40.0 * scale_factor;
                let bar_width = overlay_width - 36.0 * scale_factor;
                draw_rectangle(
                    bar_x,
                    bar_y,
                    bar_width,
                    6.0 * scale_factor,
                    Color::new(0.12, 0.10, 0.22, 1.0),
                );
                draw_rectangle(
                    bar_x,
                    bar_y,
                    bar_width * progress,
                    6.0 * scale_factor,
                    Color::new(1.0, 0.15, 0.78, 1.0),
                );
            }
        }

        let help = "[SOUTH] PLAY  |  [WEST] INSTALL FROM SD  |  [LB/RB] PAGE  |  [EAST] BACK";
        let help_dims = measure_text(help, Some(font), small_size, 1.0);
        text_with_config_color(
            font_cache,
            config,
            help,
            (screen_width() - help_dims.width) * 0.5,
            screen_height() - 14.0 * scale_factor,
            small_size,
        );
    }
}

fn source_label(item: &MediaItem) -> &'static str {
    if item.installed || item.source == "internal" {
        "INTERNAL"
    } else {
        "SD/USB"
    }
}

fn format_size(bytes: u64) -> String {
    let gib = bytes as f64 / 1_073_741_824.0;
    if gib >= 1.0 {
        format!("{gib:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1_048_576.0)
    }
}

fn load_common_image_texture(path: &Path) -> Result<Texture2D, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let width = u16::try_from(decoded.width()).map_err(|_| "image is too wide".to_string())?;
    let height = u16::try_from(decoded.height()).map_err(|_| "image is too tall".to_string())?;
    Ok(Texture2D::from_rgba8(width, height, decoded.as_raw()))
}
