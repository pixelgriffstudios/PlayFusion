use crate::{
    config::Config,
    types::{BackgroundState, Screen},
    ui::jukebox::JUKEBOX_CABINET_BYTES,
    ui::{get_current_font, measure_text, render_background, text_with_config_color},
    InputState, VideoPlayer,
};
use macroquad::prelude::*;
use rodio::Sink;
use std::{
    collections::HashMap,
    ffi::{c_void, CString},
    fs,
    path::PathBuf,
    process::{Child, Command},
    thread,
    time::Duration,
};

extern "C" {
    fn playfusion_projectm_create(
        config_path: *const i8,
        monitor: *const i8,
        width: i32,
        height: i32,
    ) -> *mut c_void;
    fn playfusion_projectm_render(handle: *mut c_void);
    fn playfusion_projectm_texture(handle: *mut c_void) -> u32;
    fn playfusion_projectm_destroy(handle: *mut c_void);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaMode {
    Dvd,
    Jukebox,
    Movie,
    MusicFile,
}

pub struct MediaPlayerState {
    process: Option<Child>,
    mode: Option<MediaMode>,
    error: Option<String>,
    jukebox_target: Option<PathBuf>,
    jukebox_shuffle: bool,
    jukebox_fullscreen: bool,
    jukebox_cabinet: Texture2D,
    projectm_texture: Option<Texture2D>,
    projectm_native: *mut c_void,
    projectm_warmup_started: f64,
    return_screen: Screen,
}

impl MediaPlayerState {
    pub fn new() -> Self {
        Self {
            process: None,
            mode: None,
            error: None,
            jukebox_target: None,
            jukebox_shuffle: false,
            jukebox_fullscreen: false,
            jukebox_cabinet: Texture2D::from_file_with_format(JUKEBOX_CABINET_BYTES, None),
            projectm_texture: None,
            projectm_native: std::ptr::null_mut(),
            projectm_warmup_started: -10.0,
            return_screen: Screen::Extras,
        }
    }

    pub fn prepare_jukebox(&mut self, target: PathBuf, shuffle: bool, fullscreen: bool) {
        self.jukebox_target = Some(target);
        self.jukebox_shuffle = shuffle;
        self.jukebox_fullscreen = fullscreen;
        self.return_screen = Screen::Jukebox;
        self.mode = None;
        self.error = None;
    }

    pub fn prepare_file(&mut self, target: PathBuf, return_screen: Screen) {
        self.jukebox_target = Some(target);
        self.jukebox_shuffle = false;
        self.jukebox_fullscreen = true;
        self.return_screen = return_screen;
        self.mode = None;
        self.error = None;
    }

    fn start(&mut self, mode: MediaMode, current_bgm: &mut Option<Sink>, config: &Config) {
        self.mode = Some(mode);
        self.error = None;
        if let Some(sink) = current_bgm.as_ref() {
            sink.pause();
        }

        let executable = match mode {
            MediaMode::Dvd => "/usr/local/bin/super-kazeta-dvd",
            MediaMode::Jukebox => "/usr/local/bin/super-kazeta-jukebox",
            MediaMode::Movie => "/usr/local/bin/playfusion-movie",
            MediaMode::MusicFile => "/usr/local/bin/super-kazeta-jukebox",
        };
        let mut command = Command::new(executable);
        if matches!(mode, MediaMode::Jukebox | MediaMode::MusicFile) {
            if let Some(target) = self.jukebox_target.as_ref() {
                command.arg(target);
            }
            command.arg(if self.jukebox_shuffle {
                "--shuffle"
            } else {
                "--ordered"
            });
            command.arg(config.jukebox_visual_seconds.to_string());
            command.arg(if mode == MediaMode::Jukebox && !self.jukebox_fullscreen {
                "--cabinet"
            } else {
                "--fullscreen"
            });
        } else if mode == MediaMode::Movie {
            if let Some(target) = self.jukebox_target.as_ref() {
                command.arg(target);
            }
        }
        match command.spawn() {
            Ok(child) => {
                self.process = Some(child);
                if mode == MediaMode::Jukebox && !self.jukebox_fullscreen {
                    self.start_native_projectm();
                }
            }
            Err(error) => {
                self.error = Some(format!("FAILED TO START MEDIA PLAYER: {error}"));
                self.process = None;
            }
        }
    }

    fn resume_menu_music(&self, current_bgm: &mut Option<Sink>, config: &Config) {
        if let Some(sink) = current_bgm.as_ref() {
            sink.set_volume(config.bgm_volume);
            sink.play();
        }
    }

    fn stop_process(&mut self) {
        if let Some(mut process) = self.process.take() {
            // The jukebox launcher owns both MPV and projectM. Kill its direct
            // children first so a forced return cannot leave either one running.
            let pid = process.id().to_string();
            let _ = Command::new("/usr/bin/pkill")
                .args(["-KILL", "-P", &pid])
                .status();
            let _ = process.kill();
            let _ = process.wait();
        }
        self.stop_native_projectm();
    }

    fn start_native_projectm(&mut self) {
        self.stop_native_projectm();

        // The launcher creates the user-writable projectM configuration on
        // first use.  A clean install used to race that setup and ask the
        // native renderer to open a file that did not exist yet, leaving the
        // cabinet black until a later launch. Wait for a complete config so
        // first launch behaves exactly like every subsequent launch.
        let config_file = PathBuf::from("/var/kazeta/state/projectm-home/.projectM/config.inp");
        let mut config_ready = false;
        for _ in 0..200 {
            config_ready = fs::metadata(&config_file)
                .map(|metadata| metadata.len() > 0)
                .unwrap_or(false);
            if config_ready {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        if !config_ready {
            eprintln!(
                "[Jukebox] projectM configuration was not prepared: {}",
                config_file.display()
            );
            return;
        }

        let sink = Command::new("/usr/bin/pactl")
            .args(["get-default-sink"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default();
        let monitor = if sink.is_empty() {
            String::new()
        } else {
            format!("{sink}.monitor")
        };
        let Ok(config_path) = CString::new(config_file.to_string_lossy().as_bytes()) else {
            return;
        };
        let Ok(monitor) = CString::new(monitor) else {
            return;
        };
        // Match the cabinet's physical on-screen size, capped at 1080p. This
        // is sharp on a 4K TV without doing a wasteful full-screen 4K render
        // for the much smaller cabinet opening.
        let requested_width = screen_width() * (341.0 / 640.0);
        let width = requested_width.round().clamp(640.0, 1920.0) as i32;
        let height = (width * 9 / 16).max(270);

        unsafe {
            let mut gl = macroquad::window::get_internal_gl();
            gl.flush();
            gl.quad_gl.reset();
            self.projectm_native =
                playfusion_projectm_create(config_path.as_ptr(), monitor.as_ptr(), width, height);
            gl.quad_gl.reset();
            if !self.projectm_native.is_null() {
                let raw_texture = playfusion_projectm_texture(self.projectm_native);
                if raw_texture != 0 {
                    let texture_id = macroquad::miniquad::TextureId::from_raw_id(
                        macroquad::miniquad::RawId::OpenGl(raw_texture),
                    );
                    self.projectm_texture = Some(Texture2D::from_miniquad_texture(texture_id));
                    self.projectm_warmup_started = get_time();
                }
            }
        }
    }

    fn render_native_projectm(&self) {
        if self.projectm_native.is_null() {
            return;
        }
        unsafe {
            let mut gl = macroquad::window::get_internal_gl();
            gl.flush();
            gl.quad_gl.reset();
            playfusion_projectm_render(self.projectm_native);
            gl.quad_gl.reset();
        }
    }

    fn stop_native_projectm(&mut self) {
        self.projectm_texture = None;
        if !self.projectm_native.is_null() {
            unsafe {
                let mut gl = macroquad::window::get_internal_gl();
                gl.flush();
                gl.quad_gl.reset();
                playfusion_projectm_destroy(self.projectm_native);
                gl.quad_gl.reset();
            }
            self.projectm_native = std::ptr::null_mut();
        }
        self.projectm_warmup_started = -10.0;
    }

    pub fn update(
        &mut self,
        requested_mode: MediaMode,
        input: &InputState,
        current_screen: &mut Screen,
        current_bgm: &mut Option<Sink>,
        config: &Config,
    ) {
        if self.mode != Some(requested_mode) && self.process.is_none() {
            self.start(requested_mode, current_bgm, config);
        }

        if input.back {
            self.stop_process();
            self.mode = None;
            self.error = None;
            self.resume_menu_music(current_bgm, config);
            *current_screen = match requested_mode {
                MediaMode::Dvd => Screen::Extras,
                MediaMode::Jukebox => Screen::Jukebox,
                MediaMode::Movie | MediaMode::MusicFile => self.return_screen.clone(),
            };
            return;
        }

        if let Some(process) = self.process.as_mut() {
            match process.try_wait() {
                Ok(Some(status)) => {
                    self.process = None;
                    self.stop_native_projectm();
                    if status.success() {
                        self.mode = None;
                        self.resume_menu_music(current_bgm, config);
                        *current_screen = match requested_mode {
                            MediaMode::Dvd => Screen::Extras,
                            MediaMode::Jukebox => Screen::Jukebox,
                            MediaMode::Movie | MediaMode::MusicFile => self.return_screen.clone(),
                        };
                    } else {
                        let feature = match requested_mode {
                            MediaMode::Dvd => "DVD MOVIE",
                            MediaMode::Jukebox => "MP3 JUKEBOX",
                            MediaMode::Movie => "MOVIE",
                            MediaMode::MusicFile => "MUSIC",
                        };
                        self.error = Some(format!(
                            "{feature} COULD NOT START (EXIT CODE {}).",
                            status.code().unwrap_or(-1)
                        ));
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.process = None;
                    self.error = Some(format!("MEDIA PLAYER ERROR: {error}"));
                }
            }
        }

        if requested_mode == MediaMode::Jukebox
            && !self.jukebox_fullscreen
            && self.process.is_some()
        {
            self.render_native_projectm();
        }
    }

    pub fn draw(
        &self,
        requested_mode: MediaMode,
        background_cache: &HashMap<String, Texture2D>,
        video_cache: &mut HashMap<String, VideoPlayer>,
        font_cache: &HashMap<String, Font>,
        config: &Config,
        background_state: &mut BackgroundState,
        scale_factor: f32,
    ) {
        if requested_mode == MediaMode::Jukebox && !self.jukebox_fullscreen {
            // Use a lightweight static neon field behind the cabinet. Its
            // transparent side areas remain styled without spending GPU time
            // on the normal animated menu background during playback.
            clear_background(Color::new(0.005, 0.0, 0.025, 1.0));
            draw_rectangle(
                0.0,
                0.0,
                screen_width() * 0.5,
                screen_height(),
                Color::new(0.0, 0.025, 0.11, 1.0),
            );
            draw_rectangle(
                screen_width() * 0.5,
                0.0,
                screen_width() * 0.5,
                screen_height(),
                Color::new(0.13, 0.0, 0.12, 1.0),
            );
            // The cabinet artwork uses a 640x360 design grid. Calculate both
            // axes from the live framebuffer so changing from 4K to 1080p,
            // 720p or a non-16:9 test mode cannot retain stale 4K geometry.
            let cabinet_scale_x = screen_width() / 640.0;
            let cabinet_scale_y = screen_height() / 360.0;
            draw_rectangle(
                0.0,
                0.0,
                screen_width(),
                screen_height(),
                Color::new(0.01, 0.0, 0.08, 0.72),
            );
            draw_texture_ex(
                &self.jukebox_cabinet,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    ..Default::default()
                },
            );

            let view_x = 149.0 * cabinet_scale_x;
            let view_y = 73.0 * cabinet_scale_y;
            let view_w = 341.0 * cabinet_scale_x;
            let view_h = 191.0 * cabinet_scale_y;
            let projectm_ready = get_time() - self.projectm_warmup_started >= 5.0;
            if let Some(projectm) = self.projectm_texture.as_ref().filter(|_| projectm_ready) {
                draw_texture_ex(
                    projectm,
                    view_x,
                    view_y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(view_w, view_h)),
                        flip_y: true,
                        ..Default::default()
                    },
                );
            } else {
                Self::draw_neon_cabinet_bars(view_x, view_y, view_w, view_h);
            }

            // Redraw the cabinet around the projectM opening after projectM.
            // This puts the physical cabinet, glass surround and laser rim
            // above the visualization at every output resolution.
            let cabinet_w = self.jukebox_cabinet.width();
            let cabinet_h = self.jukebox_cabinet.height();
            let overlay_regions = [
                Rect::new(0.0, 0.0, 640.0, 73.0),
                Rect::new(0.0, 264.0, 640.0, 96.0),
                Rect::new(0.0, 73.0, 149.0, 191.0),
                Rect::new(490.0, 73.0, 150.0, 191.0),
                Rect::new(142.0, 66.0, 355.0, 14.0),
                Rect::new(142.0, 257.0, 355.0, 14.0),
                Rect::new(142.0, 80.0, 14.0, 177.0),
                Rect::new(483.0, 80.0, 14.0, 177.0),
                Rect::new(142.0, 66.0, 36.0, 36.0),
                Rect::new(461.0, 66.0, 36.0, 36.0),
                Rect::new(142.0, 235.0, 36.0, 36.0),
                Rect::new(461.0, 235.0, 36.0, 36.0),
            ];
            for band in overlay_regions {
                let source = Rect::new(
                    band.x / 640.0 * cabinet_w,
                    band.y / 360.0 * cabinet_h,
                    band.w / 640.0 * cabinet_w,
                    band.h / 360.0 * cabinet_h,
                );
                draw_texture_ex(
                    &self.jukebox_cabinet,
                    band.x * cabinet_scale_x,
                    band.y * cabinet_scale_y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(band.w * cabinet_scale_x, band.h * cabinet_scale_y)),
                        source: Some(source),
                        ..Default::default()
                    },
                );
            }

            if let Some(error) = self.error.as_deref() {
                let font = get_current_font(font_cache, config);
                let body_size = (11.0 * scale_factor).max(10.0) as u16;
                let message_dims = measure_text(error, Some(font), body_size, 1.0);
                text_with_config_color(
                    font_cache,
                    config,
                    error,
                    (screen_width() - message_dims.width) * 0.5,
                    screen_height() * 0.5,
                    body_size,
                );
            }
            return;
        }
        render_background(background_cache, video_cache, config, background_state);
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.0, 0.0, 0.0, 0.82),
        );

        let title = match requested_mode {
            MediaMode::Dvd => "DVD MOVIE",
            MediaMode::Jukebox => "MP3 JUKEBOX",
            MediaMode::Movie => "MOVIE PLAYER",
            MediaMode::MusicFile => "MUSIC PLAYER",
        };
        let font = get_current_font(font_cache, config);
        let title_size = (22.0 * scale_factor) as u16;
        let body_size = (14.0 * scale_factor) as u16;
        let title_dims = measure_text(title, Some(font), title_size, 1.0);
        text_with_config_color(
            font_cache,
            config,
            title,
            (screen_width() - title_dims.width) * 0.5,
            screen_height() * 0.38,
            title_size,
        );

        let message = self.error.as_deref().unwrap_or(match requested_mode {
            MediaMode::Dvd => "STARTING DVD...  [EAST] RETURN",
            MediaMode::Jukebox => "STARTING JUKEBOX...  [EAST] RETURN",
            MediaMode::Movie => "STARTING MOVIE...  [EAST] RETURN",
            MediaMode::MusicFile => "STARTING MUSIC...  [EAST] RETURN",
        });
        let message_dims = measure_text(message, Some(font), body_size, 1.0);
        text_with_config_color(
            font_cache,
            config,
            message,
            (screen_width() - message_dims.width) * 0.5,
            screen_height() * 0.58,
            body_size,
        );
    }

    fn draw_neon_cabinet_bars(x: f32, y: f32, width: f32, height: f32) {
        draw_rectangle(x, y, width, height, Color::new(0.004, 0.0, 0.035, 1.0));

        // The original PlayFusion cabinet effect: purple/magenta equalizer
        // bars with cyan highlights. It is deliberately rendered by the menu
        // itself, so it remains fast and resolution-independent and cannot
        // escape into a separate fullscreen window.
        let time = get_time() as f32;
        let bar_count = 42usize;
        let gap = (width * 0.0045).max(1.0);
        let bar_width = (width - gap * (bar_count as f32 + 1.0)) / bar_count as f32;
        let baseline = y + height * 0.91;
        let max_height = height * 0.78;

        for index in 0..bar_count {
            let phase = index as f32 * 0.43;
            let slow = (time * 2.25 + phase).sin() * 0.5 + 0.5;
            let beat = (time * 5.6 - phase * 1.7).sin().abs();
            let shimmer = (time * 11.0 + phase * 2.3).sin() * 0.5 + 0.5;
            let center = 1.0 - (((index as f32 / (bar_count - 1) as f32) - 0.5).abs() * 1.15);
            let level = (0.12 + slow * 0.36 + beat * 0.30 + shimmer * 0.10)
                * center.clamp(0.45, 1.0);
            let bar_height = (max_height * level).clamp(height * 0.08, max_height);
            let bx = x + gap + index as f32 * (bar_width + gap);
            let by = baseline - bar_height;
            let mix = index as f32 / (bar_count - 1) as f32;
            let color = Color::new(
                0.30 + 0.70 * mix,
                0.08 + 0.18 * (1.0 - mix),
                1.0,
                0.96,
            );

            draw_rectangle(
                bx - gap * 0.65,
                by - gap,
                bar_width + gap * 1.3,
                bar_height + gap,
                Color::new(color.r, color.g, color.b, 0.13),
            );
            draw_rectangle(bx, by, bar_width, bar_height, color);
            draw_rectangle(
                bx,
                by,
                bar_width,
                (bar_height * 0.08).max(1.0),
                Color::new(0.20, 0.95, 1.0, 0.95),
            );
        }

        let sweep = y + ((time * 0.18).fract() * height);
        draw_rectangle(x, sweep, width, (height * 0.007).max(1.0), Color::new(0.2, 0.8, 1.0, 0.18));
        draw_line(x, baseline, x + width, baseline, (height * 0.008).max(1.0), Color::new(1.0, 0.1, 0.9, 0.85));
    }
}
