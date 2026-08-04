use crate::{
    config::Config,
    types::{BackgroundState, BatteryInfo},
    ui::{
        draw_configured_cursor_frame, get_current_font, measure_text, render_background,
        text_with_color, text_with_config_color,
    },
    AnimationState, InputState, VideoPlayer,
};
use macroquad::prelude::*;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const MUSIC_ROOT: &str = "/var/kazeta/music";
const VISIBLE_ROWS: usize = 7;
pub const JUKEBOX_CABINET_BYTES: &[u8] =
    include_bytes!("../../assets/playfusion-jukebox-cabinet.png");

#[derive(Clone, Copy, PartialEq)]
pub enum JukeboxVisualMode {
    Cabinet,
    Fullscreen,
}

#[derive(Clone)]
enum JukeboxAction {
    Play { target: PathBuf, shuffle: bool },
    OpenAlbum { path: PathBuf, name: String },
    ImportMusic,
    DeleteIncludedMusic,
}

#[derive(Clone)]
struct JukeboxItem {
    label: String,
    action: JukeboxAction,
}

#[derive(Clone)]
enum BrowserView {
    Library,
    Album { path: PathBuf, name: String },
}

pub enum JukeboxEvent {
    None,
    Move,
    Select,
    BackToExtras,
    Launch {
        target: PathBuf,
        shuffle: bool,
        fullscreen: bool,
    },
}

pub struct JukeboxBrowserState {
    view: BrowserView,
    items: Vec<JukeboxItem>,
    selection: usize,
    loaded: bool,
    last_refresh: f64,
    cabinet_texture: Texture2D,
    visual_mode: JukeboxVisualMode,
    confirm_delete_included: bool,
    status_message: Option<(String, f64)>,
}

impl JukeboxBrowserState {
    pub fn new() -> Self {
        Self {
            view: BrowserView::Library,
            items: Vec::new(),
            selection: 0,
            loaded: false,
            last_refresh: -10.0,
            cabinet_texture: Texture2D::from_file_with_format(JUKEBOX_CABINET_BYTES, None),
            visual_mode: JukeboxVisualMode::Cabinet,
            confirm_delete_included: false,
            status_message: None,
        }
    }

    pub fn ensure_loaded(&mut self) {
        if !self.loaded || get_time() - self.last_refresh >= 5.0 {
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        let selection = self.selection;
        match self.view.clone() {
            BrowserView::Library => self.load_library(),
            BrowserView::Album { path, name } => self.load_album(path, name),
        }
        self.selection = selection.min(self.items.len().saturating_sub(1));
    }

    fn load_library(&mut self) {
        let root = PathBuf::from(MUSIC_ROOT);
        let all_tracks = music_files_recursive(&root);
        self.items.clear();
        if !all_tracks.is_empty() {
            self.items.push(JukeboxItem {
                label: "PLAY ALL MUSIC - SHUFFLE".to_string(),
                action: JukeboxAction::Play {
                    target: root.clone(),
                    shuffle: true,
                },
            });
            self.items.push(JukeboxItem {
                label: "PLAY ALL MUSIC - IN ORDER".to_string(),
                action: JukeboxAction::Play {
                    target: root.clone(),
                    shuffle: false,
                },
            });
        }

        let mut albums = fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let path = entry.path();
                if music_files_recursive(&path).is_empty() {
                    return None;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                Some((name, path))
            })
            .collect::<Vec<_>>();
        albums.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
        for (name, path) in albums {
            self.items.push(JukeboxItem {
                label: format!("ALBUM: {name}"),
                action: JukeboxAction::OpenAlbum { path, name },
            });
        }

        self.items.push(JukeboxItem {
            label: "IMPORT MUSIC FROM USB / SD".to_string(),
            action: JukeboxAction::ImportMusic,
        });
        if Path::new(MUSIC_ROOT).join("30 years").is_dir() {
            self.items.push(JukeboxItem {
                label: "DELETE INCLUDED '30 YEARS' MUSIC".to_string(),
                action: JukeboxAction::DeleteIncludedMusic,
            });
        }

        self.view = BrowserView::Library;
        self.selection = self.selection.min(self.items.len().saturating_sub(1));
        self.loaded = true;
        self.last_refresh = get_time();
    }

    fn load_album(&mut self, path: PathBuf, name: String) {
        let tracks = music_files_recursive(&path);
        self.items.clear();
        if !tracks.is_empty() {
            self.items.push(JukeboxItem {
                label: "PLAY ALBUM - IN ORDER".to_string(),
                action: JukeboxAction::Play {
                    target: path.clone(),
                    shuffle: false,
                },
            });
            self.items.push(JukeboxItem {
                label: "PLAY ALBUM - SHUFFLE".to_string(),
                action: JukeboxAction::Play {
                    target: path.clone(),
                    shuffle: true,
                },
            });
        }
        for track in tracks {
            let label = track
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("UNKNOWN TRACK")
                .to_string();
            self.items.push(JukeboxItem {
                label,
                action: JukeboxAction::Play {
                    target: track,
                    shuffle: false,
                },
            });
        }
        self.view = BrowserView::Album { path, name };
        self.selection = 0;
        self.loaded = true;
        self.last_refresh = get_time();
    }

    pub fn handle_input(&mut self, input: &InputState) -> JukeboxEvent {
        if input.secondary {
            self.visual_mode = if self.visual_mode == JukeboxVisualMode::Cabinet {
                JukeboxVisualMode::Fullscreen
            } else {
                JukeboxVisualMode::Cabinet
            };
            return JukeboxEvent::Select;
        }
        if input.back {
            if self.confirm_delete_included {
                self.confirm_delete_included = false;
                self.status_message = Some(("DELETE CANCELLED".to_string(), get_time()));
                return JukeboxEvent::Select;
            }
            match self.view.clone() {
                BrowserView::Library => return JukeboxEvent::BackToExtras,
                BrowserView::Album { .. } => {
                    self.selection = 0;
                    self.load_library();
                    return JukeboxEvent::Select;
                }
            }
        }
        if self.items.is_empty() {
            return JukeboxEvent::None;
        }
        if input.up {
            self.confirm_delete_included = false;
            self.selection = if self.selection == 0 {
                self.items.len() - 1
            } else {
                self.selection - 1
            };
            return JukeboxEvent::Move;
        }
        if input.down {
            self.confirm_delete_included = false;
            self.selection = (self.selection + 1) % self.items.len();
            return JukeboxEvent::Move;
        }
        if input.select {
            match self.items[self.selection].action.clone() {
                JukeboxAction::OpenAlbum { path, name } => {
                    self.load_album(path, name);
                    return JukeboxEvent::Select;
                }
                JukeboxAction::Play { target, shuffle } => {
                    return JukeboxEvent::Launch {
                        target,
                        shuffle,
                        fullscreen: self.visual_mode == JukeboxVisualMode::Fullscreen,
                    };
                }
                JukeboxAction::ImportMusic => {
                    let imported = import_music_from_removable_media();
                    self.status_message = Some((
                        if imported > 0 {
                            format!("IMPORTED {imported} MUSIC FILE(S)")
                        } else {
                            "NO MUSIC FOUND IN A /MUSIC FOLDER".to_string()
                        },
                        get_time(),
                    ));
                    self.refresh();
                    return JukeboxEvent::Select;
                }
                JukeboxAction::DeleteIncludedMusic => {
                    if !self.confirm_delete_included {
                        self.confirm_delete_included = true;
                        self.status_message = Some((
                            "PRESS [A] AGAIN TO DELETE INCLUDED MUSIC; [B] CANCELS".to_string(),
                            get_time(),
                        ));
                    } else {
                        self.confirm_delete_included = false;
                        let target = Path::new(MUSIC_ROOT).join("30 years");
                        self.status_message = Some((
                            if fs::remove_dir_all(&target).is_ok() {
                                "INCLUDED MUSIC DELETED".to_string()
                            } else {
                                "COULD NOT DELETE INCLUDED MUSIC".to_string()
                            },
                            get_time(),
                        ));
                        self.refresh();
                    }
                    return JukeboxEvent::Select;
                }
            }
        }
        JukeboxEvent::None
    }

    pub fn draw(
        &self,
        _logo_cache: &HashMap<String, Texture2D>,
        background_cache: &HashMap<String, Texture2D>,
        video_cache: &mut HashMap<String, VideoPlayer>,
        font_cache: &HashMap<String, Font>,
        config: &Config,
        animation_state: &AnimationState,
        background_state: &mut BackgroundState,
        _battery_info: &Option<BatteryInfo>,
        _current_time_str: &str,
        _gcc_adapter_poll_rate: &Option<u32>,
        scale_factor: f32,
    ) {
        render_background(background_cache, video_cache, config, background_state);
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.01, 0.0, 0.08, 0.72),
        );
        draw_texture_ex(
            &self.cabinet_texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                ..Default::default()
            },
        );

        let font = get_current_font(font_cache, config);
        let row_size = (9.5 * scale_factor).max(9.0) as u16;
        let subtitle = match &self.view {
            BrowserView::Library => "SELECT MUSIC".to_string(),
            BrowserView::Album { name, .. } => format!("ALBUM: {name}"),
        };
        let subtitle_size = (10.0 * scale_factor).max(9.0) as u16;
        let subtitle_dims = measure_text(&subtitle, Some(font), subtitle_size, 1.0);
        text_with_color(
            font_cache,
            config,
            &subtitle,
            (screen_width() - subtitle_dims.width) * 0.5,
            88.0 * scale_factor,
            subtitle_size,
            Color::new(1.0, 0.18, 0.78, 1.0),
        );

        if self.items.is_empty() {
            let empty = "NO MUSIC FOUND";
            let dims = measure_text(empty, Some(font), row_size, 1.0);
            text_with_config_color(
                font_cache,
                config,
                empty,
                (screen_width() - dims.width) * 0.5,
                175.0 * scale_factor,
                row_size,
            );
        } else {
            let maximum_start = self.items.len().saturating_sub(VISIBLE_ROWS);
            let start = self
                .selection
                .saturating_sub(VISIBLE_ROWS / 2)
                .min(maximum_start);
            let end = (start + VISIBLE_ROWS).min(self.items.len());
            for (visible_index, item_index) in (start..end).enumerate() {
                let item = &self.items[item_index];
                let y = (111.0 + visible_index as f32 * 21.0) * scale_factor;
                let selected = item_index == self.selection;
                let mut label = item.label.clone();
                if label.chars().count() > 42 {
                    label = label.chars().take(39).collect::<String>() + "...";
                }
                let row_x = 158.0 * scale_factor;
                let row_w = 324.0 * scale_factor;
                let row_h = 17.0 * scale_factor;
                if visible_index % 2 == 0 {
                    draw_rectangle(
                        row_x,
                        y - 13.0 * scale_factor,
                        row_w,
                        row_h,
                        Color::new(0.08, 0.02, 0.18, 0.55),
                    );
                }
                if selected {
                    draw_configured_cursor_frame(
                        config,
                        animation_state,
                        row_x - 2.0 * scale_factor,
                        y - 14.0 * scale_factor,
                        row_w + 4.0 * scale_factor,
                        row_h + 1.0 * scale_factor,
                        1.5 * scale_factor,
                    );
                    text_with_color(
                        font_cache,
                        config,
                        &label,
                        164.0 * scale_factor,
                        y,
                        row_size,
                        Color::new(0.96, 0.96, 1.0, 1.0),
                    );
                } else {
                    text_with_config_color(
                        font_cache,
                        config,
                        &label,
                        164.0 * scale_factor,
                        y,
                        row_size,
                    );
                }
            }
        }

        let mode = if self.visual_mode == JukeboxVisualMode::Cabinet {
            "CABINET VISUALS"
        } else {
            "FULLSCREEN VISUALS"
        };
        let mode_size = (8.0 * scale_factor).max(8.0) as u16;
        let mode_dims = measure_text(mode, Some(font), mode_size, 1.0);
        text_with_color(
            font_cache,
            config,
            mode,
            (screen_width() - mode_dims.width) * 0.5,
            271.0 * scale_factor,
            mode_size,
            Color::new(0.08, 0.9, 1.0, 1.0),
        );
        let help = "[A] PLAY  [X] VISUAL MODE  [B] BACK";
        let help_dims = measure_text(help, Some(font), mode_size, 1.0);
        text_with_color(
            font_cache,
            config,
            help,
            (screen_width() - help_dims.width) * 0.5,
            287.0 * scale_factor,
            mode_size,
            Color::new(0.95, 0.95, 1.0, 1.0),
        );

        if let Some((message, shown_at)) = self.status_message.as_ref() {
            if self.confirm_delete_included || get_time() - *shown_at < 6.0 {
                let status_size = (7.0 * scale_factor).max(7.0) as u16;
                let status_dims = measure_text(message, Some(font), status_size, 1.0);
                text_with_color(
                    font_cache,
                    config,
                    message,
                    (screen_width() - status_dims.width) * 0.5,
                    301.0 * scale_factor,
                    status_size,
                    Color::new(1.0, 0.24, 0.78, 1.0),
                );
            }
        }
    }
}

fn import_music_from_removable_media() -> usize {
    let destination = Path::new(MUSIC_ROOT);
    let mut imported = 0usize;
    let roots = [Path::new("/run/media/gamer"), Path::new("/run/media"), Path::new("/media")];
    for root in roots {
        let Ok(mounts) = fs::read_dir(root) else {
            continue;
        };
        for mount in mounts.flatten() {
            let mount_path = mount.path();
            if !mount_path.is_dir() {
                continue;
            }
            for folder_name in ["Music", "music", "MUSIC"] {
                let source = mount_path.join(folder_name);
                if source.is_dir() {
                    imported += copy_music_tree(&source, destination, &source);
                }
            }
        }
    }
    imported
}

fn copy_music_tree(source: &Path, destination: &Path, source_root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(source) else {
        return 0;
    };
    let mut copied = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            copied += copy_music_tree(&path, destination, source_root);
        } else if is_music_file(&path) {
            let relative = path.strip_prefix(source_root).unwrap_or_else(|_| Path::new("track"));
            let target = destination.join(relative);
            if let Some(parent) = target.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if !target.exists() && fs::copy(&path, &target).is_ok() {
                copied += 1;
            }
        }
    }
    copied
}

fn is_music_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "mp3" | "flac" | "ogg" | "opus" | "m4a" | "wav"
    )
}

fn music_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut tracks = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return tracks;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            tracks.extend(music_files_recursive(&path));
        } else if is_music_file(&path) {
            tracks.push(path);
        }
    }
    tracks.sort_by(|left, right| {
        left.to_string_lossy()
            .to_lowercase()
            .cmp(&right.to_string_lossy().to_lowercase())
    });
    tracks
}
